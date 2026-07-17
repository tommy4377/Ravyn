import {
  MAX_RESOURCES_PER_TAB,
  RESOURCE_MAX_AGE_MS,
  type DetectedResource,
} from "../../shared/contracts";
import { sanitizeDetectedResources } from "../../shared/validation";

interface TabCache {
  resources: Map<string, DetectedResource>;
  createdAt: number;
  monitored: boolean;
  /** A media segment (.ts/.m4s) was seen but no manifest has been found yet. */
  streamHint: boolean;
}

function emptyTab(): TabCache {
  return {
    resources: new Map<string, DetectedResource>(),
    createdAt: Date.now(),
    monitored: false,
    streamHint: false,
  };
}

export class ResourceCache {
  private tabs = new Map<number, TabCache>();

  merge(
    tabId: number,
    resources: DetectedResource[],
    maximum = MAX_RESOURCES_PER_TAB,
  ): DetectedResource[] {
    this.prune();
    const tab: TabCache = this.tabs.get(tabId) ?? emptyTab();
    for (const resource of sanitizeDetectedResources(resources, maximum)) {
      const current = tab.resources.get(resource.normalizedUrl);
      if (!current || resource.confidence >= current.confidence)
        tab.resources.set(resource.normalizedUrl, resource);
      // A real manifest URL supersedes the segment-only hint.
      if (resource.type === "manifest") tab.streamHint = false;
    }
    // Map iteration order is insertion order (re-setting an existing key
    // doesn't move it), so the first key is a good O(1) proxy for "oldest"
    // — evicting it in a loop avoids an O(n log n) sort per merge.
    while (tab.resources.size > maximum) {
      const oldestKey = tab.resources.keys().next().value;
      if (oldestKey === undefined) break;
      tab.resources.delete(oldestKey);
    }
    this.tabs.set(tabId, tab);
    return this.list(tabId);
  }

  /**
   * Marks that an adaptive-streaming segment was observed for this tab with
   * no corresponding manifest resource — the player likely fetched the
   * manifest through a path we can't see (e.g. a blob: URL), so there's a
   * stream here even though nothing showed up in the resource list.
   */
  markStreamHint(tabId: number): void {
    const tab = this.tabs.get(tabId) ?? emptyTab();
    const hasManifest = [...tab.resources.values()].some(
      (resource) => resource.type === "manifest",
    );
    if (hasManifest) return;
    tab.streamHint = true;
    this.tabs.set(tabId, tab);
  }

  hasStreamHint(tabId: number): boolean {
    return this.tabs.get(tabId)?.streamHint ?? false;
  }

  list(tabId: number): DetectedResource[] {
    this.prune();
    const resources = this.tabs.get(tabId)?.resources;
    return resources
      ? [...resources.values()].sort(
          (left, right) => right.discoveredAt - left.discoveredAt,
        )
      : [];
  }

  clear(tabId: number): void {
    this.tabs.delete(tabId);
  }

  clearAll(): void {
    this.tabs.clear();
  }

  setMonitored(tabId: number, monitored: boolean): void {
    const tab: TabCache = this.tabs.get(tabId) ?? emptyTab();
    tab.monitored = monitored;
    this.tabs.set(tabId, tab);
  }

  isMonitored(tabId: number): boolean {
    return this.tabs.get(tabId)?.monitored ?? false;
  }

  private prune(): void {
    const cutoff = Date.now() - RESOURCE_MAX_AGE_MS;
    for (const [tabId, cache] of this.tabs) {
      for (const [url, resource] of cache.resources)
        if (resource.discoveredAt < cutoff) cache.resources.delete(url);
      if (
        cache.resources.size === 0 &&
        cache.createdAt < cutoff &&
        !cache.monitored &&
        !cache.streamHint
      )
        this.tabs.delete(tabId);
    }
  }
}
