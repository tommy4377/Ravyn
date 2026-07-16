import { describe, expect, it } from "vitest";
import type { BrowserRule } from "../../shared/contracts";
import { evaluateRules } from "./evaluator";

const rule = (overrides: Partial<BrowserRule>): BrowserRule => ({
  id: crypto.randomUUID(),
  name: "Rule",
  priority: 0,
  enabled: true,
  domains: [],
  extensions: [],
  mimePatterns: [],
  action: "ravyn",
  ...overrides,
});

describe("evaluateRules", () => {
  it("selects the highest-priority matching rule", () => {
    const match = evaluateRules(
      [
        rule({ name: "low", priority: 10, domains: ["*.example.com"] }),
        rule({ name: "high", priority: 100, extensions: ["zip"] }),
      ],
      { url: "https://cdn.example.com/file.zip", extension: "zip" },
    );
    expect(match?.name).toBe("high");
  });

  it("supports URL regex matching", () => {
    const match = evaluateRules(
      [rule({ name: "release", urlRegex: "^https://example\\.com/releases/" })],
      { url: "https://example.com/releases/file.zip" },
    );
    expect(match?.name).toBe("release");
    expect(
      evaluateRules([rule({ name: "invalid", urlRegex: "[" })], {
        url: "https://example.com/releases/file.zip",
      }),
    ).toBeNull();
  });

  it("supports MIME wildcards and ignores disabled rules", () => {
    const match = evaluateRules(
      [
        rule({ name: "disabled", priority: 200, enabled: false }),
        rule({ name: "video", mimePatterns: ["video/*"] }),
      ],
      { url: "https://example.com/stream", mime: "video/mp4" },
    );
    expect(match?.name).toBe("video");
  });
});
