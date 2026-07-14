import { describe, expect, it } from "vitest";
import { analyzeBatchInput } from "./batchImport";

describe("batch input analysis", () => {
  it("ignores comments, blanks, and duplicate source lines", () => {
    const result = analyzeBatchInput("# note\nhttps://a.test/file\n\n// skip\nhttps://a.test/file\nhttps://b.test/video");
    expect(result.uniqueLines).toEqual(["https://a.test/file", "https://b.test/video"]);
    expect(result.duplicateCount).toBe(1);
    expect(result.itemCount).toBe(2);
  });

  it("recognizes a JSON array of complete jobs", () => {
    const result = analyzeBatchInput(JSON.stringify([{ source: "https://a.test/file", kind: "http" }, { source: "magnet:?xt=urn:btih:abc", kind: "torrent" }]));
    expect(result.jsonBatch).toHaveLength(2);
    expect(result.itemCount).toBe(2);
  });

  it("falls back to text mode for invalid JSON job arrays", () => {
    const result = analyzeBatchInput('[{"kind":"http"}]');
    expect(result.jsonBatch).toBeNull();
    expect(result.itemCount).toBe(1);
  });
});
