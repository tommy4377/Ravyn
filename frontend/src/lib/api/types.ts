/**
 * Wire types mirrored from the Ravyn backend OpenAPI contract.
 * Keep in sync with `src/core/models.rs`, `src/api/routes/jobs.rs`,
 * `src/api/pagination.rs`, `src/core/events.rs`,
 * `src/api/openapi/components.rs` and `docs/SETUP_CAPABILITY_MATRIX.md`.
 */

// --- Setup / components ---

export type SetupProfile = "minimal" | "recommended" | "full" | "custom";

export type FeatureId =
  | "standard_downloads"
  | "video_extraction"
  | "media_merging"
  | "torrent_support"
  | "archive_extraction";

export type ComponentId = "ytdlp" | "ffmpeg" | "rqbit" | "seven_zip";

export type ComponentState =
  | "not_installed"
  | "queued"
  | "downloading"
  | "verifying"
  | "installing"
  | "installed"
  | "update_available"
  | "failed"
  | "unsupported"
  | "custom_path";

export interface FeatureStatus {
  feature: FeatureId;
  enabled: boolean;
  satisfied: boolean;
  required_components: ComponentId[];
}

export interface ComponentStatus {
  component: ComponentId;
  state: ComponentState;
  enabled: boolean;
  managed_version: string | null;
  detected_version: string | null;
  managed_path: string | null;
  custom_path: string | null;
  effective_path: string | null;
  available_version: string | null;
  rollback_available: boolean;
  error_message: string | null;
  last_checked_at: string | null;
  verified_at: string | null;
  install_started_at: string | null;
  install_completed_at: string | null;
}

export interface ComponentHealth {
  component: ComponentId;
  healthy: boolean;
  path: string | null;
  version: string | null;
  message: string | null;
}

export interface EngineCleanupReport {
  removed_versions: string[];
  removed_temp_files: string[];
  bytes_freed: number;
}

export interface ComponentOverview {
  setup_profile: SetupProfile;
  features: FeatureStatus[];
  components: ComponentStatus[];
  platform: string;
  manifest_provider: string;
}

export interface FeatureSelection {
  feature: FeatureId;
  enabled: boolean;
}

export type SetupLifecycleState =
  | "not_started"
  | "in_progress"
  | "restart_required"
  | "ready_to_complete"
  | "completed";

export type InstallationMode = "installed" | "portable" | "development";

export interface SetupInstallationState {
  installation_mode: InstallationMode;
  installed_exe: string | null;
  installed_version: string | null;
  installed_sha256: string | null;
  integration_completed: boolean;
  integration_errors: string[];
  relaunch_pending: boolean;
}

export interface ReportInstallationRequest {
  installation_mode: InstallationMode;
  installed_exe: string | null;
  installed_version: string | null;
  installed_sha256: string | null;
  integration_completed: boolean;
  integration_errors: string[];
  relaunch_pending: boolean;
}

export interface SetupState {
  completed: boolean;
  lifecycle: SetupLifecycleState;
  ready_to_complete: boolean;
  restart_required: boolean;
  completed_at: string | null;
  completed_app_version: string | null;
  app_version: string;
  platform: string;
  setup_profile: SetupProfile | null;
  features_selected: boolean;
  library_root: string | null;
  library_prepared: boolean;
  data_dir: string;
  installation: SetupInstallationState | null;
}

export interface PrepareLibraryResult {
  path: string;
  existed: boolean;
  directories: string[];
  available_bytes: number | null;
  restart_required: boolean;
}

export interface ApiErrorBody {
  code: string;
  message: string;
  request_id?: string;
  retryable?: boolean;
  details?: unknown;
}

// --- Pagination ---

/** Cursor-paginated envelope used by outputs/segments/actions/logs endpoints. */
export interface Page<T> {
  items: T[];
  next_cursor: string | null;
}

export interface PageQueryParams {
  cursor?: string;
  limit?: number;
  search?: string;
}

// --- Jobs ---

export type JobKind = "http" | "media" | "torrent";

export type JobStatus =
  | "queued"
  | "probing"
  | "downloading"
  | "paused"
  | "verifying"
  | "post_processing"
  | "seeding"
  | "completed"
  | "partial"
  | "failed"
  | "cancelled";

export type DuplicatePolicy = "reject" | "reuse_existing" | "skip" | "overwrite" | "allow";

export interface MetalinkMetadata {
  size: number;
  piece_length: number | null;
  piece_sha256: string[];
}

export interface MediaOptions {
  format?: string | null;
  max_height?: number | null;
  audio_only?: boolean;
  audio_format?: string | null;
  audio_quality?: string | null;
  merge_output_format?: string | null;
  playlist?: boolean;
  playlist_start?: number | null;
  playlist_end?: number | null;
  write_subtitles?: boolean;
  write_automatic_subtitles?: boolean;
  subtitle_languages?: string[];
  embed_subtitles?: boolean;
  write_thumbnail?: boolean;
  embed_thumbnail?: boolean;
  write_info_json?: boolean;
  write_description?: boolean;
  embed_metadata?: boolean;
  sponsorblock_remove?: string[];
  concurrent_fragments?: number | null;
  cookies_from_browser?: string | null;
  cookies_file?: string | null;
  output_template?: string | null;
}

export interface TorrentOptions {
  selected_files?: number[];
  file_regex?: string | null;
  overwrite?: boolean;
  keep_managed?: boolean;
  seed_after_download?: boolean;
  delete_files_on_remove?: boolean;
  poll_interval_ms?: number;
  max_seed_ratio?: number | null;
  max_seed_time_secs?: number | null;
  min_seed_time_secs?: number;
}

export type PostAction =
  | { type: "verify_sha256"; expected: string }
  | { type: "extract"; destination: string | null; delete_archive: boolean }
  | {
      type: "convert_media";
      extension: string;
      preset?: string | null;
      arguments?: string[];
      unsafe_arguments?: boolean;
      delete_original: boolean;
    }
  | { type: "move"; destination: string }
  | { type: "open" };

export interface DownloadOptions {
  mirrors?: string[];
  metalink?: MetalinkMetadata | null;
  headers?: Record<string, string>;
  cookies?: Record<string, string>;
  proxy?: string | null;
  proxy_secret_id?: string | null;
  cookies_secret_id?: string | null;
  authentication_header_secret_id?: string | null;
  user_agent?: string | null;
  referer?: string | null;
  segments?: number | null;
  overwrite?: boolean;
  library_auto_destination?: boolean;
  tags?: string[];
  post_actions?: PostAction[];
  media?: MediaOptions | null;
  torrent?: TorrentOptions | null;
}

export interface CreateJob {
  preset_id?: string | null;
  kind: JobKind;
  source: string;
  destination?: string | null;
  filename?: string | null;
  priority?: number;
  speed_limit_bps?: number | null;
  expected_sha256?: string | null;
  duplicate_policy?: DuplicatePolicy;
  options?: DownloadOptions;
}

export interface UpdateJob {
  priority?: number | null;
  /** Omitted leaves the limit unchanged; explicit null clears it. */
  speed_limit_bps?: number | null;
  destination?: string | null;
  filename?: string | null;
  tags?: string[] | null;
}

export interface Job {
  id: string;
  kind: JobKind;
  source: string;
  destination: string;
  filename: string | null;
  status: JobStatus;
  priority: number;
  total_bytes: number | null;
  downloaded_bytes: number;
  speed_limit_bps: number | null;
  expected_sha256: string | null;
  error: string | null;
  transfer_mode: string;
  options_json: DownloadOptions;
  created_at: string;
  updated_at: string;
  started_at: string | null;
  completed_at: string | null;
}

export interface JobListParams {
  cursor?: string;
  status?: JobStatus;
  kind?: JobKind;
  search?: string;
  limit?: number;
}

export interface JobPage {
  items: Job[];
  next_cursor: string | null;
}

export type OutputType =
  | "primary"
  | "video"
  | "audio"
  | "subtitle"
  | "thumbnail"
  | "metadata"
  | "torrent_file"
  | "extracted_file"
  | "converted_file"
  | "archive"
  | "directory"
  | "temporary"
  | "other";

export type OutputState =
  | "planned"
  | "creating"
  | "ready"
  | "failed"
  | "deleted"
  | "moved"
  | "replaced";

export type OutputSourceKind = "http" | "media" | "torrent" | "post_process";

export interface JobOutput {
  id: string;
  job_id: string;
  output_type: OutputType;
  original_path: string;
  current_path: string;
  relative_path: string;
  size_bytes: number | null;
  mime_type: string | null;
  checksum_algorithm: string | null;
  checksum_value: string | null;
  state: OutputState;
  source_kind: OutputSourceKind;
  parent_output_id: string | null;
  producing_action_index: number | null;
  metadata: unknown;
  created_at: string;
  updated_at: string;
}

export interface JobActionRecord {
  id: string;
  job_id: string;
  action_index: number;
  action: PostAction;
  input_path: string;
  output_path: string | null;
  state: string;
  attempts: number;
  error: string | null;
  created_at: string;
  updated_at: string;
}

export interface JobLogRecord {
  id: number;
  job_id: string;
  timestamp: string;
  source_module: string;
  severity: string;
  code: string;
  message: string;
  metadata: unknown;
}

export interface SegmentRecord {
  id: string;
  job_id: string;
  index: number;
  start_byte: number;
  end_byte: number;
  downloaded_bytes: number;
  state: string;
  [key: string]: unknown;
}

export type BulkJobAction = "pause" | "resume" | "cancel" | "retry" | "delete";

export interface BulkJobActionResult {
  id: string;
  success: boolean;
  error: string | null;
}

export interface ImportDefaults {
  kind?: JobKind;
  destination?: string | null;
  priority?: number;
  speed_limit_bps?: number | null;
  duplicate_policy?: DuplicatePolicy;
  options?: DownloadOptions;
}

export interface ImportTextRequest {
  text: string;
  defaults?: ImportDefaults;
}

export interface ImportItemResult {
  source: string;
  job: Job | null;
  error: string | null;
}

export interface ImportResult {
  accepted: number;
  rejected: number;
  duplicate_lines: number;
  truncated: boolean;
  items: ImportItemResult[];
}


// --- Library / basket / torrents / automation / settings / diagnostics ---

export type LibraryCategory =
  | "downloads"
  | "videos"
  | "music"
  | "documents"
  | "images"
  | "archives"
  | "torrents"
  | "playlists"
  | "temporary"
  | "other";

export type LibraryEntryState = "active" | "trashed" | "missing";

export interface LibraryEntry {
  id: string;
  job_id: string | null;
  source_url: string;
  mirrors: string[];
  sha256: string | null;
  size_bytes: number | null;
  path: string;
  filename: string;
  category: LibraryCategory;
  mime_type: string | null;
  media_metadata: unknown;
  torrent_metadata: unknown;
  tags: string[];
  trust: unknown | null;
  state: LibraryEntryState;
  trash_path: string | null;
  imported: boolean;
  downloaded_at: string;
  created_at: string;
  updated_at: string;
}

export interface LibraryListParams extends PageQueryParams {
  q?: string;
  category?: LibraryCategory;
  state?: LibraryEntryState;
  tag?: string;
  mime?: string;
  downloaded_from?: string;
  downloaded_to?: string;
}

export interface LibraryImportRequest {
  path: string;
  tags?: string[];
  max_entries?: number;
  max_depth?: number;
}

export interface LibraryImportStatus {
  run_id: string | null;
  running: boolean;
  root: string | null;
  scanned: number;
  imported: number;
  duplicates: number;
  skipped: number;
  errors: string[];
  started_at: string | null;
  completed_at: string | null;
}

export interface VerifyLibraryReport {
  checked: number;
  missing: number;
}

export interface RelocationReport {
  scanned: number;
  repaired: number;
  unmatched: number;
}

export interface DuplicateCandidate {
  entry: LibraryEntry;
  matches: string[];
}

export interface CleanupPolicies {
  temporary_max_age_days: number;
  trash_retention_days: number;
  log_retention_days: number;
  cache_retention_days: number;
}

export interface CleanupReport {
  temporary_files_removed: number;
  temporary_bytes_removed: number;
  cache_files_removed: number;
  cache_bytes_removed: number;
  trash_entries_purged: number;
  job_logs_removed: number;
}

export interface DownloadPresetPayload {
  destination: string | null;
  filename_template: string | null;
  priority: number | null;
  speed_limit_bps: number | null;
  duplicate_policy: string | null;
  options: DownloadOptions | null;
  template_variables: Record<string, string>;
  scheduler: unknown | null;
  rules: string[];
  metadata: unknown;
}

export interface DownloadPreset {
  id: string;
  name: string;
  payload: DownloadPresetPayload;
  created_at: string;
  updated_at: string;
}

export interface PutDownloadPreset {
  name: string;
  payload: Partial<DownloadPresetPayload>;
}

export interface UserProfile {
  id: string;
  name: string;
  settings_patch: PersistentSettingsPatch;
  default_preset_id: string | null;
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface PutUserProfile {
  name: string;
  settings_patch: PersistentSettingsPatch;
  default_preset_id: string | null;
}

export interface ActivateProfileResponse {
  profile: UserProfile;
  restart_required: boolean;
}

export interface BasketItem {
  id: string;
  position: number;
  request: CreateJob;
  preset_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface BasketStartItemResult {
  basket_item_id: string;
  job: Job | null;
  error: string | null;
}

export interface BasketStartResult {
  items: BasketStartItemResult[];
  started: number;
  failed: number;
}

export interface TorrentRecord {
  job_id: string;
  torrent_id: string;
  info_hash: string | null;
  name: string | null;
  state: string;
  downloaded_bytes: number;
  uploaded_bytes: number;
  total_bytes: number | null;
  download_speed_bps: number;
  upload_speed_bps: number;
  peers_connected: number;
  seeders: number;
  leechers: number;
  raw: unknown;
  updated_at: string;
}

export interface TorrentFile {
  index: number;
  path: string;
  size_bytes: number | null;
}

export interface TorrentProbeRequest {
  source: string;
  destination?: string | null;
  file_regex?: string | null;
}

export interface TorrentProbe {
  torrent_id: string | null;
  info_hash: string | null;
  name: string | null;
  total_bytes: number | null;
  files: TorrentFile[];
  raw: unknown;
}

export interface TorrentDetails {
  torrent_id: string;
  info_hash: string | null;
  name: string | null;
  state: string | null;
  total_bytes: number | null;
  files: TorrentFile[];
  raw: unknown;
}

export interface TorrentSnapshot {
  torrent_id: string;
  info_hash: string | null;
  name: string | null;
  state: string;
  downloaded_bytes: number;
  uploaded_bytes: number;
  total_bytes: number | null;
  download_speed_bps: number;
  upload_speed_bps: number;
  peers_connected: number;
  seeders: number;
  leechers: number;
  finished: boolean;
  progress: number | null;
  raw: unknown;
}

export interface TorrentPeer {
  address: string | null;
  client: string | null;
  state: string | null;
  downloaded_bytes: number | null;
  uploaded_bytes: number | null;
  download_speed_bps: number | null;
  upload_speed_bps: number | null;
  raw: unknown;
}

export interface TorrentPeerStats {
  peers: TorrentPeer[];
  raw: unknown;
}

export interface TorrentGlobalStats {
  downloaded_bytes: number | null;
  uploaded_bytes: number | null;
  download_speed_bps: number | null;
  upload_speed_bps: number | null;
  active_torrents: number | null;
  raw: unknown;
}

export interface TorrentDhtStats {
  id: string;
  outstanding_requests: number;
  routing_table_size: number;
  routing_table_size_v6: number;
}

export interface TorrentSeedingState {
  job_id: string;
  torrent_id: string;
  started_at: string;
  stopped_at: string | null;
  stop_reason: string | null;
  last_ratio: number | null;
  updated_at: string;
}

export interface MediaProbeRequest {
  url: string;
  cookies_from_browser?: string | null;
  cookies_file?: string | null;
  proxy?: string | null;
}

export interface MediaFormat {
  format_id: string;
  extension: string | null;
  width: number | null;
  height: number | null;
  fps: number | null;
  video_codec: string | null;
  audio_codec: string | null;
  bitrate_kbps: number | null;
  audio_bitrate_kbps: number | null;
  filesize: number | null;
  filesize_approx: number | null;
  protocol: string | null;
  note: string | null;
}

export interface MediaProbe {
  id: string | null;
  title: string | null;
  description: string | null;
  webpage_url: string | null;
  extractor: string | null;
  duration: number | null;
  live_status: string | null;
  thumbnail: string | null;
  uploader: string | null;
  playlist_count: number | null;
  formats: MediaFormat[];
  subtitles: string[];
  automatic_captions: string[];
}

export interface MediaItemRecord {
  id: string;
  job_id: string;
  item_key: string;
  extractor: string | null;
  media_id: string | null;
  title: string | null;
  webpage_url: string | null;
  playlist_id: string | null;
  playlist_title: string | null;
  playlist_index: number | null;
  playlist_count: number | null;
  extension: string | null;
  state: string;
  output_path: string | null;
  output_id: string | null;
  retry_job_id: string | null;
  error: string | null;
  metadata: unknown;
  created_at: string;
  updated_at: string;
}

export interface MediaItemSummary {
  job_id: string;
  total: number;
  planned: number;
  downloading: number;
  completed: number;
  failed: number;
  skipped: number;
  retried: number;
  playlist_id: string | null;
  playlist_title: string | null;
  declared_playlist_count: number | null;
}

export interface MediaArchiveRecord {
  extractor: string;
  media_id: string;
  first_job_id: string | null;
  last_job_id: string | null;
  last_output_id: string | null;
  webpage_url: string | null;
  downloaded_at: string;
  metadata: unknown;
}

export interface MediaItemRetryResult {
  item_id: string;
  job: Job | null;
  error_code: string | null;
  error: string | null;
}

export interface RetryFailedMediaItemsResponse {
  attempted: number;
  accepted: number;
  failed: number;
  results: MediaItemRetryResult[];
}

export interface RuleMatcher {
  domains: string[];
  extensions: string[];
  mime_types: string[];
  url_regex: string | null;
}

export interface RuleActions {
  destination: string | null;
  tags: string[];
  speed_limit_bps: number | null;
  post_actions: PostAction[];
}

export interface AutomationRule {
  id: string;
  name: string;
  enabled: boolean;
  priority: number;
  matcher: RuleMatcher;
  actions: RuleActions;
}

export interface RuleInput {
  name: string;
  enabled: boolean;
  priority: number;
  matcher: RuleMatcher;
  actions: RuleActions;
}

export type ScheduleMode = "download" | "sniff_resources";
export type ScheduleOverlapPolicy = "skip" | "queue" | "replace" | "allow_parallel";
export type ScheduleMissedRunPolicy = "skip" | "run_once" | "catch_up";

export interface ScheduleInput {
  enabled: boolean;
  source: string;
  kind: JobKind;
  destination: string;
  mode: ScheduleMode;
  automation: unknown | null;
  interval_seconds: number | null;
  cron_expression: string | null;
  next_run_at: string | null;
  timezone_offset_minutes: number;
  timezone_name: string | null;
  overlap_policy: ScheduleOverlapPolicy;
  missed_run_policy: ScheduleMissedRunPolicy;
  max_catch_up_runs: number;
  paused_until: string | null;
  options: DownloadOptions;
}

export interface ScheduleExecutionRecord {
  id: string;
  schedule_id: string;
  intended_run_at: string;
  state: string;
  summary: unknown | null;
  error: string | null;
  started_at: string;
  completed_at: string | null;
}

export interface ScheduleRecord {
  id: string;
  enabled: boolean;
  source: string;
  kind: JobKind;
  destination: string;
  mode: ScheduleMode;
  interval_seconds: number | null;
  cron_expression: string | null;
  next_run_at: string;
  timezone_offset_minutes: number;
  timezone_name: string | null;
  overlap_policy: ScheduleOverlapPolicy;
  missed_run_policy: ScheduleMissedRunPolicy;
  max_catch_up_runs: number;
  catch_up_runs: number;
  paused_until: string | null;
  options: DownloadOptions;
  last_run_at: string | null;
  failure_count: number;
  last_error: string | null;
  created_at: string;
  updated_at: string;
}

export interface PersistentSettings {
  download_dir: string | null;
  library_root: string | null;
  library_auto_organize: boolean;
  library_category_overrides: Record<string, LibraryCategory>;
  max_active: number;
  max_segments: number;
  segment_threshold_mib: number;
  max_connections_per_host: number;
  global_speed_limit_bps: number;
  bandwidth_schedule: unknown;
  ytdlp: string;
  ffmpeg: string;
  rqbit: string;
  rqbit_api: string;
  rqbit_credentials_secret_id: string | null;
  seven_zip: string;
  auto_provision: boolean;
  max_extract_mib: number;
  max_extract_files: number;
  max_extract_depth: number;
  max_extract_ratio: number;
  max_retries: number;
  host_circuit_threshold: number;
  host_circuit_cooldown_secs: number;
  max_torrent_mib: number;
  max_html_mib: number;
  max_sniff_resources: number;
  max_batch_urls: number;
  connect_timeout_secs: number;
  read_timeout_secs: number;
  media_probe_timeout_secs: number;
  media_probe_max_mib: number;
  rqbit_timeout_secs: number;
  rqbit_stats_timeout_secs: number;
  torrent_refresh_concurrency: number;
  image_converter: string;
  avif_quality: number;
  cookie_dir: string | null;
  api_request_timeout_secs: number;
  api_max_concurrent_requests: number;
  api_rate_limit_per_minute: number;
  api_rate_limit_burst: number;
}

export type PersistentSettingsPatch = Partial<PersistentSettings>;

export interface SettingsResponse {
  values: PersistentSettings;
  application: Record<string, "live" | "backend_restart">;
  restart_required: boolean;
}

export interface SettingsIssue {
  field: string;
  message: string;
}

export interface SettingsValidationResponse {
  valid: boolean;
  restart_required: boolean;
  issues: SettingsIssue[];
}

export interface ReadinessStatus {
  ready: boolean;
  database_writable: boolean;
  download_root_writable: boolean;
  progress_writer_running: boolean;
  accepting_tasks: boolean;
}

export interface DatabaseStatus {
  integrity: string;
}

export interface DependencyStatus {
  name: string;
  available: boolean;
  path: string;
  version: string | null;
  compatibility: "compatible" | "incompatible" | "unknown";
  missing_capabilities: string[];
  error: string | null;
}

export interface TorrentDependencyStatus {
  api_url: string;
  available: boolean;
  server: string | null;
  version: string | null;
  compatibility: "compatible" | "incompatible" | "unknown";
  missing_required_apis: string[];
  error: string | null;
}

export interface DependenciesStatus {
  media: DependencyStatus[];
  torrent: TorrentDependencyStatus;
}

export interface SystemCapabilities {
  backend_version: string;
  api_version: string;
  database_version: number;
  supported_job_kinds: JobKind[];
  external_tools: DependenciesStatus;
  available_features: string[];
  disabled_features: string[];
  platform: string;
  authentication_modes: string[];
}

export interface AuditRecord {
  id: number | string;
  timestamp?: string;
  created_at?: string;
  action: string;
  resource_type: string;
  resource_id: string | null;
  outcome: string;
  metadata?: unknown;
  [key: string]: unknown;
}

export interface AuditChainStatus {
  valid: boolean;
  chained_entries: number;
  head: string | null;
}

export interface BackupRecord {
  name: string;
  size_bytes: number;
  modified_at: string | null;
}

export interface BackupVerification {
  name: string;
  integrity: string;
}

export interface PendingRestore {
  backup_name: string;
  requested_at: string;
  phase: "staged" | "applied";
}

export interface RestoreResultRecord {
  backup_name: string;
  outcome: string;
  completed_at: string;
  message: string;
}

export interface RestoreStatus {
  pending: PendingRestore | null;
  last_result: RestoreResultRecord | null;
  restart_required: boolean;
}

export interface HostProfile {
  host: string;
  successful_downloads: number;
  failed_downloads: number;
  consecutive_failures: number;
  average_throughput_bps: number | null;
  range_failures: number;
  circuit_open_until: string | null;
  last_error: string | null;
}


// --- Events (Server-Sent Events on /v1/events) ---

export interface ProgressSnapshot {
  job_id: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  bytes_per_second: number;
}

export interface JobStatusEvent {
  sequence: number;
  type: "job_status";
  job_id: string;
  status: JobStatus;
  error: string | null;
}

export interface ProgressEvent {
  sequence: number;
  type: "progress";
  job_id: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  bytes_per_second: number;
}

export interface ComponentEvent {
  sequence: number;
  type: "component";
  component: ComponentId;
  state: ComponentState;
  progress_pct?: number;
  bytes_downloaded?: number;
  bytes_total?: number;
  message?: string;
}

export interface QueueChangedEvent {
  sequence: number;
  type: "queue_changed";
}

export interface ResyncRequiredEvent {
  sequence: number;
  type: "resync_required";
  oldest_available: number;
  newest_available: number;
}

export type RavynEvent =
  | JobStatusEvent
  | ProgressEvent
  | ComponentEvent
  | QueueChangedEvent
  | ResyncRequiredEvent
  | { sequence: number; type: string; [key: string]: unknown };
