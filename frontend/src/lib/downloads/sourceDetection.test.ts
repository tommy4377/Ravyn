import { describe, expect, it } from "vitest";
import { detectSource } from "./sourceDetection";

describe("detectSource", () => {
  it("detects magnets and torrent files", () => {
    expect(detectSource("magnet:?xt=urn:btih:abc").kind).toBe("torrent");
    expect(detectSource("https://example.com/file.torrent").kind).toBe("torrent");
  });

  it("detects common media providers", () => {
    expect(detectSource("https://www.youtube.com/watch?v=abc").kind).toBe("media");
    expect(detectSource("https://soundcloud.com/artist/track").kind).toBe("media");
  });

  it("treats multiple lines as a simple direct-download batch", () => {
    const result = detectSource("https://example.com/a.zip\nhttps://example.com/b.zip");
    expect(result.kind).toBe("http");
    expect(result.multiple).toBe(true);
    expect(result.label).toBe("2 direct downloads");
  });

  it("detects Metalink XML and structured batches", () => {
    expect(detectSource('<metalink xmlns="urn:ietf:params:xml:ns:metalink"></metalink>').kind).toBe("metalink");
    expect(detectSource('[{"kind":"http","source":"https://example.com/a"}]').kind).toBe("batch");
  });

  it("falls back to direct downloads", () => {
    expect(detectSource("https://example.com/archive.zip").kind).toBe("http");
    expect(detectSource("C:\\Downloads\\archive.zip").kind).toBe("http");
  });
});
