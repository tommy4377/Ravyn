import type { CreateDownloadPayload } from "../../shared/contracts";
import { logger } from "../../shared/logger";
import { loadSettings } from "../../shared/settings";
import { domainMatches } from "../../shared/urls";
import { evaluateEligibility, type DownloadCandidate } from "./eligibility";
import type { DelegationRegistry } from "./delegation";
import type { NativeClient } from "../native/client";
import type { RuleCache } from "../rules/cache";
import { evaluateRules } from "../rules/evaluator";
import { decideInterception } from "./state-machine";

interface ConfirmationRequest {
  id: string;
  resolve(value: boolean): void;
  timer: number;
}

export class DownloadInterceptor {
  private confirmations = new Map<string, ConfirmationRequest>();

  constructor(
    private readonly native: NativeClient,
    private readonly rules: RuleCache,
    private readonly delegated: DelegationRegistry,
  ) {}

  register(): void {
    browser.downloads.onCreated.addListener((item) => {
      void this.handle(item).catch((error) =>
        logger.error("Download interception failed", error),
      );
    });
  }

  resolveConfirmation(requestId: string, accepted: boolean): void {
    const confirmation = this.confirmations.get(requestId);
    if (!confirmation) return;
    window.clearTimeout(confirmation.timer);
    this.confirmations.delete(requestId);
    confirmation.resolve(accepted);
  }

  private async handle(item: browser.downloads.DownloadItem): Promise<void> {
    const settings = await loadSettings();
    const candidate = candidateFrom(item);
    const eligibility = evaluateEligibility(
      candidate,
      settings,
      browser.runtime.id,
    );
    if (!eligibility.eligible || (await this.delegated.contains(item.url)))
      return;
    const rule = evaluateRules(await this.rules.get(), {
      url: item.url,
      mime: item.mime,
      extension: eligibility.extension,
    });
    const forced = eligibility.host
      ? settings.alwaysInterceptDomains.some((pattern) =>
          domainMatches(pattern, eligibility.host!),
        )
      : false;
    const decision = decideInterception(settings, rule?.action, forced);
    if (decision === "ignore") return;
    if (decision === "confirm" && !(await this.confirm(item))) return;

    let paused = false;
    try {
      await browser.downloads.pause(item.id);
      paused = true;
      const payload: CreateDownloadPayload = {
        url: item.url,
        kind: "http",
        filename: filenameHint(item.filename),
        referer: candidate.referrer,
        presetId: rule?.presetId,
        idempotencyKey: `firefox-download-${item.id}-${item.startTime ?? Date.now()}`,
        sourceContext: {
          browser: "firefox",
          incognito: item.incognito,
          pageUrl: candidate.referrer,
        },
      };
      const result = await this.native.request<{ id: string }>(
        "create_download",
        payload,
      );
      await this.delegated.remember(item.url, result.id);
      await browser.downloads.cancel(item.id);
      await browser.downloads.removeFile(item.id).catch(() => undefined);
      if (settings.eraseDelegatedBrowserEntries)
        await browser.downloads.erase({ id: item.id }).catch(() => undefined);
      if (settings.notifications)
        await notify(
          "Download sent to Ravyn",
          payload.filename ?? new URL(payload.url).hostname,
        );
    } catch (error) {
      logger.warn(
        "Ravyn handoff failed; Firefox will continue the download",
        error,
      );
      if (paused)
        await browser.downloads.resume(item.id).catch(() => undefined);
      if (settings.notifications)
        await notify(
          "Ravyn handoff failed",
          "Firefox is continuing the download.",
        );
    }
  }

  private async confirm(
    item: browser.downloads.DownloadItem,
  ): Promise<boolean> {
    const id = crypto.randomUUID();
    const url = browser.runtime.getURL(
      `confirmation/index.html?id=${encodeURIComponent(id)}&filename=${encodeURIComponent(filenameHint(item.filename) ?? item.url)}&url=${encodeURIComponent(item.url)}`,
    );
    await browser.downloads.pause(item.id).catch(() => undefined);
    await browser.windows.create({
      url,
      type: "popup",
      width: 460,
      height: 310,
    });
    const accepted = await new Promise<boolean>((resolve) => {
      const timer = window.setTimeout(() => {
        this.confirmations.delete(id);
        resolve(false);
      }, 30_000);
      this.confirmations.set(id, { id, resolve, timer });
    });
    await browser.downloads.resume(item.id).catch(() => undefined);
    return accepted;
  }
}

function candidateFrom(
  item: browser.downloads.DownloadItem,
): DownloadCandidate {
  const extended = item as browser.downloads.DownloadItem & {
    byExtensionId?: string;
    referrer?: string;
    method?: string;
  };
  return {
    id: item.id,
    url: item.url,
    filename: item.filename,
    mime: item.mime,
    referrer: extended.referrer,
    incognito: item.incognito,
    byExtensionId: extended.byExtensionId,
    method: extended.method,
  };
}

function filenameHint(path: string | undefined): string | undefined {
  if (!path) return undefined;
  return path.replace(/\\/g, "/").split("/").pop()?.slice(0, 255);
}

async function notify(title: string, message: string): Promise<void> {
  await browser.notifications.create({
    type: "basic",
    iconUrl: browser.runtime.getURL("icons/ravyn-96.png"),
    title,
    message,
  });
}
