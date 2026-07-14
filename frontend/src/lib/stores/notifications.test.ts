import { beforeEach, describe, expect, it, vi } from "vitest";
import { notifications } from "./notifications.svelte";

describe("notification history", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    notifications.clear();
  });

  it("keeps auto-dismissed notifications in history", () => {
    notifications.success("Download completed", "archive.zip is ready.");

    expect(notifications.items).toHaveLength(1);
    expect(notifications.history).toHaveLength(1);
    expect(notifications.unreadCount).toBe(1);

    vi.advanceTimersByTime(6_000);

    expect(notifications.items).toHaveLength(0);
    expect(notifications.history).toHaveLength(1);
  });

  it("tracks read state independently from toast visibility", () => {
    const id = notifications.warning("Verification recommended");

    notifications.dismiss(id);
    notifications.markRead(id);

    expect(notifications.items).toHaveLength(0);
    expect(notifications.history[0]?.read).toBe(true);
    expect(notifications.unreadCount).toBe(0);
  });

  it("clears history without affecting currently visible toasts", () => {
    notifications.error("Download failed", "The server closed the connection.");

    notifications.clearHistory();

    expect(notifications.history).toHaveLength(0);
    expect(notifications.items).toHaveLength(1);
  });
});
