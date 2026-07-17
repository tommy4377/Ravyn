import { hashUrl } from "../../shared/hashUrl";

const MAX_AGE_MS = 5_000;

/**
 * Short-lived per-URL "let the browser handle this one" marker, armed by the
 * content script when the user clicks a download link while holding the
 * configured bypass modifier (Alt, by default — mirrors IDM's escape hatch).
 * Keyed by URL hash rather than tab/frame id because `browser.downloads`
 * items don't carry the originating tab.
 */
export class BypassRegistry {
  private entries = new Map<string, number>();

  async arm(url: string): Promise<void> {
    this.prune();
    this.entries.set(await hashUrl(url), Date.now());
  }

  async consume(url: string): Promise<boolean> {
    this.prune();
    const key = await hashUrl(url);
    const armed = this.entries.has(key);
    if (armed) this.entries.delete(key);
    return armed;
  }

  private prune(): void {
    const cutoff = Date.now() - MAX_AGE_MS;
    for (const [key, armedAt] of this.entries)
      if (armedAt < cutoff) this.entries.delete(key);
  }
}
