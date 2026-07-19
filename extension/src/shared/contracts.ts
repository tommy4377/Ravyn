export const NATIVE_HOST_NAME = "com.ravyn.download_manager";
export const EXTENSION_ID = "firefox-extension@ravyn.app";
export const NATIVE_PROTOCOL_VERSION = 1 as const;
/** Oldest host protocol this extension still understands. */
export const NATIVE_PROTOCOL_MIN = 1 as const;

/**
 * Whether the host's advertised protocol window overlaps ours. The extension
 * and the desktop application update on independent cadences, so an
 * out-of-window host must surface an explicit "update required" state
 * instead of silently dropping traffic.
 */
export function protocolCompatible(capabilities: {
  protocolVersion: number;
  minProtocolVersion?: number;
}): boolean {
  const hostMax = capabilities.protocolVersion;
  const hostMin = capabilities.minProtocolVersion ?? hostMax;
  return hostMin <= NATIVE_PROTOCOL_VERSION && NATIVE_PROTOCOL_MIN <= hostMax;
}
export const MAX_RESOURCE_BATCH = 1_000;
export const MAX_RESOURCES_PER_TAB = 2_000;
export const RESOURCE_MAX_AGE_MS = 30 * 60 * 1_000;

export type InterceptionMode =
  "disabled" | "rules-only" | "ask" | "all-compatible";
export type BypassModifierKey = "alt" | "shift" | "ctrl" | "none";
export type ResourceKind =
  "image" | "video" | "audio" | "manifest" | "document" | "archive" | "other";
export type ResourceSource =
  "dom" | "performance" | "webRequest" | "context-menu" | "video-element";
export type NativeCommand =
  | "ping"
  | "get_capabilities"
  | "create_download"
  | "create_batch"
  | "probe_media"
  | "get_download_summary"
  | "get_job"
  | "pause_job"
  | "resume_job"
  | "cancel_job"
  | "pause_all"
  | "resume_all"
  | "get_rules"
  | "list_presets"
  | "evaluate_url"
  | "open_ravyn"
  | "subscribe_events"
  | "unsubscribe_events";

export interface NativeRequest<T = unknown> {
  id: string;
  protocolVersion: typeof NATIVE_PROTOCOL_VERSION;
  command: NativeCommand;
  payload: T;
}

export interface NativeError {
  code: string;
  message: string;
  retryable: boolean;
}

export interface NativeResponse<T = unknown> {
  id: string;
  ok: boolean;
  result?: T;
  error?: NativeError;
}

export interface NativeEvent {
  type: "event";
  /** Host protocol version; any value inside the negotiated window is accepted. */
  protocolVersion: number;
  event: string;
  payload: unknown;
}

export interface NativeCapabilities {
  protocolVersion: number;
  /** Oldest protocol the host accepts; absent on hosts predating negotiation. */
  minProtocolVersion?: number;
  hostVersion: string;
  backendConnected: boolean;
  features: string[];
}

export interface SourceContext {
  browser: "firefox";
  containerId?: string;
  incognito: boolean;
  pageUrl?: string;
  pageTitle?: string;
  tabId?: number;
  frameId?: number;
}

export interface CookieValue {
  name: string;
  value: string;
  domain: string;
  path: string;
  secure: boolean;
  httpOnly: boolean;
  sameSite: string;
}

export interface CreateDownloadPayload {
  url: string;
  kind?: "http" | "media";
  filename?: string;
  paused?: boolean;
  priority?: number;
  presetId?: string;
  referer?: string;
  userAgent?: string;
  tags?: string[];
  cookies?: CookieValue[];
  postProcessingPreset?:
    | "image-webp"
    | "image-avif"
    | "audio-mp3"
    | "audio-opus"
    | "video-h264"
    | "video-h265";
  media?: {
    format?: string;
    maxHeight?: number;
    audioOnly?: boolean;
    audioFormat?: string;
    writeSubtitles?: boolean;
    subtitleLanguages?: string[];
  };
  idempotencyKey?: string;
  sourceContext: SourceContext;
}

export interface CreateBatchPayload {
  downloads: CreateDownloadPayload[];
}

export interface DetectedResource {
  id: string;
  url: string;
  normalizedUrl: string;
  pageUrl: string;
  frameUrl?: string;
  type: ResourceKind;
  mime?: string;
  extension?: string;
  filename?: string;
  size?: number;
  source: ResourceSource;
  confidence: number;
  discoveredAt: number;
  title?: string;
  width?: number;
  height?: number;
  parentManifestUrl?: string;
}

export interface BrowserRule {
  id: string;
  name: string;
  priority: number;
  enabled: boolean;
  domains: string[];
  extensions: string[];
  mimePatterns: string[];
  urlRegex?: string;
  action: "ravyn" | "browser" | "ask" | "ignore";
  presetId?: string;
}

export interface RuleSnapshot {
  revision: string;
  updatedAt: number;
  expiresAt: number;
  rules: BrowserRule[];
}

export interface DownloadPreset {
  id: string;
  name: string;
}

export interface MediaFormat {
  formatId: string;
  extension?: string;
  width?: number;
  height?: number;
  fps?: number;
  videoCodec?: string;
  audioCodec?: string;
  bitrateKbps?: number;
  audioBitrateKbps?: number;
  filesize?: number;
  protocol?: string;
  note?: string;
}

export interface MediaProbeResult {
  title?: string;
  duration?: number;
  formats: MediaFormat[];
}

export interface DownloadSummary {
  active: number;
  queued: number;
  speedBps: number;
  recent: Array<{
    id: string;
    filename: string;
    status: string;
    progress: number | null;
    speedBps: number | null;
  }>;
}

export type BackgroundRequest =
  | { type: "connection-status" }
  | { type: "download-url"; payload: CreateDownloadPayload }
  | { type: "download-batch"; payload: CreateBatchPayload }
  | { type: "probe-media"; url: string; sourceContext: SourceContext }
  | {
      type: "download-media-element";
      resources: DetectedResource[];
      pageUrl: string;
      sourceContext: SourceContext;
    }
  | { type: "scan-tab"; tabId?: number }
  | { type: "get-tab-resources"; tabId?: number }
  | { type: "get-stream-hint"; tabId?: number }
  | { type: "get-presets" }
  | {
      type: "resources-detected";
      tabId?: number;
      resources: DetectedResource[];
    }
  | { type: "open-ravyn"; section?: string }
  | { type: "get-summary" }
  | { type: "pause-all" }
  | { type: "resume-all" }
  | { type: "get-settings" }
  | { type: "save-settings"; settings: Partial<ExtensionSettings> }
  | {
      type: "request-site-permissions";
      url: string;
      cookies: boolean;
      network: boolean;
    }
  | { type: "clear-extension-data" }
  | { type: "confirmation-result"; requestId: string; accepted: boolean }
  | { type: "monitor-tab"; tabId: number; enabled: boolean }
  | { type: "bypass-download"; url: string };

export interface ExtensionSettings {
  interceptionMode: InterceptionMode;
  automaticInterception: boolean;
  bypassModifierKey: BypassModifierKey;
  interceptExtensions: string[];
  minInterceptSizeBytes: number;
  mediaDetection: boolean;
  networkObservation: boolean;
  videoOverlays: boolean;
  imageOverlays: boolean;
  overlayMinimumWidth: number;
  overlayMinimumHeight: number;
  includePrivateWindows: boolean;
  eraseDelegatedBrowserEntries: boolean;
  notifications: boolean;
  sameDomainOnly: boolean;
  disabledDomains: string[];
  alwaysInterceptDomains: string[];
  allowCookiesByOrigin: string[];
  maxResourcesPerTab: number;
}

export interface ConnectionStatus {
  hostAvailable: boolean;
  backendConnected: boolean;
  error?: NativeError;
  capabilities?: NativeCapabilities;
}
