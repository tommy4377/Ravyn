import { describe, expect, it } from "vitest";
import { ApiError, describeError } from "./errors";

describe("ApiError", () => {
  it("carries the backend's structured fields", () => {
    const error = new ApiError(409, {
      code: "JOB_CONFLICT",
      message: "the job is already running",
      request_id: "req-1",
      retryable: true,
      details: { jobId: "abc" },
    });
    expect(error.status).toBe(409);
    expect(error.code).toBe("JOB_CONFLICT");
    expect(error.requestId).toBe("req-1");
    expect(error.retryable).toBe(true);
    expect(error.details).toEqual({ jobId: "abc" });
  });

  it("defaults retryable to false when omitted", () => {
    const error = new ApiError(500, { code: "INTERNAL_ERROR", message: "boom" });
    expect(error.retryable).toBe(false);
  });
});

describe("describeError", () => {
  it("formats an ApiError with its stable code", () => {
    const error = new ApiError(404, { code: "JOB_NOT_FOUND", message: "no such job" });
    expect(describeError(error)).toBe("no such job (JOB_NOT_FOUND)");
  });

  it("formats a plain Error by its message", () => {
    expect(describeError(new Error("network down"))).toBe("network down");
  });

  it("stringifies non-Error values", () => {
    expect(describeError("weird failure")).toBe("weird failure");
  });
});
