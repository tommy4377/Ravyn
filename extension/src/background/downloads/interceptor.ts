import type { ExtensionSettings } from "../../shared/contracts";
import { logger } from "../../shared/logger";
import { notify } from "../notifications";
import {
  DEFAULT_SETTINGS,
  SETTINGS_KEY,
  loadSettings,
  sanitizeSettings,
} from "../../shared/settings";
import { domainMatches } from "../../shared/urls";
import { downloadLabel, trackDownload } from "./completion";
import { enrichDownload } from "./browser-context";
import { evaluateEligibility, type DownloadCandidate } from "./eligibility";
import type { BypassRegistry } from "./bypass";
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

// Persisted (survives MV3 event-page suspension/restart) record of download
// ids Ravyn itself paused pending a handoff decision. Used to scope the
// startup orphan-sweep to downloads WE paused — not ones the user paused
// manually for their own reasons, which happen to be indistinguishable from
// ours through the plain `browser.downloads` state alone.
const PENDING_PAUSE_KEY = "ravyn.pendingPausedDownloadIds";

// Serializes every read-modify-write of the persisted pending set. Two
// concurrent handoffs (a page dropping several downloads at once) would
// otherwise interleave get/set on the same key and silently drop an id —
// the exact record the startup orphan-sweep needs to resume that download.
let pendingIdsQueue: Promise<unknown> = Promise.resolve();

function withPendingIds<T>(
  mutate: (ids: Set<number>) => T | Promise<T>,
): Promise<T> {
  const run = pendingIdsQueue.then(async () => {
    const stored = await browser.storage.local.get(PENDING_PAUSE_KEY);
    const ids = new Set<number>(
      (stored[PENDING_PAUSE_KEY] as number[] | undefined) ?? [],
    );
    const result = await mutate(ids);
    await browser.storage.local.set({ [PENDING_PAUSE_KEY]: [...ids] });
    return result;
  });
  pendingIdsQueue = run.catch(() => undefined);
  return run;
}

async function markPending(id: number): Promise<void> {
  await withPendingIds((ids) => ids.add(id));
}

async function clearPending(id: number): Promise<void> {
  await withPendingIds((ids) => ids.delete(id));
}

export class DownloadInterceptor {
  private confirmations = new Map<string, ConfirmationRequest>();
  // Serializes confirmation popups so a page dropping several downloads at
  // once doesn't open overlapping windows — one confirmation at a time.
  private confirmLock: Promise<void> = Promise.resolve();
  private settingsSnapshot: ExtensionSettings | null = null;

  constructor(
    private readonly native: NativeClient,
    private readonly rules: RuleCache,
    private readonly delegated: DelegationRegistry,
    private readonly bypass: BypassRegistry,
  ) {}

  register(initialSettings?: ExtensionSettings): void {
    this.settingsSnapshot = initialSettings ?? null;
    browser.storage.onChanged.addListener((changes, areaName) => {
      if (areaName !== "local" || !(SETTINGS_KEY in changes)) return;
      const stored = changes[SETTINGS_KEY]?.newValue as
        Partial<ExtensionSettings> | undefined;
      this.settingsSnapshot = sanitizeSettings({
        ...DEFAULT_SETTINGS,
        ...stored,
      });
    });
    browser.downloads.onCreated.addListener((item) => {
      void this.handle(item).catch((error) =>
        logger.error("Download interception failed", error),
      );
    });
    // A confirmation promise lost to background-page suspension (or a crash)
    // would otherwise leave the browser download paused forever — resume
    // anything paused that we have no live confirmation tracking for.
    void this.resumeOrphanedPausedDownloads();
  }

  private async resumeOrphanedPausedDownloads(): Promise<void> {
    // Any in-memory confirmation state is necessarily gone the moment this
    // runs (we just started). Only resume downloads WE paused (tracked in
    // persisted storage, see markPending/clearPending) — a plain
    // `{paused:true}` search can't distinguish an interrupted handoff or
    // orphaned confirmation from a download the user paused manually for
    // their own reasons, and force-resuming the latter would be wrong.
    await withPendingIds(async (ids) => {
      if (ids.size === 0) return;
      const paused = await browser.downloads
        .search({ paused: true, state: "in_progress" })
        .catch(() => []);
      const stillPaused = new Set(paused.map((item) => item.id));
      for (const id of ids) {
        if (stillPaused.has(id))
          await browser.downloads.resume(id).catch(() => undefined);
      }
      // Every tracked id is now resolved (resumed here, or no longer paused —
      // completed/cancelled/removed since it was recorded) — drop them all so
      // the persisted set doesn't grow across restarts.
      ids.clear();
    }).catch(() => undefined);
  }

  resolveConfirmation(requestId: string, accepted: boolean): void {
    const confirmation = this.confirmations.get(requestId);
    if (!confirmation) return;
    window.clearTimeout(confirmation.timer);
    this.confirmations.delete(requestId);
    confirmation.resolve(accepted);
  }

  private async handle(item: browser.downloads.DownloadItem): Promise<void> {
    if (await this.bypass.consume(item.url)) return;

    const settings = this.settingsSnapshot ?? (await loadSettings());
    this.settingsSnapshot = settings;
    if (
      !settings.automaticInterception ||
      settings.interceptionMode === "disabled"
    )
      return;

    let pendingRecorded = false;
    let browserPaused = false;
    let handedOff = false;
    let claimed = false;
    let ravynJobId: string | undefined;
    try {
      // Persist ownership intent before pausing Firefox. If storage is
      // unavailable we leave the browser download untouched instead of
      // creating an orphaned paused download that startup recovery cannot see.
      await markPending(item.id);
      pendingRecorded = true;
      await browser.downloads.pause(item.id);
      browserPaused = true;

      const candidate = candidateFrom(item);
      const eligibility = evaluateEligibility(
        candidate,
        settings,
        browser.runtime.id,
      );
      if (!eligibility.eligible) return;
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

      // Firefox download ids identify one concrete browser download. Using the
      // id rather than a two-minute URL cache allows intentional repeated
      // downloads of the same URL while still preventing one event from being
      // handed off twice concurrently.
      claimed = this.delegated.claim(item.id);
      if (!claimed) return;

      const payload = await enrichDownload({
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
      });
      const result = await this.native.request<{ id: string }>(
        "create_download",
        payload,
      );
      ravynJobId = result.id;
      await trackDownload(result.id, downloadLabel(payload));

      try {
        await browser.downloads.cancel(item.id);
      } catch (error) {
        // Ravyn only owns the transfer once Firefox has relinquished it. Roll
        // back the newly-created Ravyn job if browser cancellation fails so the
        // same bytes cannot continue downloading in both applications.
        await this.native
          .request("cancel_job", { id: result.id })
          .catch(() => undefined);
        throw error;
      }
      handedOff = true;
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
      if (ravynJobId && !handedOff) {
        await this.native
          .request("cancel_job", { id: ravynJobId })
          .catch(() => undefined);
      }
      if (settings.notifications)
        await notify(
          "Ravyn handoff failed",
          "Firefox is continuing the download.",
        );
    } finally {
      if (claimed) this.delegated.release(item.id);
      if (!handedOff && browserPaused)
        await browser.downloads.resume(item.id).catch(() => undefined);
      if (pendingRecorded) await clearPending(item.id).catch(() => undefined);
    }
  }

  // Queues confirmation dialogs so several downloads landing at once don't
  // pop overlapping windows — the caller (handle()) keeps the download
  // paused for this entire wait and resumes it only if the user declines.
  private async confirm(
    item: browser.downloads.DownloadItem,
  ): Promise<boolean> {
    let release!: () => void;
    const previous = this.confirmLock;
    this.confirmLock = new Promise<void>((resolve) => (release = resolve));
    await previous;
    try {
      return await this.showConfirmationDialog(item);
    } finally {
      release();
    }
  }

  private async showConfirmationDialog(
    item: browser.downloads.DownloadItem,
  ): Promise<boolean> {
    const id = crypto.randomUUID();
    const url = browser.runtime.getURL(
      `confirmation/index.html?id=${encodeURIComponent(id)}&filename=${encodeURIComponent(filenameHint(item.filename) ?? item.url)}&url=${encodeURIComponent(item.url)}`,
    );
    await browser.windows.create({
      url,
      type: "popup",
      width: 460,
      height: 310,
    });
    return new Promise<boolean>((resolve) => {
      const timer = window.setTimeout(() => {
        this.confirmations.delete(id);
        resolve(false);
      }, 30_000);
      this.confirmations.set(id, { id, resolve, timer });
    });
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
    totalBytes: item.totalBytes >= 0 ? item.totalBytes : undefined,
  };
}

function filenameHint(path: string | undefined): string | undefined {
  if (!path) return undefined;
  return path.replace(/\\/g, "/").split("/").pop()?.slice(0, 255);
}
