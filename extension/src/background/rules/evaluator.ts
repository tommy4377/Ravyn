import type { BrowserRule } from "../../shared/contracts";
import { domainMatches } from "../../shared/urls";

export interface RuleInput {
  url: string;
  mime?: string;
  extension?: string;
}

export function evaluateRules(
  rules: BrowserRule[],
  input: RuleInput,
): BrowserRule | null {
  let host = "";
  try {
    host = new URL(input.url).hostname;
  } catch {
    return null;
  }
  return (
    [...rules]
      .filter((rule) => rule.enabled)
      .sort((left, right) => right.priority - left.priority)
      .find((rule) =>
        matches(rule, input.url, host, input.mime, input.extension),
      ) ?? null
  );
}

function matches(
  rule: BrowserRule,
  url: string,
  host: string,
  mime?: string,
  extension?: string,
): boolean {
  const domainMatchesRule =
    rule.domains.length === 0 ||
    rule.domains.some((domain) => domainMatches(domain, host));
  const extensionMatchesRule =
    rule.extensions.length === 0 ||
    (!!extension &&
      rule.extensions.some(
        (value) => value.toLowerCase() === extension.toLowerCase(),
      ));
  const mimeMatchesRule =
    rule.mimePatterns.length === 0 ||
    (!!mime && rule.mimePatterns.some((pattern) => mimeMatches(pattern, mime)));
  const urlMatchesRule = !rule.urlRegex || safeRegexMatches(rule.urlRegex, url);
  return (
    domainMatchesRule &&
    extensionMatchesRule &&
    mimeMatchesRule &&
    urlMatchesRule
  );
}

function mimeMatches(pattern: string, mime: string): boolean {
  const normalizedPattern = pattern.toLowerCase();
  const normalizedMime = mime.toLowerCase();
  return normalizedPattern.endsWith("/*")
    ? normalizedMime.startsWith(normalizedPattern.slice(0, -1))
    : normalizedPattern === normalizedMime;
}

function safeRegexMatches(pattern: string, value: string): boolean {
  try {
    return new RegExp(pattern).test(value);
  } catch {
    return false;
  }
}
