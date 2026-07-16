import type { DetectedResource } from "../../shared/contracts";
import { normalizeResource } from "../scanner/normalizer";

export function collectMediaSources(
  element: HTMLMediaElement,
): DetectedResource[] {
  const resources: DetectedResource[] = [];
  const pageUrl = location.href;
  const add = (url: string | null | undefined, confidence: number): void => {
    if (!url || url.startsWith("blob:")) return;
    const resource = normalizeResource({
      url,
      pageUrl,
      source: "video-element",
      elementHint: element instanceof HTMLVideoElement ? "video" : "audio",
      confidence,
      title: element.title || document.title,
      width:
        element instanceof HTMLVideoElement ? element.videoWidth : undefined,
      height:
        element instanceof HTMLVideoElement ? element.videoHeight : undefined,
    });
    if (resource) resources.push(resource);
  };
  add(element.currentSrc, 1);
  add(element.src, 0.95);
  for (const source of element.querySelectorAll<HTMLSourceElement>(
    "source[src]",
  ))
    add(source.src, 0.9);
  return resources;
}
