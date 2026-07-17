import type { ResourceKind } from "../../shared/contracts";
import { classifyResource, extensionFromUrl } from "../../shared/urls";

const SEGMENT_EXTENSIONS = new Set(["m4s", "ts"]);

export function classifyObservedRequest(
  url: string,
  mime?: string,
  requestType?: string,
): { kind: ResourceKind; ignore: boolean; isSegment: boolean } {
  const extension = extensionFromUrl(url);
  if (extension && SEGMENT_EXTENSIONS.has(extension))
    return { kind: "other", ignore: true, isSegment: true };
  const elementHint = requestType === "media" ? "video" : undefined;
  const kind = classifyResource(url, mime, elementHint);
  const interesting =
    kind !== "other" || requestType === "media" || requestType === "object";
  return { kind, ignore: !interesting, isSegment: false };
}
