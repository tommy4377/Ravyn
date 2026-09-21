/**
 * Structured API error type and human-facing error description shared by
 * every service and store. Centralized here per the frontend architecture
 * so components never need to know about transport-level failure shapes.
 */

import type { ApiErrorBody } from "./types";

export class ApiError extends Error {
  readonly code: string;
  readonly status: number;
  readonly requestId: string | undefined;
  readonly retryable: boolean;
  readonly details: unknown;

  constructor(status: number, body: ApiErrorBody) {
    super(body.message);
    this.name = "ApiError";
    this.code = body.code;
    this.status = status;
    this.requestId = body.request_id;
    this.retryable = body.retryable ?? false;
    this.details = body.details;
  }
}

/** Render any thrown value as a concise, user-facing message. */
export function describeError(error: unknown): string {
  if (error instanceof ApiError) {
    return `${error.message} (${error.code})`;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
