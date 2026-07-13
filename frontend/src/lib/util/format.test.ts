import { describe, expect, it } from "vitest";
import { formatBytes, formatEta, formatPercent, formatSpeed, jobDisplayName } from "./format";

describe("formatBytes", () => {
  it("renders sub-kilobyte sizes as whole bytes", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("renders larger sizes with a unit and limited precision", () => {
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5 MB");
  });

  it("renders a placeholder for missing values", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(undefined)).toBe("—");
  });
});

describe("formatSpeed", () => {
  it("appends a per-second suffix", () => {
    expect(formatSpeed(2048)).toBe("2 KB/s");
  });

  it("renders a placeholder when there is no throughput", () => {
    expect(formatSpeed(0)).toBe("—");
    expect(formatSpeed(null)).toBe("—");
  });
});

describe("formatPercent", () => {
  it("computes a rounded percentage", () => {
    expect(formatPercent(50, 200)).toBe("25%");
  });

  it("clamps above 100%", () => {
    expect(formatPercent(300, 200)).toBe("100%");
  });

  it("renders a placeholder without a known total", () => {
    expect(formatPercent(50, null)).toBe("—");
  });
});

describe("formatEta", () => {
  it("returns a placeholder without a known total or speed", () => {
    expect(formatEta(50, null, 100)).toBe("—");
    expect(formatEta(50, 200, null)).toBe("—");
    expect(formatEta(50, 200, 0)).toBe("—");
  });

  it("returns a placeholder once the transfer is complete", () => {
    expect(formatEta(200, 200, 100)).toBe("—");
  });

  it("estimates remaining time from current throughput", () => {
    // 100 bytes left at 10 bytes/sec = 10 seconds.
    expect(formatEta(100, 200, 10)).toBe("10s");
  });
});

describe("jobDisplayName", () => {
  it("prefers an explicit filename", () => {
    expect(jobDisplayName("https://example.com/a/b.zip", "custom.zip")).toBe("custom.zip");
  });

  it("falls back to the URL's last path segment", () => {
    expect(jobDisplayName("https://example.com/a/b/file.zip", null)).toBe("file.zip");
  });

  it("falls back to the hostname when there is no path", () => {
    expect(jobDisplayName("https://example.com", null)).toBe("example.com");
  });

  it("falls back to the raw source when it is not a valid URL", () => {
    expect(jobDisplayName("not-a-url", null)).toBe("not-a-url");
  });
});
