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

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: (() => void) | null = null;
  closed = false;
  constructor(public readonly url: string) {
    FakeEventSource.instances.push(this);
  }
  close(): void {
    this.closed = true;
  }
}

describe("ConnectionStore.connect", () => {
  beforeEach(() => {
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        status: 200,
        headers: new Headers({ "content-type": "application/json" }),
        json: async () => ({ completed: true }),
      })),
    );
    FakeEventSource.instances = [];
    backendInfo.mockClear();
    mainWindowReady.mockClear();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("does not re-trigger a calling $effect when it reassigns this.events", async () => {
    // AppShell boots the connection from inside an $effect. connect() closes
    // any previous EventSource before reconnecting; if that read of
    // `this.events` were tracked, the `this.events = events` assignment
    // later in the same call would re-run the effect — an infinite connect
    // loop that pins the main window on the "Connecting…" boot screen.
    let effectRuns = 0;
    render(ConnectEffectHarness, {
      props: { onRun: () => (effectRuns += 1) },
    });

    await waitFor(() => expect(connection.status).toBe("ready"));
    // Give any spuriously-scheduled effect re-run a chance to land.
    await new Promise((resolve) => setTimeout(resolve, 20));

    expect(effectRuns).toBe(1);
    expect(backendInfo).toHaveBeenCalledTimes(1);
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
  });

  it("closes the previous EventSource when a retry reconnects", async () => {
    await connection.connect();
    await connection.connect();
    expect(FakeEventSource.instances).toHaveLength(2);
    expect(FakeEventSource.instances[0]!.closed).toBe(true);
    expect(FakeEventSource.instances[1]!.closed).toBe(false);
  });
});
