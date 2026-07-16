import { describe, expect, it } from "vitest";
import { ResourceCache } from "./cache";

function resource(id: string, confidence: number, discoveredAt = Date.now()) {
  return {
    id,
    url: `https://example.com/${id}.mp4`,
    normalizedUrl: `https://example.com/${id}.mp4`,
    pageUrl: "https://example.com/",
    type: "video" as const,
    source: "dom" as const,
    confidence,
    discoveredAt,
  };
}

describe("ResourceCache", () => {
  it("merges, bounds and clears per-tab resources", () => {
    const cache = new ResourceCache();
    cache.merge(1, [resource("a", 0.5), resource("b", 0.6)], 1);
    expect(cache.list(1)).toHaveLength(1);
    cache.clear(1);
    expect(cache.list(1)).toEqual([]);
  });

  it("can clear every memory-only tab cache", () => {
    const cache = new ResourceCache();
    cache.merge(1, [resource("a", 0.5)]);
    cache.merge(2, [resource("b", 0.5)]);
    cache.clearAll();
    expect(cache.list(1)).toEqual([]);
    expect(cache.list(2)).toEqual([]);
  });
});
