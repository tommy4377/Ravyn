import { hashUrl } from "../../shared/hashUrl";

interface DelegatedDownload {
  normalizedUrlHash: string;
  createdAt: number;
  ravynJobId: string;
}

const MAX_AGE_MS = 2 * 60 * 1_000;

export class DelegationRegistry {
  private entries = new Map<string, DelegatedDownload>();
  // URLs currently mid-handoff (native request in flight, result not yet
  // remembered). Two `downloads.onCreated` events for the same URL — a
  // double-click, or a page firing near-simultaneous requests for the same
  // resource — otherwise both pass `contains()` (neither has an entry yet)
  // and both call `create_download`, producing two Ravyn jobs for one click.
  private inFlight = new Set<string>();

  async remember(url: string, ravynJobId: string): Promise<void> {
    this.prune();
    const normalizedUrlHash = await hashUrl(url);
    this.entries.set(normalizedUrlHash, {
      normalizedUrlHash,
      createdAt: Date.now(),
      ravynJobId,
    });
  }

  async contains(url: string): Promise<boolean> {
    this.prune();
    return this.entries.has(await hashUrl(url));
  }

  /** Claims the URL for a handoff in progress; false if another one already has it. */
  async claim(url: string): Promise<boolean> {
    const key = await hashUrl(url);
    if (this.inFlight.has(key) || this.entries.has(key)) return false;
    this.inFlight.add(key);
    return true;
  }

  /** Releases a claim taken via {@link claim}; safe to call unconditionally once handling finishes. */
  async release(url: string): Promise<void> {
    this.inFlight.delete(await hashUrl(url));
  }

  private prune(): void {
    const cutoff = Date.now() - MAX_AGE_MS;
    for (const [key, entry] of this.entries)
      if (entry.createdAt < cutoff) this.entries.delete(key);
  }
}
