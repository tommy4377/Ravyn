import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../../shared/settings";
import { decideInterception } from "./state-machine";

describe("decideInterception", () => {
  it("does nothing when automatic interception is disabled", () => {
    expect(decideInterception(DEFAULT_SETTINGS, "ravyn", true)).toBe("ignore");
  });

  it("honors browser and ignore rules before forced domains", () => {
    const settings = {
      ...DEFAULT_SETTINGS,
      automaticInterception: true,
      interceptionMode: "all-compatible" as const,
    };
    expect(decideInterception(settings, "browser", true)).toBe("ignore");
    expect(decideInterception(settings, "ignore", true)).toBe("ignore");
  });

  it("supports rules-only, confirmation and all-compatible modes", () => {
    expect(
      decideInterception(
        { ...DEFAULT_SETTINGS, automaticInterception: true },
        "ravyn",
        false,
      ),
    ).toBe("intercept");
    expect(
      decideInterception(
        {
          ...DEFAULT_SETTINGS,
          automaticInterception: true,
          interceptionMode: "ask",
        },
        undefined,
        false,
      ),
    ).toBe("confirm");
    expect(
      decideInterception(
        {
          ...DEFAULT_SETTINGS,
          automaticInterception: true,
          interceptionMode: "all-compatible",
        },
        undefined,
        false,
      ),
    ).toBe("intercept");
  });
});
