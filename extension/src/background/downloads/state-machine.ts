import type { ExtensionSettings } from "../../shared/contracts";

export type BrowserRuleAction =
  "ravyn" | "browser" | "ask" | "ignore" | undefined;
export type InterceptionDecision = "ignore" | "intercept" | "confirm";

export function decideInterception(
  settings: ExtensionSettings,
  ruleAction: BrowserRuleAction,
  forcedByDomain: boolean,
): InterceptionDecision {
  if (
    !settings.automaticInterception ||
    settings.interceptionMode === "disabled"
  )
    return "ignore";
  if (ruleAction === "browser" || ruleAction === "ignore") return "ignore";
  // An explicit "always intercept" domain is a stronger, more specific
  // signal than the blanket "ask every time" mode — it should win rather
  // than still prompting.
  if (forcedByDomain) return "intercept";
  if (settings.interceptionMode === "ask" || ruleAction === "ask")
    return "confirm";
  if (settings.interceptionMode === "all-compatible") return "intercept";
  if (settings.interceptionMode === "rules-only" && ruleAction === "ravyn")
    return "intercept";
  return "ignore";
}
