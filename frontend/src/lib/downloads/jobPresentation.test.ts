import { describe, expect, it } from "vitest";
import { permittedActions, presentStatus, presentTrust } from "./jobPresentation";
import type { JobStatus } from "../api/types";

// This mirrors the exact transition guards in src/core/lifecycle.rs
// (pause/resume/retry/cancel) so the UI never offers an action the
// backend will reject.
describe("permittedActions", () => {
  it("only allows pausing an http job while probing or downloading", () => {
    expect(permittedActions("downloading", "http").pause).toBe(true);
    expect(permittedActions("probing", "http").pause).toBe(true);
    expect(permittedActions("queued", "http").pause).toBe(false);
    expect(permittedActions("seeding", "http").pause).toBe(false);
  });

  it("allows pausing a torrent while seeding, unlike other job kinds", () => {
    expect(permittedActions("seeding", "torrent").pause).toBe(true);
    expect(permittedActions("seeding", "media").pause).toBe(false);
  });

  it("allows resume from paused or failed (matches lifecycle::resume's allowed list)", () => {
    expect(permittedActions("paused", "http").resume).toBe(true);
    expect(permittedActions("failed", "http").resume).toBe(true);
    expect(permittedActions("cancelled", "http").resume).toBe(false);
    expect(permittedActions("downloading", "http").resume).toBe(false);
  });

  it("allows retry from failed, cancelled, or partial only", () => {
    for (const status of ["failed", "cancelled", "partial"] as JobStatus[]) {
      expect(permittedActions(status, "http").retry).toBe(true);
    }
    for (const status of ["queued", "downloading", "paused", "completed", "seeding"] as JobStatus[]) {
      expect(permittedActions(status, "http").retry).toBe(false);
    }
  });

  it("allows cancel for any non-terminal status", () => {
    for (const status of ["queued", "probing", "downloading", "paused", "verifying", "post_processing", "seeding"] as JobStatus[]) {
      expect(permittedActions(status, "http").cancel).toBe(true);
    }
    for (const status of ["completed", "partial", "failed", "cancelled"] as JobStatus[]) {
      expect(permittedActions(status, "http").cancel).toBe(false);
    }
  });

  it("allows remove once a job is terminal, paused, or seeding", () => {
    expect(permittedActions("completed", "http").remove).toBe(true);
    expect(permittedActions("paused", "http").remove).toBe(true);
    expect(permittedActions("seeding", "torrent").remove).toBe(true);
    expect(permittedActions("downloading", "http").remove).toBe(false);
  });
});

describe("presentStatus", () => {
  it("gives every status a distinct, non-empty label", () => {
    const statuses: JobStatus[] = [
      "queued",
      "probing",
      "downloading",
      "paused",
      "verifying",
      "post_processing",
      "seeding",
      "completed",
      "partial",
      "failed",
      "cancelled",
    ];
    const labels = new Set(statuses.map((status) => presentStatus(status).label));
    expect(labels.size).toBe(statuses.length);
  });

  it("marks in-flight transitional statuses as spinning", () => {
    expect(presentStatus("probing").spinning).toBe(true);
    expect(presentStatus("verifying").spinning).toBe(true);
    expect(presentStatus("post_processing").spinning).toBe(true);
    expect(presentStatus("downloading").spinning).toBeFalsy();
  });
});


describe("presentTrust", () => {
  const report = (score: number) => ({ score, level: "advisory", factors: [] });

  it("uses plain-language trust states instead of exposing the numeric score", () => {
    expect(presentTrust(report(90)).label).toBe("Secure source");
    expect(presentTrust(report(65)).label).toBe("Verification recommended");
    expect(presentTrust(report(20)).label).toBe("Source requires attention");
  });
});
