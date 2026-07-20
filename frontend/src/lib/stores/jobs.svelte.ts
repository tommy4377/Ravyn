/**
 * Normalized job collection, kept in sync via paginated fetches and live
 * SSE events. Downloads/{Active,Queued,Completed,Failed,All} are derived
 * filters over this single collection, not separate data silos (per the
 * frontend design plan) — there is no "Scheduled" view yet because the
 * `Job` model carries no schedule linkage until the Automation slice lands.
 *
 * Progress events are coalesced to ~10 Hz total (not per entity) via one
 * shared flush timer, so a burst of concurrent downloads can't flood
 * reactivity or accessibility announcements.
 */

import { SvelteMap } from "svelte/reactivity";
import { describeError } from "../api/errors";
import type {
  Job,
  JobKind,
  JobListParams,
  JobSummary,
  JobStatus,
  JobStatusEvent,
  ProgressEvent,
  RavynEvent,
} from "../api/types";
import type { JobsService } from "../services/jobs";

export type JobView = "all" | "active" | "queued" | "completed" | "failed";

const ACTIVE_STATUSES = new Set<JobStatus>([
  "probing",
  "downloading",
  "paused",
  "verifying",
  "post_processing",
  "seeding",
]);
const COMPLETED_STATUSES = new Set<JobStatus>(["completed", "partial"]);
const FAILED_STATUSES = new Set<JobStatus>(["failed", "cancelled"]);

export interface LiveProgress {
  downloadedBytes: number;
  totalBytes: number | null;
  bytesPerSecond: number;
  updatedAt: number;
}

const PAGE_SIZE = 100;
const COALESCE_INTERVAL_MS = 100; // ~10 Hz

export class JobsStore {
  private service: JobsService | null = null;
  private flushTimer: ReturnType<typeof setInterval> | null = null;
  private summaryTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly pendingProgress = new Map<string, ProgressEvent>();
  private loadAbort: AbortController | null = null;
  private lastParams: JobListParams = {};
  private queryGeneration = 0;
  private summaryGeneration = 0;

  readonly byId = new SvelteMap<string, Job>();
  readonly liveProgress = new SvelteMap<string, LiveProgress>();
  order = $state<string[]>([]);
  status = $state<"idle" | "loading" | "error">("idle");
  errorMessage = $state<string | null>(null);
  loadingMore = $state(false);
  nextCursor = $state<string | null>(null);
  hasLoadedOnce = $state(false);
  searchTerm = $state("");
  summary = $state<JobSummary | null>(null);

  init(service: JobsService): void {
    this.service = service;
    if (!this.flushTimer) {
      this.flushTimer = setInterval(() => this.flushProgress(), COALESCE_INTERVAL_MS);
    }
  }

  dispose(): void {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = null;
    }
    this.loadAbort?.abort();
    this.summaryGeneration += 1;
    if (this.summaryTimer) {
      clearTimeout(this.summaryTimer);
      this.summaryTimer = null;
    }
  }

  private flushProgress(): void {
    if (this.pendingProgress.size === 0) return;
    for (const [jobId, snapshot] of this.pendingProgress) {
      // A progress event for a job removed moments earlier (removeLocal
      // clears liveProgress, but a packet already in flight lands on the
      // next flush tick regardless) would otherwise resurrect an entry for
      // a job that no longer exists in byId, left until the next removal or
      // full reload.
      if (!this.byId.has(jobId)) continue;
      this.liveProgress.set(jobId, {
        downloadedBytes: snapshot.downloaded_bytes,
        totalBytes: snapshot.total_bytes,
        bytesPerSecond: snapshot.bytes_per_second,
        updatedAt: Date.now(),
      });
    }
    this.pendingProgress.clear();
  }

  async loadInitial(params: JobListParams = {}): Promise<void> {
    if (!this.service) return;
    this.loadAbort?.abort();
    const abort = new AbortController();
    const generation = ++this.queryGeneration;
    const summaryGeneration = ++this.summaryGeneration;
    this.loadAbort = abort;
    this.lastParams = params;
    this.searchTerm = params.search ?? "";
    this.status = "loading";
    this.errorMessage = null;
    try {
      const [page, summary] = await Promise.all([
        this.service.list({ limit: PAGE_SIZE, ...params }, abort.signal),
        this.service.summary(abort.signal).catch(() => null),
      ]);
      if (abort.signal.aborted || generation !== this.queryGeneration) return;
      if (summary && summaryGeneration === this.summaryGeneration)
        this.summary = summary;
      this.byId.clear();
      this.liveProgress.clear();
      for (const job of page.items) this.byId.set(job.id, job);
      this.order = page.items.map((job) => job.id);
      this.nextCursor = page.next_cursor;
      this.status = "idle";
      this.hasLoadedOnce = true;
    } catch (error) {
      if (abort.signal.aborted) return;
      this.status = "error";
      this.errorMessage = describeError(error);
    }
  }

  async refreshSummary(): Promise<void> {
    if (!this.service) return;
    const generation = ++this.summaryGeneration;
    const summary = await this.service.summary().catch(() => null);
    if (summary && generation === this.summaryGeneration) this.summary = summary;
  }

  private scheduleSummaryRefresh(): void {
    if (this.summaryTimer) return;
    this.summaryTimer = setTimeout(() => {
      this.summaryTimer = null;
      void this.refreshSummary();
    }, 500);
  }

  /** Re-run the most recent query (used after resync/queue-changed events). */
  refreshAll(): void {
    void this.loadInitial(this.lastParams);
  }

  async loadMore(): Promise<void> {
    if (!this.service || !this.nextCursor || this.loadingMore) return;
    const generation = this.queryGeneration;
    const cursor = this.nextCursor;
    const signal = this.loadAbort?.signal;
    this.loadingMore = true;
    try {
      const page = await this.service.list(
        { ...this.lastParams, limit: PAGE_SIZE, cursor },
        signal,
      );
      if (signal?.aborted || generation !== this.queryGeneration) return;
      for (const job of page.items) {
        this.byId.set(job.id, job);
        if (!this.order.includes(job.id) && this.matchesCurrentQuery(job))
          this.order.push(job.id);
      }
      this.nextCursor = page.next_cursor;
    } catch (error) {
      if (!signal?.aborted && generation === this.queryGeneration)
        this.errorMessage = describeError(error);
    } finally {
      if (generation === this.queryGeneration) this.loadingMore = false;
    }
  }

  private async refetchJob(id: string): Promise<void> {
    if (!this.service) return;
    const generation = this.queryGeneration;
    try {
      const job = await this.service.get(id);
      if (generation !== this.queryGeneration) return;
      this.upsert(job);
    } catch {
      if (generation === this.queryGeneration) this.removeLocal(id);
    }
  }

  /** Updates the normalized entity cache without changing the current query result ids. */
  cacheEntity(job: Job): void {
    this.byId.set(job.id, job);
  }

  upsert(job: Job): void {
    this.byId.set(job.id, job);
    const included = this.order.includes(job.id);
    const matches = this.matchesCurrentQuery(job);
    if (matches && !included) this.order.unshift(job.id);
    if (!matches && included) this.removeFromQuery(job.id);
  }

  private removeFromQuery(id: string): void {
    const index = this.order.indexOf(id);
    if (index !== -1) this.order.splice(index, 1);
  }

  private matchesCurrentQuery(job: Job): boolean {
    if (this.lastParams.kind && job.kind !== this.lastParams.kind) return false;
    if (this.lastParams.status && job.status !== this.lastParams.status) return false;
    const search = this.lastParams.search?.trim().toLowerCase();
    if (search) {
      const haystack = `${job.filename ?? ""} ${job.source} ${job.status}`.toLowerCase();
      if (!haystack.includes(search)) return false;
    }
    return true;
  }

  removeLocal(id: string): void {
    this.byId.delete(id);
    this.liveProgress.delete(id);
    const index = this.order.indexOf(id);
    if (index !== -1) this.order.splice(index, 1);
  }

  applyEvent(event: RavynEvent): void {
    switch (event.type) {
      case "job_status": {
        const e = event as JobStatusEvent;
        const job = this.byId.get(e.job_id);
        if (job) {
          this.upsert({ ...job, status: e.status, error: e.error });
        } else {
          void this.refetchJob(e.job_id);
        }
        this.scheduleSummaryRefresh();
        break;
      }
      case "progress": {
        const e = event as ProgressEvent;
        this.pendingProgress.set(e.job_id, e);
        this.scheduleSummaryRefresh();
        break;
      }
      case "queue_changed":
      case "resync_required": {
        this.refreshAll();
        break;
      }
      default:
        break;
    }
  }

  get list(): Job[] {
    return this.order
      .map((id) => this.byId.get(id))
      .filter((job): job is Job => job !== undefined);
  }

  jobsFor(view: JobView): Job[] {
    const jobs = this.list;
    switch (view) {
      case "active":
        return jobs.filter((job) => ACTIVE_STATUSES.has(job.status));
      case "queued":
        return jobs.filter((job) => job.status === "queued");
      case "completed":
        return jobs.filter((job) => COMPLETED_STATUSES.has(job.status));
      case "failed":
        return jobs.filter((job) => FAILED_STATUSES.has(job.status));
      default:
        return jobs;
    }
  }

  /** Sets an ephemeral status filter (used by the status dropdown) alongside the current search term. */
  setFilters(filters: { status?: JobStatus; kind?: JobKind }): void {
    void this.loadInitial({ ...this.lastParams, search: this.searchTerm || undefined, ...filters });
  }
}

export const jobsStore = new JobsStore();
