export type DetectedSourceKind = "http" | "media" | "torrent" | "metalink" | "batch";

export interface SourceDetection {
  kind: DetectedSourceKind;
  label: string;
  description: string;
  multiple: boolean;
  confidence: "high" | "medium" | "low";
}

const MEDIA_HOSTS = [
  "youtube.com",
  "youtu.be",
  "vimeo.com",
  "dailymotion.com",
  "twitch.tv",
  "soundcloud.com",
  "bandcamp.com",
  "tiktok.com",
  "instagram.com",
  "x.com",
  "twitter.com",
];

function sourceLines(input: string): string[] {
  return input
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#") && !line.startsWith("//"));
}

function looksLikeBatchJson(value: string): boolean {
  if (!value.trimStart().startsWith("[")) return false;
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) && parsed.some((entry) => typeof entry === "object" && entry !== null);
  } catch {
    return false;
  }
}

function looksLikeMetalink(value: string): boolean {
  const normalized = value.trim().toLowerCase();
  return (
    normalized.endsWith(".metalink") ||
    normalized.endsWith(".meta4") ||
    (normalized.includes("<metalink") && normalized.includes("</metalink>")) ||
    normalized.includes("urn:ietf:params:xml:ns:metalink")
  );
}

function looksLikeMediaUrl(value: string): boolean {
  try {
    const url = new URL(value);
    const host = url.hostname.toLowerCase().replace(/^www\./, "");
    return MEDIA_HOSTS.some((candidate) => host === candidate || host.endsWith(`.${candidate}`));
  } catch {
    return false;
  }
}

export function detectSource(input: string): SourceDetection {
  const lines = sourceLines(input);
  const multiple = lines.length > 1;

  if (!input.trim()) {
    return {
      kind: "http",
      label: "Waiting for a source",
      description: "Paste a link, magnet, file path, or multiple links.",
      multiple: false,
      confidence: "low",
    };
  }

  if (looksLikeBatchJson(input)) {
    return {
      kind: "batch",
      label: "Batch document",
      description: "A structured batch of download jobs was detected.",
      multiple: false,
      confidence: "high",
    };
  }

  if (looksLikeMetalink(input)) {
    return {
      kind: "metalink",
      label: "Metalink document",
      description: "Mirrors and integrity information will be read from the document.",
      multiple: false,
      confidence: "high",
    };
  }

  if (multiple) {
    return {
      kind: "http",
      label: `${lines.length} direct downloads`,
      description: "Each non-empty line will be added as a separate download.",
      multiple: true,
      confidence: "high",
    };
  }

  const source = lines[0] ?? input.trim();
  const normalized = source.toLowerCase();
  if (normalized.startsWith("magnet:") || normalized.endsWith(".torrent")) {
    return {
      kind: "torrent",
      label: "Torrent source",
      description: "Ravyn will inspect the torrent contents automatically.",
      multiple: false,
      confidence: "high",
    };
  }

  if (looksLikeMediaUrl(source)) {
    return {
      kind: "media",
      label: "Media source",
      description: "Ravyn will inspect the available formats automatically.",
      multiple: false,
      confidence: "high",
    };
  }

  return {
    kind: "http",
    label: "Direct download",
    description: "The file name and size will be detected when the transfer starts.",
    multiple: false,
    confidence: /^https?:\/\//i.test(source) ? "medium" : "low",
  };
}
