/**
 * Typed HTTP client for the embedded Ravyn backend.
 *
 * Errors are normalized to `ApiError` carrying the backend's stable error
 * code; network failures map to code `NETWORK_UNAVAILABLE`. Transport
 * mechanics (fetch, timeout, abort, JSON) live in `transport.ts`.
 */

import { httpRequest } from "./transport";
import type {
  BulkJobAction,
  BulkJobActionResult,
  ComponentId,
  ComponentOverview,
  CreateJob,
  FeatureSelection,
  ImportResult,
  ImportTextRequest,
  Job,
  JobActionRecord,
  JobListParams,
  JobLogRecord,
  JobOutput,
  JobPage,
  Page,
  PageQueryParams,
  PrepareLibraryResult,
  SegmentRecord,
  SetupProfile,
  SetupState,
  UpdateJob,
} from "./types";

export { ApiError } from "./errors";

export class RavynClient {
  constructor(
    readonly baseUrl: string,
    private readonly apiToken: string,
  ) {}

  private request<T>(
    method: string,
    path: string,
    body?: unknown,
    signal?: AbortSignal,
    query?: Record<string, string | number | boolean | undefined>,
    headers?: Record<string, string>,
  ): Promise<T> {
    return httpRequest<T>(this.baseUrl, this.apiToken, method, path, body, {
      signal,
      query,
      headers,
    });
  }

  // --- Setup ---

  getSetupState(signal?: AbortSignal): Promise<SetupState> {
    return this.request("GET", "/v1/setup", undefined, signal);
  }

  prepareLibrary(path: string): Promise<PrepareLibraryResult> {
    return this.request("POST", "/v1/setup/library", { path });
  }

  completeSetup(): Promise<SetupState> {
    return this.request("POST", "/v1/setup/complete");
  }

  // --- Components ---

  getComponents(signal?: AbortSignal): Promise<ComponentOverview> {
    return this.request("GET", "/v1/components", undefined, signal);
  }

  saveFeatureSelections(
    setupProfile: SetupProfile,
    features: FeatureSelection[],
  ): Promise<ComponentOverview> {
    return this.request("POST", "/v1/components/features", {
      setup_profile: setupProfile,
      features,
    });
  }

  installComponent(id: ComponentId, force = false): Promise<void> {
    return this.request("POST", `/v1/components/${wireComponentId(id)}/install`, {
      force,
    });
  }

  cancelComponentInstallation(id: ComponentId): Promise<void> {
    return this.request("POST", `/v1/components/${wireComponentId(id)}/cancel`);
  }

  // --- Jobs ---

  listJobs(params?: JobListParams, signal?: AbortSignal): Promise<JobPage> {
    return this.request("GET", "/v1/jobs", undefined, signal, { ...params });
  }

  getJob(id: string, signal?: AbortSignal): Promise<Job> {
    return this.request("GET", `/v1/jobs/${id}`, undefined, signal);
  }

  createJob(job: CreateJob, idempotencyKey?: string): Promise<Job> {
    return this.request(
      "POST",
      "/v1/jobs",
      job,
      undefined,
      undefined,
      idempotencyKey ? { "idempotency-key": idempotencyKey } : undefined,
    );
  }

  createMetalinkJob(request: {
    document: string;
    destination?: string | null;
    priority?: number;
    speed_limit_bps?: number | null;
    overwrite?: boolean;
  }): Promise<Job> {
    return this.request("POST", "/v1/jobs/metalink", request);
  }

  createBatchJobs(jobs: CreateJob[]): Promise<ImportResult> {
    return this.request("POST", "/v1/jobs/batch", jobs);
  }

  importJobsText(request: ImportTextRequest): Promise<ImportResult> {
    return this.request("POST", "/v1/jobs/import-text", request);
  }

  updateJob(id: string, request: UpdateJob): Promise<Job> {
    return this.request("PATCH", `/v1/jobs/${id}`, request);
  }

  deleteJob(id: string): Promise<void> {
    return this.request("DELETE", `/v1/jobs/${id}`);
  }

  pauseJob(id: string): Promise<void> {
    return this.request("POST", `/v1/jobs/${id}/pause`);
  }

  resumeJob(id: string): Promise<void> {
    return this.request("POST", `/v1/jobs/${id}/resume`);
  }

  cancelJob(id: string): Promise<void> {
    return this.request("POST", `/v1/jobs/${id}/cancel`);
  }

  retryJob(id: string): Promise<void> {
    return this.request("POST", `/v1/jobs/${id}/retry`);
  }

  applyJobAction(action: BulkJobAction, ids: string[]): Promise<BulkJobActionResult[]> {
    return this.request("POST", "/v1/jobs/actions", { action, ids });
  }

  listJobOutputs(id: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<JobOutput>> {
    return this.request("GET", `/v1/jobs/${id}/outputs`, undefined, signal, { ...params });
  }

  listJobSegments(id: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<SegmentRecord>> {
    return this.request("GET", `/v1/jobs/${id}/segments`, undefined, signal, { ...params });
  }

  listJobActions(id: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<JobActionRecord>> {
    return this.request("GET", `/v1/jobs/${id}/actions`, undefined, signal, { ...params });
  }

  listJobLogs(id: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<JobLogRecord>> {
    return this.request("GET", `/v1/jobs/${id}/logs`, undefined, signal, { ...params });
  }
}

/** Route path segment for a component id (differs from the JSON enum). */
function wireComponentId(id: ComponentId): string {
  switch (id) {
    case "ytdlp":
      return "yt-dlp";
    case "seven_zip":
      return "7zip";
    default:
      return id;
  }
}
