/**
 * SSE client for `/v1/events`.
 *
 * The browser's `EventSource` reconnects automatically and resends
 * `Last-Event-ID`, which the backend uses to replay missed events. On a
 * `resync_required` event consumers must refetch their full state.
 */

import type { RavynEvent } from "./types";

export type EventHandler = (event: RavynEvent) => void;

export class RavynEventClient {
  private source: EventSource | null = null;
  private readonly handlers = new Set<EventHandler>();
  connected = $state(false);

  constructor(private readonly baseUrl: string) {}

  connect(): void {
    if (this.source) return;
    const source = new EventSource(`${this.baseUrl}/v1/events`);
    this.source = source;
    source.onopen = () => {
      this.connected = true;
    };
    source.onerror = () => {
      // EventSource retries automatically; reflect the gap in the UI.
      this.connected = false;
    };
    source.onmessage = (message) => {
      if (!message.data || message.data === "keep-alive") return;
      let parsed: RavynEvent;
      try {
        parsed = JSON.parse(message.data) as RavynEvent;
      } catch {
        return;
      }
      for (const handler of this.handlers) {
        handler(parsed);
      }
    };
  }

  subscribe(handler: EventHandler): () => void {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  close(): void {
    this.source?.close();
    this.source = null;
    this.connected = false;
  }
}
