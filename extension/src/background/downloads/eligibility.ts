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
  return {
    eligible: true,
    extension: extensionFromUrl(normalized),
    host: parsed.hostname,
  };
}
