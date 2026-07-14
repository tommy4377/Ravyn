import type { AppUpdateStatus } from "../native/tauri";

export function appUpdateHeading(status: AppUpdateStatus): string {
  if (status.repair_mode && status.phase === "downloading") return "Downloading repair package…";
  if (status.repair_mode && status.phase === "ready") return `Ravyn ${status.current_version} repair is ready`;
  if (status.repair_mode && status.phase === "installing") return "Repairing Ravyn…";
  switch (status.phase) {
    case "checking": return "Checking for updates…";
    case "downloading": return `Downloading Ravyn ${status.available_version ?? "update"}…`;
    case "cancelling": return "Stopping update download…";
    case "cancelled": return "Update download cancelled";
    case "ready": return `Ravyn ${status.available_version ?? "update"} is ready`;
    case "installing": return "Installing update…";
    case "up_to_date": return `Ravyn ${status.current_version} is up to date`;
    case "error": return "Update check failed";
    case "disabled": return "Application updates are unavailable";
    default: return `Ravyn ${status.current_version}`;
  }
}

export function appUpdateDescription(status: AppUpdateStatus): string {
  if (status.phase === "ready") {
    return status.repair_mode
      ? "The verified installer will reinstall Ravyn after a normal close."
      : "The verified installer will run silently after you close Ravyn.";
  }
  if (status.phase === "downloading") {
    const total = status.total_bytes ?? 0;
    const percent = total > 0 ? Math.min(100, Math.round(status.downloaded_bytes / total * 100)) : 0;
    return total > 0 ? `${percent}% downloaded` : "Downloading and verifying the signed installer.";
  }
  if (status.phase === "cancelling") return "Ravyn is stopping the network request and removing partial update files.";
  if (status.phase === "cancelled") return "No installer is staged. Automatic checks will continue in the background.";
  if (status.last_error) return status.last_error;
  if (!status.automatic) return "Automatic updates require an installed Windows build.";
  return "Ravyn checks periodically in the background and installs downloaded updates only when the app closes.";
}

export function canCancelAppUpdate(status: AppUpdateStatus): boolean {
  return status.phase === "checking" || status.phase === "downloading" || status.phase === "ready";
}

export function canInstallAppUpdateNow(status: AppUpdateStatus): boolean {
  return status.phase === "ready" && status.install_on_exit;
}

export function formatAppUpdateTime(value: number | null): string {
  if (value === null) return "Not yet";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "Unknown" : date.toLocaleString();
}
