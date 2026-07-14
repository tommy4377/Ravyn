import { describe, expect, it } from "vitest";
import type { Job, MediaItemOutputRecord } from "../api/types";
import { mediaProgress, uniqueProducedFiles } from "./mediaPresentation";

function job(overrides: Partial<Job>): Job {
  return {
    id: "job",
    kind: "media",
    source: "https://example.test/video",
    destination: "C:/Downloads",
    filename: "video.mp4",
    status: "downloading",
    priority: 0,
    total_bytes: 100,
    downloaded_bytes: 25,
    speed_limit_bps: null,
    expected_sha256: null,
    error: null,
    transfer_mode: "single",
    options_json: {},
    created_at: "2026-07-14T10:00:00Z",
    updated_at: "2026-07-14T10:00:00Z",
    started_at: null,
    completed_at: null,
    ...overrides,
  };
}

function output(path: string, createdAt: string): MediaItemOutputRecord {
  return {
    media_item_id: "item",
    role: "primary",
    created_at: createdAt,
    output: {
      id: `${path}-${createdAt}`,
      job_id: "job",
      output_type: "video",
      original_path: path,
      current_path: path,
      relative_path: path.split("/").at(-1) ?? path,
      size_bytes: 1,
      mime_type: "video/mp4",
      checksum_algorithm: null,
      checksum_value: null,
      state: "ready",
      source_kind: "media",
      parent_output_id: null,
      producing_action_index: null,
      metadata: null,
      created_at: createdAt,
      updated_at: createdAt,
    },
  };
}

describe("mediaProgress", () => {
  it("clamps byte progress", () => {
    expect(mediaProgress(job({ downloaded_bytes: 150 }))).toBe(100);
  });

  it("reports completed jobs without a known size as complete", () => {
    expect(mediaProgress(job({ status: "completed", total_bytes: null }))).toBe(100);
  });
});

describe("uniqueProducedFiles", () => {
  it("keeps the newest record for the same path", () => {
    const records = uniqueProducedFiles([
      output("C:/video.mp4", "2026-07-14T10:00:00Z"),
      output("C:/video.mp4", "2026-07-14T11:00:00Z"),
    ]);
    expect(records).toHaveLength(1);
    expect(records[0]?.created_at).toBe("2026-07-14T11:00:00Z");
  });
});
