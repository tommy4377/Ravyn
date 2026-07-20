/**
 * Authenticated SSE client for `/v1/events`.
 *
 * `EventSource` cannot attach the Authorization header required by Ravyn, so
 * this client consumes the SSE stream with `fetch`, preserves Last-Event-ID,
 * and reconnects with bounded exponential backoff.
 */

import type { RavynEvent } from "./types";

export type EventHandler = (event: RavynEvent) => void;

const INITIAL_RETRY_MS = 1_000;
const MAX_RETRY_MS = 10_000;

export class RavynEventClient {
  private controller: AbortController | null = null;
  private readonly handlers = new Set<EventHandler>();
  private lastEventId: string | null = null;
  connected = $state(false);

  constructor(
    private readonly baseUrl: string,
    private readonly apiToken: string,
  ) {}

  connect(): void {
    if (this.controller) return;
    const controller = new AbortController();
    this.controller = controller;
    void this.run(controller);
  }

  subscribe(handler: EventHandler): () => void {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  close(): void {
    this.controller?.abort();
    this.controller = null;
    this.connected = false;
  }

  private async run(controller: AbortController): Promise<void> {
    let retryMs = INITIAL_RETRY_MS;
    while (this.controller === controller && !controller.signal.aborted) {
      try {
        const headers: Record<string, string> = {
          Accept: "text/event-stream",
          Authorization: `Bearer ${this.apiToken}`,
        };
        if (this.lastEventId) headers["Last-Event-ID"] = this.lastEventId;
        const response = await fetch(`${this.baseUrl}/v1/events`, {
          headers,
          signal: controller.signal,
          cache: "no-store",
        });
        if (!response.ok || !response.body) {
          throw new Error(`SSE connection failed with HTTP ${response.status}`);
        }
        this.connected = true;
        retryMs = INITIAL_RETRY_MS;
        await this.consume(response.body, controller.signal);
      } catch (error) {
        if (controller.signal.aborted) return;
        console.warn("Ravyn event stream disconnected", error);
      } finally {
        if (this.controller === controller) this.connected = false;
      }
      if (this.controller !== controller || controller.signal.aborted) return;
      await abortableDelay(retryMs, controller.signal);
      retryMs = Math.min(MAX_RETRY_MS, retryMs * 2);
    }
  }

  private async consume(
    stream: ReadableStream<Uint8Array>,
    signal: AbortSignal,
  ): Promise<void> {
    const reader = stream.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    let eventId: string | null = null;
    let dataLines: string[] = [];

    const dispatch = (): void => {
      if (eventId !== null) this.lastEventId = eventId;
      if (dataLines.length === 0) {
        eventId = null;
        return;
      }
      const data = dataLines.join("\n");
      eventId = null;
      dataLines = [];
      if (!data || data === "keep-alive") return;
      let parsed: RavynEvent;
      try {
        parsed = JSON.parse(data) as RavynEvent;
      } catch {
        return;
      }
      for (const handler of this.handlers) handler(parsed);
    };

    try {
      while (!signal.aborted) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        let newline = buffer.indexOf("\n");
        while (newline >= 0) {
          let line = buffer.slice(0, newline);
          buffer = buffer.slice(newline + 1);
          if (line.endsWith("\r")) line = line.slice(0, -1);
          if (line === "") {
            dispatch();
          } else if (!line.startsWith(":")) {
            const separator = line.indexOf(":");
            const field = separator >= 0 ? line.slice(0, separator) : line;
            let value = separator >= 0 ? line.slice(separator + 1) : "";
            if (value.startsWith(" ")) value = value.slice(1);
            if (field === "data") dataLines.push(value);
            else if (field === "id" && !value.includes("\0")) eventId = value;
          }
          newline = buffer.indexOf("\n");
        }
      }
      buffer += decoder.decode();
      if (buffer.length > 0) {
        if (buffer.startsWith("data:")) dataLines.push(buffer.slice(5).trimStart());
        dispatch();
      }
    } finally {
      reader.releaseLock();
    }
  }
}

function abortableDelay(milliseconds: number, signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = window.setTimeout(done, milliseconds);
    signal.addEventListener("abort", done, { once: true });
    function done(): void {
      window.clearTimeout(timer);
      signal.removeEventListener("abort", done);
      resolve();
    }
  });
}
