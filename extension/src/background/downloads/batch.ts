import type { CreateDownloadPayload } from "../../shared/contracts";
import { toExtensionError } from "../../shared/errors";
import type { NativeClient } from "../native/client";

const NATIVE_BATCH_CHUNK_SIZE = 50;
const MAX_NATIVE_PAYLOAD_BYTES = 700_000;
const MAX_CHUNK_ATTEMPTS = 2;

export interface BatchItemResult {
  ok: boolean;
  jobId?: string;
  error?: { code: string; message: string; retryable: boolean };
}

export interface BatchResult {
  attempted: number;
  accepted: number;
  failed: number;
  results: BatchItemResult[];
}

/**
 * Sends a large browser batch as bounded native-messaging chunks. Every item
 * gets a stable idempotency key for the lifetime of this operation, so a
 * retry after a native timeout cannot create the same job twice.
 */
export async function createBatchSafely(
  native: NativeClient,
  downloads: CreateDownloadPayload[],
): Promise<BatchResult> {
  const operationId = crypto.randomUUID();
  const prepared = downloads.map((download, index) => ({
    ...download,
    idempotencyKey:
      download.idempotencyKey ?? `firefox-batch-${operationId}-${index}`,
  }));
  const results: BatchItemResult[] = [];

  for (const chunk of buildChunks(prepared)) {
    let response: BatchResult | undefined;
    let lastError: unknown;
    for (let attempt = 0; attempt < MAX_CHUNK_ATTEMPTS; attempt += 1) {
      try {
        response = await native.request<BatchResult>("create_batch", {
          downloads: chunk,
        });
        break;
      } catch (error) {
        lastError = error;
        if (
          !toExtensionError(error).retryable ||
          attempt + 1 >= MAX_CHUNK_ATTEMPTS
        )
          throw error;
      }
    }
    if (!response) throw lastError;
    if (response.results.length !== chunk.length)
      throw new Error(
        "Native batch response length did not match the request.",
      );
    results.push(...response.results);
  }

  const accepted = results.filter((result) => result.ok).length;
  return {
    attempted: results.length,
    accepted,
    failed: results.length - accepted,
    results,
  };
}

function buildChunks(
  downloads: CreateDownloadPayload[],
): CreateDownloadPayload[][] {
  const encoder = new TextEncoder();
  const chunks: CreateDownloadPayload[][] = [];
  let current: CreateDownloadPayload[] = [];

  for (const download of downloads) {
    const candidate = [...current, download];
    const bytes = encoder.encode(
      JSON.stringify({ downloads: candidate }),
    ).byteLength;
    if (
      current.length > 0 &&
      (candidate.length > NATIVE_BATCH_CHUNK_SIZE ||
        bytes > MAX_NATIVE_PAYLOAD_BYTES)
    ) {
      chunks.push(current);
      current = [download];
    } else {
      current = candidate;
    }
    const singleBytes = encoder.encode(
      JSON.stringify({ downloads: current }),
    ).byteLength;
    if (singleBytes > MAX_NATIVE_PAYLOAD_BYTES)
      throw new Error(
        "A single download request is too large for native messaging.",
      );
  }
  if (current.length > 0) chunks.push(current);
  return chunks;
}
