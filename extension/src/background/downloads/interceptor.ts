import type {
  CreateDownloadPayload,
  ExtensionSettings,
} from "../../shared/contracts";
import { logger } from "../../shared/logger";
import { notify } from "../notifications";
import { loadSettings } from "../../shared/settings";
import { domainMatches } from "../../shared/urls";
import { downloadLabel, trackDownload } from "./completion";
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

  constructor(
    private readonly native: NativeClient,
    private readonly rules: RuleCache,
    private readonly delegated: DelegationRegistry,
    private readonly bypass: BypassRegistry,
  ) {}

  register(): void {
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
    // The user held the bypass modifier (Alt, by default) while clicking
    // this link — let Firefox handle it untouched, IDM-style escape hatch.
    if (await this.bypass.consume(item.url)) return;
    // Pause before any async eligibility/rule lookups (settings, delegation
    // cache, rule cache all round-trip through browser.storage). Otherwise
    // Firefox keeps transferring — and can finish a small file outright —
    // before we ever get around to pausing it, so the shelf visibly shows
    // the download start and then vanish once we hand it off and cancel it.
    await browser.downloads.pause(item.id).catch(() => undefined);
    await markPending(item.id);
    let handedOff = false;
    let claimed = false;
    let settings: ExtensionSettings | undefined;
    try {
      settings = await loadSettings();
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
      // A second `onCreated` for the same URL (double-click, or a page
      // firing near-simultaneous requests for one resource) racing this one
      // through the async checks above would otherwise both pass `contains`
      // (neither has recorded a delegation yet) and both hand off — claim
      // the URL now, right before the point of no return.
      claimed = await this.delegated.claim(item.url);
      if (!claimed) return;

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
      await trackDownload(result.id, downloadLabel(payload));
      handedOff = true;
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
      if (settings?.notifications)
        await notify(
          "Ravyn handoff failed",
          "Firefox is continuing the download.",
        );
    } finally {
      if (claimed) await this.delegated.release(item.url);
      if (!handedOff)
        await browser.downloads.resume(item.id).catch(() => undefined);
      await clearPending(item.id);
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
