import { describe, expect, it } from "vitest";
import { classifyObservedRequest } from "./classifier";

describe("classifyObservedRequest", () => {
  it("keeps manifests and ignores media fragments", () => {
    expect(classifyObservedRequest("https://cdn.example/master.m3u8")).toEqual({
      kind: "manifest",
      ignore: false,
    });
    expect(classifyObservedRequest("https://cdn.example/chunk.m4s")).toEqual({
      kind: "other",
      ignore: true,
    });
    expect(classifyObservedRequest("https://cdn.example/chunk.ts")).toEqual({
      kind: "other",
      ignore: true,
    });
  });

  it("uses response MIME and request type", () => {
    expect(
      classifyObservedRequest("https://cdn.example/id", "audio/ogg"),
    ).toEqual({ kind: "audio", ignore: false });
    expect(
      classifyObservedRequest("https://cdn.example/id", undefined, "media"),
    ).toEqual({ kind: "video", ignore: false });
  });
});
