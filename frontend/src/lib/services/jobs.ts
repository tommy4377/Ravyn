/**
 * Job-domain service: the only path components/stores use to reach the
 * jobs routes. Wraps `RavynClient` with UI-oriented composition (e.g.
 * choosing single-job create vs. batch text import) so screens never
 * encode backend routing decisions themselves.
 */

import type { RavynClient } from "../api/client";
import type {
  BulkJobAction,
  BulkJobActionResult,
  CreateJob,
  DownloadOptions,
  DuplicatePolicy,
  Job,
  JobActionRecord,
  JobKind,
  JobListParams,
  JobLogRecord,
  MediaOptions,
  JobOutput,
  JobPage,
  Page,
  PageQueryParams,
  SegmentRecord,
  TorrentOptions,
  UpdateJob,
} from "../api/types";

/** Everything the Add Download dialog can collect from the user. */
export interface AddDownloadInput {
  /** One or more URLs, one per line. */
  source: string;
  destination?: string;
  filename?: string;
  priority?: number;
  speedLimitBps?: number;
  expectedSha256?: string;
  duplicatePolicy?: DuplicatePolicy;
  tags?: string[];
  headers?: Record<string, string>;
  userAgent?: string;
  referer?: string;
  proxy?: string;
  media?: MediaOptions;
  torrent?: TorrentOptions;
}

export interface AddDownloadResult {
  requestedCount: number;
  createdCount: number;
  rejectedCount: number;
  jobs: Job[];
  errors: { source: string; message: string }[];
}

function splitLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#") && !line.startsWith("//"));
}

function buildOptions(input: AddDownloadInput): DownloadOptions {
  const options: DownloadOptions = {};
  if (input.tags?.length) options.tags = input.tags;
  if (input.headers && Object.keys(input.headers).length) options.headers = input.headers;
  if (input.userAgent) options.user_agent = input.userAgent;
  if (input.referer) options.referer = input.referer;
  if (input.proxy) options.proxy = input.proxy;
  if (input.media) options.media = input.media;
  if (input.torrent) options.torrent = input.torrent;
  return options;
}

export class JobsService {
  constructor(private readonly client: RavynClient) {}

  list(params?: JobListParams, signal?: AbortSignal): Promise<JobPage> {
    return this.client.listJobs(params, signal);
  }

  get(id: string, signal?: AbortSignal): Promise<Job> {
    return this.client.getJob(id, signal);
  }

  update(id: string, request: UpdateJob): Promise<Job> {
    return this.client.updateJob(id, request);
  }

  remove(id: string): Promise<void> {
    return this.client.deleteJob(id);
  }

  pause(id: string): Promise<void> {
    return this.client.pauseJob(id);
  }

  resume(id: string): Promise<void> {
    return this.client.resumeJob(id);
  }

  cancel(id: string): Promise<void> {
    return this.client.cancelJob(id);
  }

  retry(id: string): Promise<void> {
    return this.client.retryJob(id);
  }

  bulkAction(action: BulkJobAction, ids: string[]): Promise<BulkJobActionResult[]> {
    return this.client.applyJobAction(action, ids);
  }

  outputs(id: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<JobOutput>> {
    return this.client.listJobOutputs(id, params, signal);
  }

  segments(id: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<SegmentRecord>> {
    return this.client.listJobSegments(id, params, signal);
  }

  actions(id: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<JobActionRecord>> {
    return this.client.listJobActions(id, params, signal);
  }

  logs(id: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<JobLogRecord>> {
    return this.client.listJobLogs(id, params, signal);
  }

  /**
   * Add one or more downloads from pasted/typed text. A single non-empty
   * line creates one job directly (so per-job advanced options apply in
   * full); multiple lines go through the batch text-import route, which
   * applies the same options as shared defaults and reports per-line
   * duplicates/rejections instead of failing the whole request.
   */
  async addFromInput(input: AddDownloadInput, kind: JobKind = "http"): Promise<AddDownloadResult> {
    const lines = splitLines(input.source);
    if (lines.length === 0) {
      return { requestedCount: 0, createdCount: 0, rejectedCount: 0, jobs: [], errors: [] };
    }

    if (lines.length === 1) {
      const source = lines[0]!;
      const request: CreateJob = {
        kind,
        source,
        destination: input.destination || undefined,
        filename: input.filename || undefined,
        priority: input.priority ?? 0,
        speed_limit_bps: input.speedLimitBps,
        expected_sha256: input.expectedSha256 || undefined,
        duplicate_policy: input.duplicatePolicy ?? "allow",
        options: buildOptions(input),
      };
      try {
        const job = await this.client.createJob(request);
        return { requestedCount: 1, createdCount: 1, rejectedCount: 0, jobs: [job], errors: [] };
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        return { requestedCount: 1, createdCount: 0, rejectedCount: 1, jobs: [], errors: [{ source, message }] };
      }
    }

    const result = await this.client.importJobsText({
      text: lines.join("\n"),
      defaults: {
        kind,
        destination: input.destination || undefined,
        priority: input.priority ?? 0,
        speed_limit_bps: input.speedLimitBps,
        duplicate_policy: input.duplicatePolicy ?? "allow",
        options: buildOptions(input),
      },
    });
    const jobs = result.items.map((item) => item.job).filter((job): job is Job => job !== null);
    const errors = result.items
      .filter((item) => item.error)
      .map((item) => ({ source: item.source, message: item.error as string }));
    return {
      requestedCount: lines.length,
      createdCount: result.accepted,
      rejectedCount: result.rejected,
      jobs,
      errors,
    };
  }
}
