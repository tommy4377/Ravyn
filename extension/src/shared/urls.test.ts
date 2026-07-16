import { describe, expect, it } from "vitest";
import {
  classifyResource,
  domainMatches,
  extensionFromUrl,
  normalizeUrl,
  originPattern,
} from "./urls";

describe("normalizeUrl", () => {
  it("resolves relative URLs, preserves queries and removes fragments", () => {
    expect(
      normalizeUrl(
        "../file.zip?token=abc#section",
        "https://example.com/path/page",
      ),
    ).toBe("https://example.com/file.zip?token=abc");
  });

  it("rejects non-network schemes", () => {
    expect(normalizeUrl("blob:https://example.com/id")).toBeNull();
    expect(normalizeUrl("data:text/plain,hello")).toBeNull();
    expect(normalizeUrl("file:///tmp/file.zip")).toBeNull();
  });

  it("rejects credentials embedded in URLs", () => {
    expect(normalizeUrl("https://user:secret@example.com/file.zip")).toBeNull();
  });
});

describe("resource helpers", () => {
  it("classifies common resources", () => {
    expect(classifyResource("https://cdn.example/video.m3u8")).toBe("manifest");
    expect(classifyResource("https://cdn.example/image", "image/avif")).toBe(
      "image",
    );
    expect(classifyResource("https://cdn.example/archive.7z")).toBe("archive");
  });

  it("extracts safe extensions and origin patterns", () => {
    expect(extensionFromUrl("https://example.com/a.FILE.ZIP?x=1")).toBe("zip");
    expect(originPattern("https://example.com:8443/file")).toBe(
      "https://example.com:8443/*",
    );
  });

  it("matches exact and wildcard domains without matching sibling suffixes", () => {
    expect(domainMatches("example.com", "example.com")).toBe(true);
    expect(domainMatches("*.example.com", "media.example.com")).toBe(true);
    expect(domainMatches("*.example.com", "badexample.com")).toBe(false);
  });
});
