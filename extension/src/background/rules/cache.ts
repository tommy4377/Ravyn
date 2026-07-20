import type { BrowserRule, RuleSnapshot } from "../../shared/contracts";
import { logger } from "../../shared/logger";
import type { NativeClient } from "../native/client";

const STORAGE_KEY = "ravyn.ruleSnapshot";
const CACHE_TTL_MS = 10 * 60 * 1_000;
const MAX_RULES = 50_000;

export class RuleCache {
  private snapshot: RuleSnapshot | null = null;

  constructor(private readonly native: NativeClient) {}

  async get(force = false): Promise<BrowserRule[]> {
    if (!this.snapshot) this.snapshot = await this.loadStored();
    if (!force && this.snapshot && this.snapshot.expiresAt > Date.now())
      return this.snapshot.rules;
    try {
      const rules: BrowserRule[] = [];
      const seenCursors = new Set<string>();
      let cursor: string | undefined;
      do {
        if (cursor) {
          if (seenCursors.has(cursor))
            throw new Error("Ravyn returned a repeated rule cursor.");
          seenCursors.add(cursor);
        }
        const page = await this.native.request<{
          items: BrowserRule[];
          nextCursor?: string | null;
        }>("get_rules", cursor ? { cursor } : {});
        rules.push(...page.items);
        if (rules.length > MAX_RULES)
          throw new Error(
            `Ravyn returned more than ${MAX_RULES} browser rules.`,
          );
        cursor = page.nextCursor ?? undefined;
      } while (cursor);
      this.snapshot = {
        revision: await revisionFor(rules),
        updatedAt: Date.now(),
        expiresAt: Date.now() + CACHE_TTL_MS,
        rules,
      };
      await browser.storage.local.set({ [STORAGE_KEY]: this.snapshot });
      return rules;
    } catch (error) {
      logger.warn("Failed to refresh Ravyn rules", error);
      return this.snapshot?.rules ?? [];
    }
  }

  invalidate(): void {
    if (this.snapshot) this.snapshot.expiresAt = 0;
    void browser.storage.local.remove(STORAGE_KEY).catch(() => undefined);
  }

  private async loadStored(): Promise<RuleSnapshot | null> {
    const result = await browser.storage.local.get(STORAGE_KEY);
    const value = result[STORAGE_KEY] as RuleSnapshot | undefined;
    return value && Array.isArray(value.rules) ? value : null;
  }
}

async function revisionFor(rules: BrowserRule[]): Promise<string> {
  const bytes = new TextEncoder().encode(JSON.stringify(rules));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)]
    .slice(0, 12)
    .map((value) => value.toString(16).padStart(2, "0"))
    .join("");
}
