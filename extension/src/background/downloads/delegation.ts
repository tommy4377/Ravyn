import { urlHashInput } from "../../shared/urls";

interface DelegatedDownload {
  normalizedUrlHash: string;
  createdAt: number;
  ravynJobId: string;
}

const MAX_AGE_MS = 2 * 60 * 1_000;

export class DelegationRegistry {
  private entries = new Map<string, DelegatedDownload>();

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

  private prune(): void {
    const cutoff = Date.now() - MAX_AGE_MS;
    for (const [key, entry] of this.entries)
      if (entry.createdAt < cutoff) this.entries.delete(key);
  }
}

async function hashUrl(url: string): Promise<string> {
  const bytes = new TextEncoder().encode(urlHashInput(url));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)]
    .map((value) => value.toString(16).padStart(2, "0"))
    .join("");
}
