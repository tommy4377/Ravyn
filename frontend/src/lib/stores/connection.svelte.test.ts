// @vitest-environment jsdom

import { cleanup, render, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { connection } from "./connection.svelte";
import ConnectEffectHarness from "./ConnectEffectHarness.svelte";

const backendInfo = vi.fn(async () => ({
  base_url: "http://127.0.0.1:1",
  api_token: "token",
}));
const mainWindowReady = vi.fn(async () => {});

vi.mock("../native/tauri", () => ({
  backendInfo: (...args: unknown[]) => backendInfo(...(args as [])),
  mainWindowReady: (...args: unknown[]) => mainWindowReady(...(args as [])),
}));

const eventSignals: AbortSignal[] = [];

function installFetchMock(): void {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith("/v1/events")) {
        const signal = init?.signal;
        if (signal) eventSignals.push(signal);
        let controller: ReadableStreamDefaultController<Uint8Array> | undefined;
        const body = new ReadableStream<Uint8Array>({
          start(value) {
            controller = value;
          },
        });
        signal?.addEventListener(
          "abort",
          () => {
            try {
              controller?.close();
            } catch {
              // The stream may already be closed by the test environment.
            }
          },
          { once: true },
        );
        return new Response(body, {
          status: 200,
          headers: { "content-type": "text/event-stream" },
        });
      }
      return new Response(JSON.stringify({ completed: true }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }),
  );
}

describe("ConnectionStore.connect", () => {
  beforeEach(() => {
    connection.events?.close();
    eventSignals.length = 0;
    installFetchMock();
    backendInfo.mockClear();
    mainWindowReady.mockClear();
  });

  afterEach(() => {
    connection.events?.close();
    cleanup();
    vi.unstubAllGlobals();
  });

  it("does not re-trigger a calling $effect when it reassigns this.events", async () => {
    // AppShell boots the connection from inside an $effect. connect() closes
    // any previous SSE client before reconnecting; if that read of
    // `this.events` were tracked, the `this.events = events` assignment later
    // in the same call would re-run the effect and create a connect loop.
    let effectRuns = 0;
    render(ConnectEffectHarness, {
      props: { onRun: () => (effectRuns += 1) },
    });

    await waitFor(() => expect(connection.status).toBe("ready"));
    await new Promise((resolve) => setTimeout(resolve, 20));

    expect(effectRuns).toBe(1);
    expect(backendInfo).toHaveBeenCalledTimes(1);
    expect(eventSignals).toHaveLength(1);
    expect(eventSignals[0]!.aborted).toBe(false);
  });

  it("aborts the previous authenticated SSE stream when a retry reconnects", async () => {
    await connection.connect();
    await connection.connect();
    expect(eventSignals).toHaveLength(2);
    expect(eventSignals[0]!.aborted).toBe(true);
    expect(eventSignals[1]!.aborted).toBe(false);
  });
});
