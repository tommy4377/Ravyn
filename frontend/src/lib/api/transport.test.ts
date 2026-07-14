import { afterEach, describe, expect, it, vi } from "vitest";
import { httpRequest } from "./transport";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("httpRequest", () => {
  it("keeps JSON bodies returned with 202 Accepted", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({ id: "import-1", state: "running" }), {
      status: 202,
      headers: { "content-type": "application/json" },
    })));

    const result = await httpRequest<{ id: string; state: string }>("http://127.0.0.1:1", "token", "POST", "/v1/library/import", { root: "C:/Downloads" });

    expect(result).toEqual({ id: "import-1", state: "running" });
  });

  it("can accept a structured 503 readiness response", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({ ready: false, checks: [{ name: "database", healthy: false }] }), {
      status: 503,
      headers: { "content-type": "application/json" },
    })));

    const result = await httpRequest<{ ready: boolean }>("http://127.0.0.1:1", "token", "GET", "/health/ready", undefined, { acceptedStatuses: [503] });

    expect(result.ready).toBe(false);
  });

  it("returns undefined for 204 No Content", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status: 204 })));

    await expect(httpRequest<void>("http://127.0.0.1:1", "token", "DELETE", "/v1/jobs/1")).resolves.toBeUndefined();
  });

  it("normalizes backend error payloads", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({
      code: "INVALID_REQUEST",
      message: "invalid source",
      retryable: false,
    }), {
      status: 400,
      headers: { "content-type": "application/json" },
    })));

    await expect(httpRequest("http://127.0.0.1:1", "token", "POST", "/v1/jobs", {})).rejects.toMatchObject({
      status: 400,
      code: "INVALID_REQUEST",
      message: "invalid source",
    });
  });
});
