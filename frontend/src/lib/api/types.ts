/**
 * Wire types mirrored from the Ravyn backend OpenAPI contract.
 * Keep in sync with `src/api/openapi/components.rs` and
 * `docs/SETUP_CAPABILITY_MATRIX.md`.
 */

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

/** Server-sent events published on `/v1/events`. */
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

export interface ResyncRequiredEvent {
  sequence: number;
  type: "resync_required";
  oldest_available: number;
  newest_available: number;
}

export type RavynEvent =
  | ComponentEvent
  | ResyncRequiredEvent
  | { sequence: number; type: string; [key: string]: unknown };
