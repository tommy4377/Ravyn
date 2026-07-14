import { describe, expect, it } from "vitest";
import { createAccentPalette, normalizeHexColor } from "./colors";

describe("Windows accent palette", () => {
  it("normalizes short and long CSS colors", () => {
    expect(normalizeHexColor("#ABC")).toBe("#aabbcc");
    expect(normalizeHexColor("#12abEF")).toBe("#12abef");
    expect(normalizeHexColor("blue")).toBeNull();
  });

  it("keeps the light-theme accent darker than the dark-theme accent", () => {
    const light = createAccentPalette("#f5d742", "light");
    const dark = createAccentPalette("#f5d742", "dark");
    expect(light.default).not.toBe(dark.default);
    expect(light.onColor).toMatch(/^#/);
    expect(dark.onColor).toMatch(/^#/);
  });

  it("falls back safely for malformed registry values", () => {
    const palette = createAccentPalette("not-a-color", "dark");
    expect(palette.default).toMatch(/^#[0-9a-f]{6}$/);
    expect(palette.subtle).toContain("rgba(");
  });
});
