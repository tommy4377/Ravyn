import { describe, expect, it } from "vitest";
import { parseSrcset } from "./srcset";

describe("parseSrcset", () => {
  it("parses width and density descriptors", () => {
    expect(parseSrcset("small.jpg 480w, large.jpg 2x")).toEqual([
      { url: "small.jpg", descriptor: "480w" },
      { url: "large.jpg", descriptor: "2x" },
    ]);
  });

  it("keeps commas inside URL functions", () => {
    expect(
      parseSrcset("https://example.com/image(1,2).jpg 1x, next.jpg 2x"),
    ).toHaveLength(2);
  });
});
