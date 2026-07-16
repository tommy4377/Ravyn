import type { DetectedResource } from "../../shared/contracts";
import { normalizeResource } from "./normalizer";
import { parseSrcset } from "./srcset";

const MAX_STYLE_ELEMENTS = 1_000;

export function scanDocument(
  documentValue: Document = document,
): DetectedResource[] {
  const resources = new Map<string, DetectedResource>();
  const pageUrl = topLevelPageUrl();
  const frameUrl = documentValue.location.href;
  const add = (
    url: string | null | undefined,
    hint?: string,
    title?: string,
    width?: number,
    height?: number,
    confidence = 0.75,
  ): void => {
    if (!url) return;
    const resource = normalizeResource({
      url,
      pageUrl,
      frameUrl,
      source: "dom",
      elementHint: hint,
      title,
      width,
      height,
      confidence,
    });
    if (resource)
      resources.set(
        resource.normalizedUrl,
        prefer(resources.get(resource.normalizedUrl), resource),
      );
  };

  for (const anchor of documentValue.querySelectorAll<HTMLAnchorElement>(
    "a[href]",
  ))
    add(
      anchor.href,
      "a",
      anchor.textContent ?? anchor.title,
      undefined,
      undefined,
      0.55,
    );
  for (const image of documentValue.images) {
    add(
      image.currentSrc || image.src,
      "img",
      image.alt || image.title,
      image.naturalWidth,
      image.naturalHeight,
      0.9,
    );
    for (const candidate of parseSrcset(image.srcset))
      add(
        candidate.url,
        "img",
        image.alt || image.title,
        image.naturalWidth,
        image.naturalHeight,
        0.82,
      );
  }
  for (const source of documentValue.querySelectorAll<HTMLSourceElement>(
    "picture source[srcset]",
  )) {
    for (const candidate of parseSrcset(source.srcset))
      add(candidate.url, "picture", source.media, undefined, undefined, 0.85);
  }
  for (const video of documentValue.querySelectorAll<HTMLVideoElement>(
    "video",
  )) {
    add(
      video.currentSrc || video.src,
      "video",
      video.title,
      video.videoWidth,
      video.videoHeight,
      0.95,
    );
    add(
      video.poster,
      "img",
      `${video.title || "Video"} poster`,
      video.videoWidth,
      video.videoHeight,
      0.7,
    );
    for (const source of video.querySelectorAll<HTMLSourceElement>(
      "source[src]",
    ))
      add(
        source.src,
        "video",
        video.title,
        video.videoWidth,
        video.videoHeight,
        0.9,
      );
    for (const track of video.querySelectorAll<HTMLTrackElement>("track[src]"))
      add(track.src, "track", track.label, undefined, undefined, 0.75);
  }
  for (const audio of documentValue.querySelectorAll<HTMLAudioElement>(
    "audio",
  )) {
    add(
      audio.currentSrc || audio.src,
      "audio",
      audio.title,
      undefined,
      undefined,
      0.95,
    );
    for (const source of audio.querySelectorAll<HTMLSourceElement>(
      "source[src]",
    ))
      add(source.src, "audio", audio.title, undefined, undefined, 0.9);
  }
  for (const object of documentValue.querySelectorAll<HTMLObjectElement>(
    "object[data]",
  ))
    add(object.data, "object", object.title, undefined, undefined, 0.7);
  for (const embed of documentValue.querySelectorAll<HTMLEmbedElement>(
    "embed[src]",
  ))
    add(embed.src, "embed", embed.title, undefined, undefined, 0.7);
  for (const link of documentValue.querySelectorAll<HTMLLinkElement>(
    "link[href]",
  ))
    add(
      link.href,
      "link",
      link.rel,
      undefined,
      undefined,
      link.rel.includes("stylesheet") ? 0.25 : 0.5,
    );
  for (const script of documentValue.querySelectorAll<HTMLScriptElement>(
    "script[src]",
  ))
    add(script.src, "script", undefined, undefined, undefined, 0.2);

  let inspected = 0;
  for (const element of documentValue.querySelectorAll<HTMLElement>(
    "[style]",
  )) {
    if (inspected >= MAX_STYLE_ELEMENTS) break;
    inspected += 1;
    for (const url of cssUrls(element.style.backgroundImage))
      add(
        url,
        "img",
        element.getAttribute("aria-label") ?? element.title,
        element.clientWidth,
        element.clientHeight,
        0.65,
      );
  }

  for (const entry of performance.getEntriesByType("resource")) {
    const resource = normalizeResource({
      url: entry.name,
      pageUrl,
      frameUrl,
      source: "performance",
      confidence: 0.6,
    });
    if (resource)
      resources.set(
        resource.normalizedUrl,
        prefer(resources.get(resource.normalizedUrl), resource),
      );
  }

  return [...resources.values()];
}

function topLevelPageUrl(): string {
  try {
    return window.top?.location.href ?? location.href;
  } catch {
    return document.referrer || location.href;
  }
}

function cssUrls(value: string): string[] {
  const urls: string[] = [];
  const pattern = /url\((?:"([^"]+)"|'([^']+)'|([^)]+))\)/gi;
  for (const match of value.matchAll(pattern)) {
    const candidate = match[1] ?? match[2] ?? match[3];
    if (candidate) urls.push(candidate.trim());
  }
  return urls;
}

function prefer(
  current: DetectedResource | undefined,
  next: DetectedResource,
): DetectedResource {
  return !current || next.confidence >= current.confidence ? next : current;
}
