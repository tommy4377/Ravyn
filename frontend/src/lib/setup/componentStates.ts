/**
 * UI mapping for backend component states and features.
 * Every state gets text, an icon name, an accessible description, and the
 * actions the UI may offer (plan §14.3).
 */

import type { ComponentId, ComponentState, FeatureId } from "../api/types";

export interface StateDescriptor {
  label: string;
  description: string;
  icon: "idle" | "busy" | "success" | "warning" | "error";
  actions: ("install" | "retry" | "cancel")[];
}

export const COMPONENT_STATE_UI: Record<ComponentState, StateDescriptor> = {
  not_installed: {
    label: "Not installed",
    description: "The component has not been installed yet.",
    icon: "idle",
    actions: ["install"],
  },
  queued: {
    label: "Queued",
    description: "Waiting to start downloading.",
    icon: "busy",
    actions: ["cancel"],
  },
  downloading: {
    label: "Downloading",
    description: "Downloading the verified component package.",
    icon: "busy",
    actions: ["cancel"],
  },
  verifying: {
    label: "Verifying",
    description: "Verifying the download checksum.",
    icon: "busy",
    actions: ["cancel"],
  },
  installing: {
    label: "Installing",
    description: "Installing the verified component.",
    icon: "busy",
    actions: ["cancel"],
  },
  installed: {
    label: "Installed",
    description: "The component is installed and verified.",
    icon: "success",
    actions: [],
  },
  update_available: {
    label: "Update available",
    description: "A compatible verified update exists.",
    icon: "warning",
    actions: ["install"],
  },
  failed: {
    label: "Failed",
    description: "Installation failed. You can retry.",
    icon: "error",
    actions: ["retry"],
  },
  unsupported: {
    label: "Unsupported",
    description: "No verified package exists for this platform.",
    icon: "warning",
    actions: [],
  },
  cancelled: {
    label: "Cancelled",
    description: "The component operation was cancelled before activation.",
    icon: "warning",
    actions: ["retry"],
  },
  custom_path: {
    label: "Custom path",
    description: "Using an executable you configured yourself.",
    icon: "success",
    actions: [],
  },
  custom_path_invalid: {
    label: "Custom path invalid",
    description: "The configured executable is missing or failed its capability check.",
    icon: "error",
    actions: [],
  },
};

export interface FeatureUi {
  title: string;
  description: string;
  engine: string | null;
  locked: boolean;
}

export const FEATURE_UI: Record<FeatureId, FeatureUi> = {
  standard_downloads: {
    title: "Standard downloads",
    description: "Core HTTP/HTTPS downloads. Always enabled.",
    engine: null,
    locked: true,
  },
  video_extraction: {
    title: "Video and playlists",
    description:
      "Download supported video, audio, playlists, and channels.",
    engine: "yt-dlp",
    locked: false,
  },
  media_merging: {
    title: "High-quality media processing",
    description: "Merge streams, probe media, and convert formats.",
    engine: "FFmpeg",
    locked: false,
  },
  torrent_support: {
    title: "Torrent downloads",
    description: "Download magnet links and torrent files.",
    engine: "rqbit",
    locked: false,
  },
  archive_extraction: {
    title: "Archive extraction",
    description: "Extract supported archive formats after download.",
    engine: "7-Zip",
    locked: false,
  },
};

export const COMPONENT_LABEL: Record<ComponentId, string> = {
  ytdlp: "yt-dlp",
  ffmpeg: "FFmpeg",
  rqbit: "rqbit",
  seven_zip: "7-Zip",
};

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = "B";
  for (const next of units) {
    if (value < 1024) break;
    value /= 1024;
    unit = next;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${unit}`;
}
