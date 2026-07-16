/**
 * Global toast queue plus an in-memory history of important product events.
 * Auto-dismiss only removes the transient toast; history remains available
 * until the user clears it or the application process exits.
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
  read: boolean;
}

const AUTO_DISMISS_MS = 6_000;
const HISTORY_LIMIT = 100;
const VISIBLE_LIMIT = 4;
const HISTORY_STORAGE_KEY = "ravyn.notifications.history";
const HISTORY_MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;

class NotificationsStore {
  private initialized = false;
  private readonly visibleById = new SvelteMap<string, AppNotification>();
  private readonly visibleOrder: string[] = $state([]);
  private readonly historyById = new SvelteMap<string, AppNotification>();
  private readonly historyOrder: string[] = $state([]);


  init(): void {
    if (this.initialized) return;
    this.initialized = true;
    if (typeof localStorage === "undefined") return;
    try {
      const parsed: unknown = JSON.parse(localStorage.getItem(HISTORY_STORAGE_KEY) ?? "[]");
      if (!Array.isArray(parsed)) return;
      const cutoff = Date.now() - HISTORY_MAX_AGE_MS;
      for (const value of parsed.slice(-HISTORY_LIMIT)) {
        if (!isPersistedNotification(value) || value.createdAt < cutoff) continue;
        const item: AppNotification = { ...value };
        this.historyById.set(item.id, item);
        this.historyOrder.push(item.id);
      }
      this.persistHistory();
    } catch {
      localStorage.removeItem(HISTORY_STORAGE_KEY);
    }
  }

  get items(): AppNotification[] {
    return this.visibleOrder
      .map((id) => this.visibleById.get(id))
      .filter((item): item is AppNotification => item !== undefined);
  }

  get history(): AppNotification[] {
    return [...this.historyOrder]
      .reverse()
      .map((id) => this.historyById.get(id))
      .filter((item): item is AppNotification => item !== undefined);
  }

  get unreadCount(): number {
    return this.historyOrder.reduce((count, id) => count + (this.historyById.get(id)?.read ? 0 : 1), 0);
  }

  push(notification: Omit<AppNotification, "id" | "createdAt" | "read">): string {
    const id = crypto.randomUUID();
    const item: AppNotification = { ...notification, id, createdAt: Date.now(), read: false };
    this.visibleById.set(id, item);
    this.visibleOrder.push(id);
    while (this.visibleOrder.length > VISIBLE_LIMIT) {
      const oldestId = this.visibleOrder.shift();
      if (oldestId) this.visibleById.delete(oldestId);
    }
    this.historyById.set(id, item);
    this.historyOrder.push(id);
    this.trimHistory();
    this.persistHistory();
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
    this.visibleById.delete(id);
    const index = this.visibleOrder.indexOf(id);
    if (index !== -1) this.visibleOrder.splice(index, 1);
  }

  markRead(id: string): void {
    const item = this.historyById.get(id);
    if (!item || item.read) return;
    this.historyById.set(id, { ...item, read: true });
    this.persistHistory();
  }

  markAllRead(): void {
    for (const id of this.historyOrder) this.markRead(id);
  }

  clearVisible(): void {
    this.visibleById.clear();
    this.visibleOrder.length = 0;
  }

  clearHistory(): void {
    this.historyById.clear();
    this.historyOrder.length = 0;
    this.persistHistory();
  }

  clear(): void {
    this.clearVisible();
    this.clearHistory();
  }

  private persistHistory(): void {
    if (typeof localStorage === "undefined") return;
    const serializable = this.historyOrder
      .map((id) => this.historyById.get(id))
      .filter((item): item is AppNotification => item !== undefined)
      .map(({ id, severity, title, message, actionLabel, createdAt, read }) => ({
        id, severity, title, message, actionLabel, createdAt, read,
      }));
    try {
      localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(serializable));
    } catch {
      // Notification persistence is optional and must never break the UI.
    }
  }

  private trimHistory(): void {
    while (this.historyOrder.length > HISTORY_LIMIT) {
      const id = this.historyOrder.shift();
      if (id) {
        this.historyById.delete(id);
        this.visibleById.delete(id);
        const visibleIndex = this.visibleOrder.indexOf(id);
        if (visibleIndex !== -1) this.visibleOrder.splice(visibleIndex, 1);
      }
    }
  }
}

function isPersistedNotification(value: unknown): value is AppNotification {
  if (!value || typeof value !== "object") return false;
  const item = value as Partial<AppNotification>;
  return typeof item.id === "string"
    && ["info", "success", "warning", "error"].includes(item.severity ?? "")
    && typeof item.title === "string"
    && typeof item.createdAt === "number"
    && typeof item.read === "boolean"
    && (item.message === undefined || typeof item.message === "string")
    && (item.actionLabel === undefined || typeof item.actionLabel === "string");
}

export const notifications = new NotificationsStore();
