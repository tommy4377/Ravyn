/**
 * Maps backend `JobStatus`/`JobKind` to UI presentation and permitted
 * actions. The action set mirrors the exact transition guards in
 * `src/core/lifecycle.rs` (`pause`/`resume`/`retry`/`cancel`) rather than a
 * generic guess, so the UI never offers an action the backend will reject.
 */

import type { IconName } from "../components/Icon.svelte";
import type { JobKind, JobStatus, TrustReport } from "../api/types";

export type Severity = "neutral" | "info" | "success" | "warning" | "error";

export interface StatusPresentation {
  label: string;
  icon: IconName;
  severity: Severity;
  spinning?: boolean;
}

const PRESENTATION: Record<JobStatus, StatusPresentation> = {
  queued: { label: "Queued", icon: "clock", severity: "neutral" },
  probing: { label: "Probing", icon: "spinner", severity: "info", spinning: true },
  downloading: { label: "Downloading", icon: "download", severity: "info" },
  paused: { label: "Paused", icon: "pause", severity: "neutral" },
  verifying: { label: "Verifying", icon: "spinner", severity: "info", spinning: true },
  post_processing: { label: "Post-processing", icon: "spinner", severity: "info", spinning: true },
  seeding: { label: "Seeding", icon: "upload", severity: "success" },
  completed: { label: "Completed", icon: "check-circle", severity: "success" },
  partial: { label: "Partially completed", icon: "warning", severity: "warning" },
  failed: { label: "Failed", icon: "alert-circle", severity: "error" },
  cancelled: { label: "Cancelled", icon: "cancel", severity: "neutral" },
};

export function presentStatus(status: JobStatus): StatusPresentation {
  return PRESENTATION[status];
}

export interface JobActionSet {
  pause: boolean;
  resume: boolean;
  retry: boolean;
  cancel: boolean;
  remove: boolean;
}

const TERMINAL: JobStatus[] = ["completed", "partial", "failed", "cancelled"];

export function permittedActions(status: JobStatus, kind: JobKind): JobActionSet {
  const pauseSources: JobStatus[] =
    kind === "torrent" ? ["downloading", "probing", "seeding"] : ["downloading", "probing"];
  return {
    pause: pauseSources.includes(status),
    resume: status === "paused" || status === "failed",
    retry: status === "failed" || status === "cancelled" || status === "partial",
    cancel: !TERMINAL.includes(status),
    remove: TERMINAL.includes(status) || status === "paused" || status === "seeding",
  };
}

export interface TrustPresentation {
  label: string;
  description: string;
  severity: "success" | "warning" | "error";
}

/** Maps the advisory backend score to user-facing language without exposing a misleading numeric grade. */
export function presentTrust(report: TrustReport): TrustPresentation {
  if (report.score >= 80) {
    return {
      label: "Secure source",
      description: "The available source and verification signals look reliable.",
      severity: "success",
    };
  }
  if (report.score >= 50) {
    return {
      label: "Verification recommended",
      description: "The source can be used, but an additional checksum or signature would improve confidence.",
      severity: "warning",
    };
  }
  return {
    label: "Source requires attention",
    description: "Review the source and verification details before opening the downloaded file.",
    severity: "error",
  };
}
