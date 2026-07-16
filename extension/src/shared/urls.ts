import type { ResourceKind } from "./contracts";

const ARCHIVE_EXTENSIONS = new Set([
  "7z",
  "bz2",
  "gz",
  "rar",
  "tar",
  "tgz",
  "xz",
  "zip",
]);
const VIDEO_EXTENSIONS = new Set([
  "avi",
  "m2ts",
  "m4v",
  "mkv",
  "mov",
  "mp4",
  "mpeg",
  "mpg",
  "webm",
]);
const AUDIO_EXTENSIONS = new Set([
  "aac",
  "flac",
  "m4a",
  "mp3",
  "ogg",
  "opus",
  "wav",
  "wma",
]);
const IMAGE_EXTENSIONS = new Set([
  "avif",
  "bmp",
  "gif",
  "heic",
  "jpeg",
  "jpg",
  "png",
  "svg",
  "tif",
  "tiff",
  "webp",
]);
const DOCUMENT_EXTENSIONS = new Set([
  "csv",
  "doc",
  "docx",
  "epub",
  "json",
  "odt",
  "pdf",
  "ppt",
  "pptx",
  "txt",
  "xls",
  "xlsx",
  "xml",
]);
const MANIFEST_EXTENSIONS = new Set(["m3u8", "mpd"]);

export function normalizeUrl(input: string, base?: string): string | null {
  try {
    const url = new URL(input, base);
    if (!isSupportedScheme(url.protocol) || url.username || url.password)
      return null;
    url.hash = "";
    return url.href;
  } catch {
    return null;
  }
}

export function isSupportedScheme(protocol: string): boolean {
  return protocol === "http:" || protocol === "https:";
}

export function extensionFromUrl(input: string): string | undefined {
  try {
    const pathname = new URL(input).pathname;
    const filename = pathname.slice(pathname.lastIndexOf("/") + 1);
    const dot = filename.lastIndexOf(".");
    if (dot <= 0 || dot === filename.length - 1) return undefined;
    const extension = filename.slice(dot + 1).toLowerCase();
    return /^[a-z0-9]{1,16}$/.test(extension) ? extension : undefined;
  } catch {
    return undefined;
  }
}

export function filenameFromUrl(input: string): string | undefined {
  try {
    const pathname = new URL(input).pathname;
    const value = decodeURIComponent(
      pathname.slice(pathname.lastIndexOf("/") + 1),
    );
    return value && value.length <= 255 ? value : undefined;
  } catch {
    return undefined;
  }
}

export function classifyResource(
  url: string,
  mime?: string,
  elementHint?: string,
): ResourceKind {
  const normalizedMime = mime?.split(";", 1)[0]?.trim().toLowerCase();
  if (normalizedMime?.startsWith("image/")) return "image";
  if (normalizedMime?.startsWith("video/")) return "video";
  if (normalizedMime?.startsWith("audio/")) return "audio";
  if (
    normalizedMime === "application/vnd.apple.mpegurl" ||
    normalizedMime === "application/x-mpegurl" ||
    normalizedMime === "application/dash+xml"
  )
    return "manifest";
  if (
    normalizedMime === "application/pdf" ||
    normalizedMime?.startsWith("text/")
  )
    return "document";
  const extension = extensionFromUrl(url);
  if (extension && MANIFEST_EXTENSIONS.has(extension)) return "manifest";
  if (extension && IMAGE_EXTENSIONS.has(extension)) return "image";
  if (extension && VIDEO_EXTENSIONS.has(extension)) return "video";
  if (extension && AUDIO_EXTENSIONS.has(extension)) return "audio";
  if (extension && ARCHIVE_EXTENSIONS.has(extension)) return "archive";
  if (extension && DOCUMENT_EXTENSIONS.has(extension)) return "document";
  if (elementHint === "video") return "video";
  if (elementHint === "audio") return "audio";
  if (elementHint === "img" || elementHint === "picture") return "image";
  return "other";
}

export function originPattern(input: string): string | null {
  try {
    const url = new URL(input);
    if (!isSupportedScheme(url.protocol)) return null;
    return `${url.protocol}//${url.host}/*`;
  } catch {
    return null;
  }
}

export function domainMatches(pattern: string, host: string): boolean {
  const normalizedPattern = pattern.trim().toLowerCase();
  const normalizedHost = host.trim().toLowerCase();
  if (!normalizedPattern || !normalizedHost) return false;
  const suffix = normalizedPattern.startsWith("*.")
    ? normalizedPattern.slice(2)
    : normalizedPattern;
  return (
    normalizedHost === suffix ||
    (normalizedPattern.startsWith("*.") &&
      normalizedHost.endsWith(`.${suffix}`))
  );
}

export function urlHashInput(url: string): string {
  return normalizeUrl(url) ?? url;
}
