/**
 * Low-level HTTP transport for the embedded Ravyn backend: bearer-token
 * auth, bounded timeouts, abort support, and structured-error normalization.
 * `RavynClient` (client.ts) is the typed surface built on top of this.
 */

import { ApiError } from "./errors";
import type { ApiErrorBody } from "./types";

const DEFAULT_TIMEOUT_MS = 30_000;

export interface RequestOptions {
  query?: Record<string, string | number | boolean | undefined>;
  signal?: AbortSignal;
  timeoutMs?: number;
  headers?: Record<string, string>;
  /** Non-success statuses that still carry a valid response payload. */
  acceptedStatuses?: number[];
}

function buildUrl(baseUrl: string, path: string, query?: RequestOptions["query"]): string {
  if (!query) return `${baseUrl}${path}`;
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined) params.set(key, String(value));
  }
  const qs = params.toString();
  return qs ? `${baseUrl}${path}?${qs}` : `${baseUrl}${path}`;
}

export async function httpRequest<T>(
  baseUrl: string,
  apiToken: string,
  method: string,
  path: string,
  body?: unknown,
  options?: RequestOptions,
): Promise<T> {
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(),
    options?.timeoutMs ?? DEFAULT_TIMEOUT_MS,
  );
  // Named handler so it can be detached once the request settles — with a
  // long-lived caller signal (a view-level controller reused across polls),
  // `{once:true}` alone would let one listener per request pile up on the
  // signal until it finally aborts.
  const onAbort = () => controller.abort();
  // An already-aborted signal never fires "abort" listeners — without this
  // check a request issued after its owner aborted (a poll racing view
  // teardown) would run to completion or the full timeout instead.
  if (options?.signal?.aborted) controller.abort();
  else options?.signal?.addEventListener("abort", onAbort, { once: true });

  try {
    let response: Response;
    try {
      response = await fetch(buildUrl(baseUrl, path, options?.query), {
        method,
        headers: {
          ...(body !== undefined ? { "content-type": "application/json" } : {}),
          authorization: `Bearer ${apiToken}`,
          ...options?.headers,
        },
        body: body !== undefined ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });
    } catch (error) {
      throw new ApiError(0, {
        code: "NETWORK_UNAVAILABLE",
        message:
          error instanceof Error ? error.message : "the backend is unreachable",
        retryable: true,
      });
    }

    const accepted = response.ok || options?.acceptedStatuses?.includes(response.status) === true;
    if (!accepted) {
      let parsed: ApiErrorBody;
      try {
        parsed = (await response.json()) as ApiErrorBody;
      } catch {
        parsed = {
          code: `HTTP_${response.status}`,
          message: response.statusText || "request failed",
          retryable: response.status >= 500,
        };
      }
      throw new ApiError(response.status, parsed);
    }

    if (response.status === 204) {
      return undefined as T;
    }
    const contentType = response.headers.get("content-type") ?? "";
    if (!contentType.includes("application/json")) {
      return (await response.text()) as T;
    }
    return (await response.json()) as T;
  } finally {
    // Cleared only after the body is consumed, so the timeout bounds the
    // whole request — previously it stopped guarding once headers arrived,
    // leaving a stalled body stream hanging forever.
    clearTimeout(timeout);
    options?.signal?.removeEventListener("abort", onAbort);
  }
}
