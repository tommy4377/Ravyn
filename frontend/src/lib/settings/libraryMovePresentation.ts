import type { LibraryMoveStatus } from "../api/types";

export function isLibraryMoveRunning(status: LibraryMoveStatus | null): boolean {
  return status?.state === "running" || status?.state === "cancelling";
}

export function libraryMoveProgress(status: LibraryMoveStatus | null): number | null {
  if (!status) return null;
  if (status.total_bytes > 0) {
    return Math.max(0, Math.min(100, Math.round((status.copied_bytes / status.total_bytes) * 100)));
  }
  if (status.total_files > 0) {
    return Math.max(0, Math.min(100, Math.round((status.verified_files / status.total_files) * 100)));
  }
  return null;
}

export function libraryMoveTitle(status: LibraryMoveStatus): string {
  switch (status.state) {
    case "running": return "Copying and verifying files";
    case "cancelling": return "Cancelling safely";
    case "cancelled": return "Move cancelled";
    case "failed": return "Move stopped";
    case "restart_required": return "New Library root is ready";
    case "completed": return "Library move completed";
    case "rolled_back": return "Library move rolled back";
    default: return "Library move";
  }
}

export function libraryMoveDescription(status: LibraryMoveStatus): string {
  if (status.error) return status.error;
  switch (status.state) {
    case "running": return "Source files remain untouched until activation.";
    case "cancelling": return "Temporary destination copies are being removed.";
    case "cancelled": return "The original Library is unchanged.";
    case "failed": return "The original Library is unchanged and created copies were cleaned up.";
    case "restart_required": return "The database and settings now reference the verified destination.";
    case "completed": return "The destination was verified after restart and old copies were removed.";
    case "rolled_back": return "Destination verification failed, so the original paths were restored.";
    default: return "";
  }
}
