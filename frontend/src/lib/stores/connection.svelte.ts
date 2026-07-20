/**
 * Owns the main window's connection to the embedded backend: the typed
 * client, the SSE event client, and the connect/retry lifecycle. A single
 * module-level instance is shared by the whole app shell.
 */

import { untrack } from "svelte";

import { RavynClient } from "../api/client";
import { describeError } from "../api/errors";
import { RavynEventClient } from "../api/events.svelte";
import type { SetupState } from "../api/types";
import { backendInfo, mainWindowReady } from "../native/tauri";

export type ConnectionStatus = "connecting" | "ready" | "error";

class ConnectionStore {
  status = $state<ConnectionStatus>("connecting");
  errorMessage = $state("");
  client = $state<RavynClient | null>(null);
  events = $state<RavynEventClient | null>(null);
  setupState = $state<SetupState | null>(null);

  async connect(): Promise<void> {
    this.status = "connecting";
    this.errorMessage = "";
    // A retry after a previous successful-but-then-failed connect (e.g.
    // mainWindowReady() rejecting after events.connect() already opened a
    // live EventSource) must not leak that earlier connection — nothing
    // else holds a reference to it once this.events is reassigned below.
    // Untracked: connect() runs inside AppShell's $effect, and a tracked
    // read of this.events here would make that effect re-run on the
    // `this.events = events` assignment below — an infinite connect loop
    // that pins the main window on the "Connecting…" boot screen.
    untrack(() => this.events)?.close();
    try {
      const backend = await backendInfo();
      const client = new RavynClient(backend.base_url, backend.api_token);
      const setupState = await client.getSetupState();
      const events = new RavynEventClient(backend.base_url, backend.api_token);
      events.connect();
      try {
        await mainWindowReady();
      } catch (error) {
        events.close();
        throw error;
      }

      this.client = client;
      this.events = events;
      this.setupState = setupState;
      this.status = "ready";
    } catch (cause) {
      this.errorMessage = describeError(cause);
      this.status = "error";
      try {
        // Still show the window so the user sees the error instead of nothing.
        await mainWindowReady();
      } catch {
        // The setup window may already be gone; ignore.
      }
    }
  }
}

export const connection = new ConnectionStore();
