/**
 * Builds the row/bulk action menu from the exact permitted-action set for a
 * job. There is deliberately no "open containing folder" action: Ravyn has
 * no Tauri command for revealing a path in Explorer yet (only the 5
 * setup-only commands exist), so offering it would violate the "never add
 * a control without implemented behavior" rule. Output paths are instead
 * shown as copyable text in the job details Outputs tab.
 */

import type { Job } from "../api/types";
import type { MenuItem } from "../components/Menu.svelte";
import type { JobActionSet } from "./jobPresentation";

export interface JobRowActions {
  onOpenDetails: (job: Job) => void;
  onPause: (job: Job) => void;
  onResume: (job: Job) => void;
  onRetry: (job: Job) => void;
  onCancel: (job: Job) => void;
  onRemove: (job: Job) => void;
}

export function buildJobMenuItems(
  job: Job,
  permitted: JobActionSet,
  actions: JobRowActions,
): MenuItem[] {
  const items: MenuItem[] = [
    {
      id: "details",
      label: "View details",
      icon: "external-link",
      onSelect: () => actions.onOpenDetails(job),
    },
  ];

  let separator = true;
  if (permitted.pause) {
    items.push({ id: "pause", label: "Pause", icon: "pause", separatorBefore: separator, onSelect: () => actions.onPause(job) });
    separator = false;
  }
  if (permitted.resume) {
    items.push({ id: "resume", label: "Resume", icon: "play", separatorBefore: separator, onSelect: () => actions.onResume(job) });
    separator = false;
  }
  if (permitted.retry) {
    items.push({ id: "retry", label: "Retry", icon: "refresh", separatorBefore: separator, onSelect: () => actions.onRetry(job) });
    separator = false;
  }
  if (permitted.cancel) {
    items.push({ id: "cancel", label: "Cancel download", icon: "cancel", separatorBefore: separator, onSelect: () => actions.onCancel(job) });
  }

  if (permitted.remove) {
    items.push({
      id: "remove",
      label: "Remove from list",
      icon: "trash",
      danger: true,
      separatorBefore: true,
      onSelect: () => actions.onRemove(job),
    });
  }

  return items;
}
