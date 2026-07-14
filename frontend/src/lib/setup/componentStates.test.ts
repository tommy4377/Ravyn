import { describe, expect, it } from "vitest";
import {
  COMPONENT_LABEL,
  COMPONENT_STATE_UI,
  FEATURE_UI,
  formatBytes,
} from "./componentStates";
import type { ComponentState, FeatureId } from "../api/types";

describe("componentStates", () => {
  it("covers every backend component state", () => {
    const states: ComponentState[] = [
      "not_installed",
      "queued",
      "downloading",
      "verifying",
      "installing",
      "installed",
      "update_available",
      "failed",
      "unsupported",
      "cancelled",
      "custom_path",
      "custom_path_invalid",
    ];
    for (const state of states) {
      expect(COMPONENT_STATE_UI[state].label).toBeTruthy();
      expect(COMPONENT_STATE_UI[state].description).toBeTruthy();
    }
  });

  it("covers every backend feature", () => {
    const features: FeatureId[] = [
      "standard_downloads",
      "video_extraction",
      "media_merging",
      "torrent_support",
      "archive_extraction",
    ];
    for (const feature of features) {
      expect(FEATURE_UI[feature].title).toBeTruthy();
    }
    expect(FEATURE_UI.standard_downloads.locked).toBe(true);
  });

  it("labels every component id", () => {
    expect(COMPONENT_LABEL.ytdlp).toBe("yt-dlp");
    expect(COMPONENT_LABEL.seven_zip).toBe("7-Zip");
  });

  it("formats byte sizes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KiB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MiB");
    expect(formatBytes(150 * 1024 * 1024)).toBe("150 MiB");
  });
});
