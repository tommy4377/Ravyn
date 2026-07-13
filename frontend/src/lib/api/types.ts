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
  managed_path: string | null;
  custom_path: string | null;
  effective_path: string | null;
  error_message: string | null;
  last_checked_at: string | null;
  install_started_at: string | null;
  install_completed_at: string | null;
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

export interface SetupState {
  completed: boolean;
  completed_at: string | null;
  completed_app_version: string | null;
  app_version: string;
  platform: string;
  setup_profile: SetupProfile | null;
  features_selected: boolean;
  library_root: string | null;
  library_prepared: boolean;
  data_dir: string;
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
