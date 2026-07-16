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
}

export class ResourceCache {
  private tabs = new Map<number, TabCache>();

  merge(
    tabId: number,
    resources: DetectedResource[],
    maximum = MAX_RESOURCES_PER_TAB,
  ): DetectedResource[] {
    this.prune();
    const tab: TabCache = this.tabs.get(tabId) ?? {
      resources: new Map<string, DetectedResource>(),
      createdAt: Date.now(),
      monitored: false,
    };
    for (const resource of sanitizeDetectedResources(resources, maximum)) {
      const current = tab.resources.get(resource.normalizedUrl);
      if (!current || resource.confidence >= current.confidence)
        tab.resources.set(resource.normalizedUrl, resource);
      if (tab.resources.size > maximum) {
        const oldest = [...tab.resources.values()].sort(
          (left, right) => left.discoveredAt - right.discoveredAt,
        )[0];
        if (oldest) tab.resources.delete(oldest.normalizedUrl);
      }
    }
    this.tabs.set(tabId, tab);
    return this.list(tabId);
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
    const tab: TabCache = this.tabs.get(tabId) ?? {
      resources: new Map<string, DetectedResource>(),
      createdAt: Date.now(),
      monitored: false,
    };
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
        !cache.monitored
      )
        this.tabs.delete(tabId);
    }
  }
}
