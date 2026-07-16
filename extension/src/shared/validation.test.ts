import { describe, expect, it } from "vitest";
import {
  validateBatchPayload,
  validateDownloadPayload,
  sanitizeDetectedResources,
} from "./validation";

const sourceContext = { browser: "firefox" as const, incognito: false };

describe("download validation", () => {
  it("normalizes and bounds user-controlled fields", () => {
    const result = validateDownloadPayload({
      url: "https://example.com/file.zip#fragment",
      filename: ` ${"a".repeat(300)} `,
      priority: 999,
      tags: ["one", "one", " two "],
      sourceContext,
    });
    expect(result.url).toBe("https://example.com/file.zip");
    expect(result.filename).toHaveLength(255);
    expect(result.priority).toBe(100);
    expect(result.tags).toEqual(["one", "two"]);
  });

  it("rejects empty and oversized batches", () => {
    expect(() => validateBatchPayload({ downloads: [] })).toThrow(
      /Select at least one/,
    );
    const item = { url: "https://example.com/file", sourceContext };
    expect(() =>
      validateBatchPayload({
        downloads: Array.from({ length: 1_001 }, () => item),
      }),
    ).toThrow(/at most 1000/);
  });
});

describe("resource validation", () => {
  it("deduplicates by normalized URL and keeps the strongest record", () => {
    const resources = sanitizeDetectedResources(
      [
        {
          id: "a",
          url: "https://example.com/a.mp4#one",
          normalizedUrl: "",
          pageUrl: "https://example.com",
          type: "video",
          source: "dom",
          confidence: 0.2,
          discoveredAt: 1,
        },
        {
          id: "b",
          url: "https://example.com/a.mp4#two",
          normalizedUrl: "",
          pageUrl: "https://example.com",
          type: "video",
          source: "performance",
          confidence: 0.9,
          discoveredAt: 2,
        },
      ],
      10,
    );
    expect(resources).toHaveLength(1);
    expect(resources[0]?.id).toBe("b");
  });
});
