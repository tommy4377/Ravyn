/**
 * Typed HTTP client for the embedded Ravyn backend.
 *
 * Errors are normalized to `ApiError` carrying the backend's stable error
 * code; network failures map to code `NETWORK_UNAVAILABLE`.
 */

import type {
  ComponentId,
  ComponentOverview,
  ApiErrorBody,
  FeatureSelection,
  PrepareLibraryResult,
  SetupProfile,
  SetupState,
} from "./types";

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

const DEFAULT_TIMEOUT_MS = 30_000;

export class RavynClient {
  constructor(
    readonly baseUrl: string,
    private readonly apiToken: string,
  ) {}

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    signal?: AbortSignal,
  ): Promise<T> {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), DEFAULT_TIMEOUT_MS);
    signal?.addEventListener("abort", () => controller.abort(), {
      once: true,
    });

    let response: Response;
    try {
      response = await fetch(`${this.baseUrl}${path}`, {
        method,
        headers: {
          ...(body !== undefined ? { "content-type": "application/json" } : {}),
          authorization: `Bearer ${this.apiToken}`,
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
    } finally {
      clearTimeout(timeout);
    }

    if (!response.ok) {
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

    if (response.status === 204 || response.status === 202) {
      return undefined as T;
    }
    return (await response.json()) as T;
  }

  // --- Setup ---

  getSetupState(signal?: AbortSignal): Promise<SetupState> {
    return this.request("GET", "/v1/setup", undefined, signal);
  }

  prepareLibrary(path: string): Promise<PrepareLibraryResult> {
    return this.request("POST", "/v1/setup/library", { path });
  }

  completeSetup(): Promise<SetupState> {
    return this.request("POST", "/v1/setup/complete");
  }

  // --- Components ---

  getComponents(signal?: AbortSignal): Promise<ComponentOverview> {
    return this.request("GET", "/v1/components", undefined, signal);
  }

  saveFeatureSelections(
    setupProfile: SetupProfile,
    features: FeatureSelection[],
  ): Promise<ComponentOverview> {
    return this.request("POST", "/v1/components/features", {
      setup_profile: setupProfile,
      features,
    });
  }

  installComponent(id: ComponentId, force = false): Promise<void> {
    return this.request("POST", `/v1/components/${wireId(id)}/install`, {
      force,
    });
  }

  cancelComponentInstallation(id: ComponentId): Promise<void> {
    return this.request("POST", `/v1/components/${wireId(id)}/cancel`);
  }
}

/** Route path segment for a component id (differs from the JSON enum). */
function wireId(id: ComponentId): string {
  switch (id) {
    case "ytdlp":
      return "yt-dlp";
    case "seven_zip":
      return "7zip";
    default:
      return id;
  }
}
