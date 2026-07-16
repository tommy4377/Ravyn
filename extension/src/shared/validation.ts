import {
  MAX_RESOURCE_BATCH,
  type CreateBatchPayload,
  type CreateDownloadPayload,
  type DetectedResource,
} from "./contracts";
import { RavynExtensionError } from "./errors";
import { normalizeUrl } from "./urls";

export function validateDownloadPayload(
  input: CreateDownloadPayload,
): CreateDownloadPayload {
  const url = normalizeUrl(input.url);
  if (!url)
    throw new RavynExtensionError(
      "UNSUPPORTED_URL",
      "Only HTTP and HTTPS downloads are supported.",
    );
  const tags = [
    ...new Set((input.tags ?? []).map((tag) => tag.trim()).filter(Boolean)),
  ].slice(0, 32);
  return {
    ...input,
    url,
    filename: input.filename?.trim().slice(0, 255) || undefined,
    priority: clamp(input.priority ?? 0, -100, 100),
    tags,
    cookies: input.cookies?.slice(0, 500),
  };
}

export function validateBatchPayload(
  input: CreateBatchPayload,
): CreateBatchPayload {
  if (input.downloads.length === 0)
    throw new RavynExtensionError(
      "EMPTY_BATCH",
      "Select at least one resource.",
    );
  if (input.downloads.length > MAX_RESOURCE_BATCH)
    throw new RavynExtensionError(
      "BATCH_TOO_LARGE",
      `A browser batch may contain at most ${MAX_RESOURCE_BATCH} resources.`,
    );
  return { downloads: input.downloads.map(validateDownloadPayload) };
}

export function sanitizeDetectedResources(
  resources: DetectedResource[],
  maximum: number,
): DetectedResource[] {
  const byUrl = new Map<string, DetectedResource>();
  for (const resource of resources) {
    const normalizedUrl = normalizeUrl(resource.url, resource.pageUrl);
    if (!normalizedUrl) continue;
    const current = byUrl.get(normalizedUrl);
    const next = {
      ...resource,
      id: resource.id.slice(0, 128),
      url: normalizedUrl,
      normalizedUrl,
      pageUrl: normalizeUrl(resource.pageUrl) ?? resource.pageUrl,
      confidence: Math.min(1, Math.max(0, resource.confidence)),
      discoveredAt: Number.isFinite(resource.discoveredAt)
        ? resource.discoveredAt
        : Date.now(),
      title: resource.title?.slice(0, 500),
      filename: resource.filename?.slice(0, 255),
    };
    if (!current || next.confidence > current.confidence)
      byUrl.set(normalizedUrl, next);
    if (byUrl.size >= maximum) break;
  }
  return [...byUrl.values()];
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, Math.trunc(value)));
}
