import {
  classifyResource,
  extensionFromUrl,
  filenameFromUrl,
  normalizeUrl,
} from "../../shared/urls";
import type { DetectedResource, ResourceSource } from "../../shared/contracts";

export interface ResourceInput {
  url: string;
  pageUrl: string;
  frameUrl?: string;
  mime?: string;
  source: ResourceSource;
  elementHint?: string;
  confidence?: number;
  title?: string;
  width?: number;
  height?: number;
}

export function normalizeResource(
  input: ResourceInput,
): DetectedResource | null {
  const normalizedUrl = normalizeUrl(
    input.url,
    input.frameUrl ?? input.pageUrl,
  );
  if (!normalizedUrl) return null;
  const type = classifyResource(normalizedUrl, input.mime, input.elementHint);
  return {
    id: `${input.source}:${stableId(normalizedUrl)}`,
    url: normalizedUrl,
    normalizedUrl,
    pageUrl: normalizeUrl(input.pageUrl) ?? input.pageUrl,
    frameUrl: input.frameUrl,
    type,
    mime: input.mime,
    extension: extensionFromUrl(normalizedUrl),
    filename: filenameFromUrl(normalizedUrl),
    source: input.source,
    confidence: input.confidence ?? 0.7,
    discoveredAt: Date.now(),
    title: input.title?.trim().slice(0, 500),
    width: input.width,
    height: input.height,
  };
}

function stableId(value: string): string {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(36);
}
