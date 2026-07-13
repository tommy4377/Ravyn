/**
 * Global toast/notification queue. Errors persist until dismissed; other
 * severities auto-dismiss so the queue never silently piles up.
 */

import { SvelteMap } from "svelte/reactivity";

export type NotificationSeverity = "info" | "success" | "warning" | "error";

export interface AppNotification {
  id: string;
  severity: NotificationSeverity;
  title: string;
  message?: string;
  actionLabel?: string;
  onAction?: () => void;
  createdAt: number;
}

const AUTO_DISMISS_MS = 6_000;

class NotificationsStore {
  private readonly byId = new SvelteMap<string, AppNotification>();
  private readonly order: string[] = $state([]);

  get items(): AppNotification[] {
    return this.order
      .map((id) => this.byId.get(id))
      .filter((item): item is AppNotification => item !== undefined);
  }

  push(notification: Omit<AppNotification, "id" | "createdAt">): string {
    const id = crypto.randomUUID();
    this.byId.set(id, { ...notification, id, createdAt: Date.now() });
    this.order.push(id);
    if (notification.severity !== "error") {
      setTimeout(() => this.dismiss(id), AUTO_DISMISS_MS);
    }
    return id;
  }

  info(title: string, message?: string): string {
    return this.push({ severity: "info", title, message });
  }

  success(title: string, message?: string): string {
    return this.push({ severity: "success", title, message });
  }

  warning(title: string, message?: string): string {
    return this.push({ severity: "warning", title, message });
  }

  error(title: string, message?: string, actionLabel?: string, onAction?: () => void): string {
    return this.push({ severity: "error", title, message, actionLabel, onAction });
  }

  dismiss(id: string): void {
    this.byId.delete(id);
    const index = this.order.indexOf(id);
    if (index !== -1) this.order.splice(index, 1);
  }

  clear(): void {
    this.byId.clear();
    this.order.length = 0;
  }
}

export const notifications = new NotificationsStore();
