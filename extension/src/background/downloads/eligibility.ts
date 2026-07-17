import type { ExtensionSettings } from "../../shared/contracts";
import {
  domainMatches,
  extensionFromUrl,
  normalizeUrl,
} from "../../shared/urls";

export interface DownloadCandidate {
  id: number;
  url: string;
  filename?: string;
  mime?: string;
  referrer?: string;
  incognito: boolean;
  byExtensionId?: string;
  method?: string;
  totalBytes?: number;
}

export interface EligibilityResult {
  eligible: boolean;
  reason?: string;
  extension?: string;
  host?: string;
}

export function evaluateEligibility(
  candidate: DownloadCandidate,
  settings: ExtensionSettings,
  extensionId: string,
): EligibilityResult {
  if (
    !settings.automaticInterception ||
    settings.interceptionMode === "disabled"
  )
    return { eligible: false, reason: "disabled" };
  if (candidate.byExtensionId === extensionId)
    return { eligible: false, reason: "extension-created" };
  if (candidate.method && candidate.method.toUpperCase() !== "GET")
    return { eligible: false, reason: "non-get" };
  if (candidate.incognito && !settings.includePrivateWindows)
    return { eligible: false, reason: "private-window" };
  const normalized = normalizeUrl(candidate.url);
  if (!normalized) return { eligible: false, reason: "unsupported-scheme" };
  const parsed = new URL(normalized);
  if (
    settings.disabledDomains.some((pattern) =>
      domainMatches(pattern, parsed.hostname),
    )
  )
    return { eligible: false, reason: "disabled-domain" };
  const extension = extensionFromUrl(normalized);
  if (
    settings.interceptExtensions.length > 0 &&
    !(extension && settings.interceptExtensions.includes(extension))
  )
    return { eligible: false, reason: "extension-not-allowed" };
  if (settings.minInterceptSizeBytes > 0) {
    const known =
      candidate.totalBytes !== undefined && candidate.totalBytes >= 0;
    if (!known || candidate.totalBytes! < settings.minInterceptSizeBytes)
      return { eligible: false, reason: "below-minimum-size" };
  }
  return {
    eligible: true,
    extension,
    host: parsed.hostname,
  };
}
