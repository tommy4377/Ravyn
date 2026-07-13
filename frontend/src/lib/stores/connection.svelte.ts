/**
 * Owns the main window's connection to the embedded backend: the typed
 * client, the SSE event client, and the connect/retry lifecycle. A single
 * module-level instance is shared by the whole app shell.
 */

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
    try {
      const backend = await backendInfo();
      const client = new RavynClient(backend.base_url, backend.api_token);
      const setupState = await client.getSetupState();
      const events = new RavynEventClient(backend.base_url, backend.api_token);
      events.connect();

      this.client = client;
      this.events = events;
      this.setupState = setupState;
      this.status = "ready";
      await mainWindowReady();
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
