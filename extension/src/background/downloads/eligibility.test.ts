import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../../shared/settings";
import { evaluateEligibility, type DownloadCandidate } from "./eligibility";

const candidate = (
  overrides: Partial<DownloadCandidate> = {},
): DownloadCandidate => ({
  id: 1,
  url: "https://example.com/file.zip",
  incognito: false,
  method: "GET",
  ...overrides,
});

describe("evaluateEligibility", () => {
  const enabled = {
    ...DEFAULT_SETTINGS,
    automaticInterception: true,
    interceptionMode: "all-compatible" as const,
  };

  it("accepts supported GET downloads", () => {
    expect(
      evaluateEligibility(candidate(), enabled, "extension-id"),
    ).toMatchObject({ eligible: true, extension: "zip", host: "example.com" });
  });

  it("rejects unsafe or unsupported cases", () => {
    expect(
      evaluateEligibility(
        candidate({ method: "POST" }),
        enabled,
        "extension-id",
      ).reason,
    ).toBe("non-get");
    expect(
      evaluateEligibility(
        candidate({ url: "blob:https://example.com/id" }),
        enabled,
        "extension-id",
      ).reason,
    ).toBe("unsupported-scheme");
    expect(
      evaluateEligibility(
        candidate({ byExtensionId: "extension-id" }),
        enabled,
        "extension-id",
      ).reason,
    ).toBe("extension-created");
    expect(
      evaluateEligibility(
        candidate({ incognito: true }),
        enabled,
        "extension-id",
      ).reason,
    ).toBe("private-window");
  });
});
