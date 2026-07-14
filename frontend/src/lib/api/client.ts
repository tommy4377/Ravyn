/**
 * Typed HTTP client for the embedded Ravyn backend.
 *
 * Errors are normalized to `ApiError` carrying the backend's stable error
 * code; network failures map to code `NETWORK_UNAVAILABLE`. Transport
 * mechanics (fetch, timeout, abort, JSON) live in `transport.ts`.
 */

import { httpRequest } from "./transport";
import type {
  AuditChainStatus,
  AuditRecord,
  AutomationRule,
  BackupRecord,
  BackupVerification,
  BasketItem,
  BasketStartResult,
  BulkJobAction,
  BulkJobActionResult,
  ComponentHealth,
  ComponentId,
  ComponentManifestStatus,
  ComponentOverview,
  CleanupPolicies,
  CleanupReport,
  CreateJob,
  DatabaseStatus,
  DependenciesStatus,
  DownloadPreset,
  DuplicateCandidate,
  EngineCleanupReport,
  FeatureSelection,
  ImportResult,
  ImportTextRequest,
  Job,
  JobActionRecord,
  JobListParams,
  JobLogRecord,
  JobOutput,
  JobPage,
  HostProfile,
  LibraryEntry,
  LibraryImportRequest,
  LibraryImportStatus,
  LibraryListParams,
  LibraryRelocationRequest,
  MediaArchiveRecord,
  MediaItemOutputRecord,
  MediaItemRecord,
  MediaItemRetryResult,
  MediaItemSummary,
  MediaProbe,
  MediaProbeRequest,
  Page,
  PageQueryParams,
  PersistentSettingsPatch,
  PutDownloadPreset,
  PutUserProfile,
  PrepareLibraryResult,
  ReadinessStatus,
  SecretReference,
  PutSecretRequest,
  ReportInstallationRequest,
  RestoreStatus,
  RetryFailedMediaItemsResponse,
  RelocationReport,
  RuleInput,
  RulePreview,
  RulePreviewRequest,
  ScheduleExecutionRecord,
  ScheduleInput,
  ScheduleRecord,
  SaveIntegrationConsentRequest,
  SegmentRecord,
  SettingsResponse,
  SettingsValidationResponse,
  SetupProfile,
  SetupState,
  SystemCapabilities,
  TagRecord,
  TemplatePreview,
  TemplatePreviewRequest,
  TorrentDetails,
  TorrentDhtStats,
  TorrentDhtTable,
  TorrentEngineList,
  TorrentGlobalStats,
  TorrentPeerStats,
  TorrentProbe,
  TorrentProbeRequest,
  TorrentRecord,
  TorrentSeedingState,
  TorrentSnapshot,
  TrustPreviewRequest,
  TrustReport,
  UpdateJob,
  UserProfile,
  ActivateProfileResponse,
  VerifyLibraryReport,
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

  saveIntegrationConsent(
    request: SaveIntegrationConsentRequest,
  ): Promise<SetupState> {
    return this.request("POST", "/v1/setup/integration-consent", request);
  }

  reportInstallation(request: ReportInstallationRequest): Promise<SetupState> {
    return this.request("POST", "/v1/setup/installation", request);
  }

  completeSetup(): Promise<SetupState> {
    return this.request("POST", "/v1/setup/complete");
  }

  // --- Components ---

  getComponents(signal?: AbortSignal): Promise<ComponentOverview> {
    return this.request("GET", "/v1/components", undefined, signal);
  }

  getComponentManifestStatus(signal?: AbortSignal): Promise<ComponentManifestStatus> {
    return this.request("GET", "/v1/components/manifest", undefined, signal);
  }

  refreshComponentManifest(): Promise<ComponentManifestStatus> {
    return this.request("POST", "/v1/components/manifest");
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

  updateComponent(id: ComponentId): Promise<void> {
    return this.request("POST", `/v1/components/${wireComponentId(id)}/update`);
  }

  verifyComponent(id: ComponentId): Promise<ComponentHealth> {
    return this.request("POST", `/v1/components/${wireComponentId(id)}/verify`);
  }

  rollbackComponent(id: ComponentId): Promise<void> {
    return this.request("POST", `/v1/components/${wireComponentId(id)}/rollback`);
  }

  removeComponent(id: ComponentId): Promise<void> {
    return this.request("DELETE", `/v1/components/${wireComponentId(id)}`);
  }

  cleanupComponent(id: ComponentId): Promise<EngineCleanupReport> {
    return this.request("POST", `/v1/components/${wireComponentId(id)}/cleanup`);
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

  // --- Tags ---

  listTags(params?: PageQueryParams, signal?: AbortSignal): Promise<Page<TagRecord>> {
    return this.request("GET", "/v1/tags", undefined, signal, { ...params });
  }

  deleteTag(id: number): Promise<void> {
    return this.request("DELETE", `/v1/tags/${id}`);
  }

  getJobTags(id: string, signal?: AbortSignal): Promise<string[]> {
    return this.request("GET", `/v1/jobs/${id}/tags`, undefined, signal);
  }

  replaceJobTags(id: string, tags: string[]): Promise<string[]> {
    return this.request("PUT", `/v1/jobs/${id}/tags`, { tags });
  }

  // --- Trust and previews ---

  previewTrust(request: TrustPreviewRequest): Promise<TrustReport> {
    return this.request("POST", "/v1/trust/preview", request);
  }

  getJobTrust(id: string, signal?: AbortSignal): Promise<TrustReport> {
    return this.request("GET", `/v1/jobs/${id}/trust`, undefined, signal);
  }

  previewTemplate(request: TemplatePreviewRequest): Promise<TemplatePreview> {
    return this.request("POST", "/v1/templates/preview", request);
  }

  previewRules(request: RulePreviewRequest): Promise<RulePreview> {
    return this.request("POST", "/v1/rules/preview", request);
  }

  // --- Library ---

  listLibrary(params?: LibraryListParams, signal?: AbortSignal): Promise<Page<LibraryEntry>> {
    return this.request("GET", "/v1/library", undefined, signal, { ...params });
  }

  getLibraryEntry(id: string, signal?: AbortSignal): Promise<LibraryEntry> {
    return this.request("GET", `/v1/library/${id}`, undefined, signal);
  }

  deleteLibraryEntry(id: string, mode: "trash" | "purge" = "trash"): Promise<{ purged: boolean; entry: LibraryEntry | null }> {
    return this.request("DELETE", `/v1/library/${id}`, undefined, undefined, { mode });
  }

  restoreLibraryEntry(id: string): Promise<LibraryEntry> {
    return this.request("POST", `/v1/library/${id}/restore`);
  }

  startLibraryImport(request: LibraryImportRequest): Promise<LibraryImportStatus> {
    return this.request("POST", "/v1/library/import", request);
  }

  getLibraryImportStatus(signal?: AbortSignal): Promise<LibraryImportStatus> {
    return this.request("GET", "/v1/library/import", undefined, signal);
  }

  verifyLibrary(): Promise<VerifyLibraryReport> {
    return this.request("POST", "/v1/library/verify");
  }

  relocateLibrary(request: LibraryRelocationRequest = {}): Promise<RelocationReport> {
    return this.request("POST", "/v1/library/relocate", {
      path: request.path || null,
      max_entries: request.max_entries,
      max_depth: request.max_depth,
    });
  }

  findLibraryDuplicates(params: { sha256?: string; size_bytes?: number; filename?: string; limit?: number }, signal?: AbortSignal): Promise<DuplicateCandidate[]> {
    return this.request("GET", "/v1/library/duplicates", undefined, signal, { ...params });
  }

  getCleanupPolicies(signal?: AbortSignal): Promise<CleanupPolicies> {
    return this.request("GET", "/v1/system/cleanup-policies", undefined, signal);
  }

  updateCleanupPolicies(policies: CleanupPolicies): Promise<CleanupPolicies> {
    return this.request("PUT", "/v1/system/cleanup-policies", policies);
  }

  runLibraryCleanup(): Promise<CleanupReport> {
    return this.request("POST", "/v1/system/cleanup");
  }

  getStatistics(signal?: AbortSignal): Promise<Record<string, unknown>> {
    return this.request("GET", "/v1/statistics", undefined, signal);
  }

  // --- Presets and profiles ---

  listPresets(signal?: AbortSignal): Promise<DownloadPreset[]> {
    return this.request("GET", "/v1/presets", undefined, signal);
  }

  createPreset(input: PutDownloadPreset): Promise<DownloadPreset> {
    return this.request("POST", "/v1/presets", input);
  }

  updatePreset(id: string, input: PutDownloadPreset): Promise<DownloadPreset> {
    return this.request("PUT", `/v1/presets/${id}`, input);
  }

  deletePreset(id: string): Promise<void> {
    return this.request("DELETE", `/v1/presets/${id}`);
  }

  listProfiles(signal?: AbortSignal): Promise<UserProfile[]> {
    return this.request("GET", "/v1/profiles", undefined, signal);
  }

  createProfile(input: PutUserProfile): Promise<UserProfile> {
    return this.request("POST", "/v1/profiles", input);
  }

  updateProfile(id: string, input: PutUserProfile): Promise<UserProfile> {
    return this.request("PUT", `/v1/profiles/${id}`, input);
  }

  deleteProfile(id: string): Promise<void> {
    return this.request("DELETE", `/v1/profiles/${id}`);
  }

  activateProfile(id: string): Promise<ActivateProfileResponse> {
    return this.request("POST", `/v1/profiles/${id}/activate`);
  }

  // --- Basket ---

  listBasket(signal?: AbortSignal): Promise<BasketItem[]> {
    return this.request("GET", "/v1/basket", undefined, signal);
  }

  addBasketItem(request: CreateJob, presetId: string | null = null): Promise<BasketItem> {
    return this.request("POST", "/v1/basket", { request, preset_id: presetId });
  }

  updateBasketItem(id: string, request: CreateJob, presetId: string | null = null): Promise<BasketItem> {
    return this.request("PATCH", `/v1/basket/${id}`, { request, preset_id: presetId });
  }

  reorderBasket(ids: string[]): Promise<BasketItem[]> {
    return this.request("POST", "/v1/basket/reorder", { ids });
  }

  deleteBasketItem(id: string): Promise<void> {
    return this.request("DELETE", `/v1/basket/${id}`);
  }

  clearBasket(): Promise<void> {
    return this.request("DELETE", "/v1/basket");
  }

  startBasket(): Promise<BasketStartResult> {
    return this.request("POST", "/v1/basket/start");
  }

  // --- Media ---

  probeMedia(request: MediaProbeRequest): Promise<MediaProbe> {
    return this.request("POST", "/v1/media/probe", request);
  }

  listMediaArchive(params?: PageQueryParams, signal?: AbortSignal): Promise<Page<MediaArchiveRecord>> {
    return this.request("GET", "/v1/media/archive", undefined, signal, { ...params });
  }

  removeMediaArchive(extractor: string, mediaId: string): Promise<void> {
    return this.request("DELETE", "/v1/media/archive", { extractor, media_id: mediaId });
  }

  getMediaSummary(jobId: string, signal?: AbortSignal): Promise<MediaItemSummary> {
    return this.request("GET", `/v1/jobs/${jobId}/media-summary`, undefined, signal);
  }

  listMediaItems(jobId: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<MediaItemRecord>> {
    return this.request("GET", `/v1/jobs/${jobId}/media-items`, undefined, signal, { ...params });
  }

  retryMediaItem(jobId: string, itemId: string): Promise<Job> {
    return this.request("POST", `/v1/jobs/${jobId}/media-items/${itemId}/retry`);
  }

  retryFailedMediaItems(jobId: string, limit = 100): Promise<RetryFailedMediaItemsResponse> {
    return this.request("POST", `/v1/jobs/${jobId}/media-items/retry-failed`, { limit });
  }

  listMediaItemOutputs(jobId: string, itemId: string, signal?: AbortSignal): Promise<MediaItemOutputRecord[]> {
    return this.request("GET", `/v1/jobs/${jobId}/media-items/${itemId}/outputs`, undefined, signal);
  }

  // --- Torrents ---

  probeTorrent(request: TorrentProbeRequest): Promise<TorrentProbe> {
    return this.request("POST", "/v1/torrents/probe", request);
  }

  listTorrents(params?: PageQueryParams, signal?: AbortSignal): Promise<Page<TorrentRecord>> {
    return this.request("GET", "/v1/torrents", undefined, signal, { ...params });
  }

  getTorrentDetails(id: string, signal?: AbortSignal): Promise<TorrentDetails> {
    return this.request("GET", `/v1/torrents/${id}`, undefined, signal);
  }

  getTorrentStats(id: string, signal?: AbortSignal): Promise<TorrentSnapshot> {
    return this.request("GET", `/v1/torrents/${id}/stats`, undefined, signal);
  }

  getTorrentPeers(id: string, signal?: AbortSignal): Promise<TorrentPeerStats> {
    return this.request("GET", `/v1/torrents/${id}/peers`, undefined, signal);
  }

  addTorrentPeers(id: string, peers: string[]): Promise<void> {
    return this.request("POST", `/v1/torrents/${id}/peers`, { peers });
  }

  updateTorrentFiles(id: string, files: number[]): Promise<void> {
    return this.request("POST", `/v1/torrents/${id}/files`, { files });
  }

  getTorrentSeedingState(id: string, signal?: AbortSignal): Promise<TorrentSeedingState | null> {
    return this.request("GET", `/v1/torrents/${id}/seeding`, undefined, signal);
  }

  listEngineTorrents(signal?: AbortSignal): Promise<TorrentEngineList> {
    return this.request("GET", "/v1/torrents/engine", undefined, signal);
  }

  getTorrentEngineStats(signal?: AbortSignal): Promise<TorrentGlobalStats> {
    return this.request("GET", "/v1/torrents/engine/stats", undefined, signal);
  }

  getTorrentDhtStats(signal?: AbortSignal): Promise<TorrentDhtStats> {
    return this.request("GET", "/v1/torrents/dht/stats", undefined, signal);
  }

  getTorrentDhtTable(signal?: AbortSignal): Promise<TorrentDhtTable> {
    return this.request("GET", "/v1/torrents/dht/table", undefined, signal);
  }

  removeTorrent(id: string, deleteFiles = false): Promise<void> {
    return this.request("POST", `/v1/torrents/${id}/remove`, { delete_files: deleteFiles });
  }

  // --- Automation ---

  listRules(params?: PageQueryParams, signal?: AbortSignal): Promise<Page<AutomationRule>> {
    return this.request("GET", "/v1/rules", undefined, signal, { ...params });
  }

  createRule(input: RuleInput): Promise<AutomationRule> {
    return this.request("POST", "/v1/rules", input);
  }

  updateRule(id: string, input: RuleInput): Promise<AutomationRule> {
    return this.request("PUT", `/v1/rules/${id}`, input);
  }

  deleteRule(id: string): Promise<void> {
    return this.request("DELETE", `/v1/rules/${id}`);
  }

  listSchedules(params?: PageQueryParams, signal?: AbortSignal): Promise<Page<ScheduleRecord>> {
    return this.request("GET", "/v1/schedules", undefined, signal, { ...params });
  }

  createSchedule(input: ScheduleInput): Promise<ScheduleRecord> {
    return this.request("POST", "/v1/schedules", input);
  }

  updateSchedule(id: string, input: ScheduleInput): Promise<ScheduleRecord> {
    return this.request("PUT", `/v1/schedules/${id}`, input);
  }

  runScheduleNow(id: string): Promise<ScheduleExecutionRecord> {
    return this.request("POST", `/v1/schedules/${id}/run-now`);
  }

  listScheduleExecutions(id: string, params?: PageQueryParams, signal?: AbortSignal): Promise<Page<ScheduleExecutionRecord>> {
    return this.request("GET", `/v1/schedules/${id}/executions`, undefined, signal, { ...params });
  }

  getScheduleExecution(id: string, signal?: AbortSignal): Promise<ScheduleExecutionRecord> {
    return this.request("GET", `/v1/schedule-executions/${id}`, undefined, signal);
  }

  cancelScheduleExecution(id: string): Promise<ScheduleExecutionRecord> {
    return this.request("POST", `/v1/schedule-executions/${id}/cancel`);
  }

  setScheduleEnabled(id: string, enabled: boolean): Promise<ScheduleRecord> {
    return this.request("POST", `/v1/schedules/${id}/${enabled ? "enable" : "disable"}`);
  }

  deleteSchedule(id: string): Promise<void> {
    return this.request("DELETE", `/v1/schedules/${id}`);
  }

  // --- Settings ---

  getSettings(signal?: AbortSignal): Promise<SettingsResponse> {
    return this.request("GET", "/v1/settings", undefined, signal);
  }

  validateSettings(patch: PersistentSettingsPatch): Promise<SettingsValidationResponse> {
    return this.request("POST", "/v1/settings/validate", patch);
  }

  patchSettings(patch: PersistentSettingsPatch): Promise<SettingsResponse> {
    return this.request("PATCH", "/v1/settings", patch);
  }

  resetSettings(): Promise<SettingsResponse> {
    return this.request("POST", "/v1/settings/reset");
  }

  // --- Diagnostics ---

  getReadiness(signal?: AbortSignal): Promise<ReadinessStatus> {
    return httpRequest<ReadinessStatus>(this.baseUrl, this.apiToken, "GET", "/health/ready", undefined, {
      signal,
      acceptedStatuses: [503],
    });
  }

  getDatabaseStatus(signal?: AbortSignal): Promise<DatabaseStatus> {
    return this.request("GET", "/v1/system/database", undefined, signal);
  }

  createDatabaseBackup(): Promise<{ path: string }> {
    return this.request("POST", "/v1/system/database/backup");
  }

  listDatabaseBackups(params?: PageQueryParams, signal?: AbortSignal): Promise<Page<BackupRecord>> {
    return this.request("GET", "/v1/system/database/backups", undefined, signal, { ...params });
  }

  verifyDatabaseBackup(name: string): Promise<BackupVerification> {
    return this.request("POST", `/v1/system/database/backups/${encodeURIComponent(name)}/verify`);
  }

  scheduleDatabaseRestore(name: string): Promise<RestoreStatus> {
    return this.request("POST", `/v1/system/database/backups/${encodeURIComponent(name)}/restore`);
  }

  getDatabaseRestoreStatus(signal?: AbortSignal): Promise<RestoreStatus> {
    return this.request("GET", "/v1/system/database/restore", undefined, signal);
  }

  cancelDatabaseRestore(): Promise<RestoreStatus> {
    return this.request("DELETE", "/v1/system/database/restore");
  }

  listHostProfiles(params?: PageQueryParams, signal?: AbortSignal): Promise<Page<HostProfile>> {
    return this.request("GET", "/v1/system/hosts", undefined, signal, { ...params });
  }

  resetHostProfiles(): Promise<{ deleted: number }> {
    return this.request("POST", "/v1/system/hosts/reset");
  }

  listAudit(params?: PageQueryParams, signal?: AbortSignal): Promise<Page<AuditRecord>> {
    return this.request("GET", "/v1/audit", undefined, signal, { ...params });
  }

  verifyAuditChain(signal?: AbortSignal): Promise<AuditChainStatus> {
    return this.request("GET", "/v1/audit/verify", undefined, signal);
  }

  listSecrets(params?: PageQueryParams, signal?: AbortSignal): Promise<Page<SecretReference>> {
    return this.request("GET", "/v1/secrets", undefined, signal, { ...params });
  }

  putSecret(request: PutSecretRequest): Promise<SecretReference> {
    return this.request("POST", "/v1/secrets", request);
  }

  deleteSecret(id: string): Promise<void> {
    return this.request("DELETE", `/v1/secrets/${id}`);
  }

  getDependencies(signal?: AbortSignal): Promise<DependenciesStatus> {
    return this.request("GET", "/v1/system/dependencies", undefined, signal);
  }

  getSystemCapabilities(signal?: AbortSignal): Promise<SystemCapabilities> {
    return this.request("GET", "/v1/system/capabilities", undefined, signal);
  }

  runMaintenance(retentionDays: number): Promise<Record<string, unknown>> {
    return this.request("POST", "/v1/system/maintenance", { retention_days: retentionDays });
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
