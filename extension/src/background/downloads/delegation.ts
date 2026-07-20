/** Tracks Firefox download ids that are currently being handed off to Ravyn. */
export class DelegationRegistry {
  private inFlight = new Set<number>();

  /** Claims one concrete Firefox download event; false if it is already in flight. */
  claim(browserDownloadId: number): boolean {
    if (this.inFlight.has(browserDownloadId)) return false;
    this.inFlight.add(browserDownloadId);
    return true;
  }

  /** Releases a prior claim; safe to call after every handoff attempt. */
  release(browserDownloadId: number): void {
    this.inFlight.delete(browserDownloadId);
  }
}
