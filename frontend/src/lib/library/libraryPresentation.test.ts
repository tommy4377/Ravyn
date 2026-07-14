import { describe, expect, it } from "vitest";
import type { LibraryEntry } from "../api/types";
import { groupLibraryDuplicates, sortLibraryEntries } from "./libraryPresentation";

function entry(overrides: Partial<LibraryEntry>): LibraryEntry {
  return {
    id: crypto.randomUUID(),
    job_id: null,
    source_url: "https://example.test/file",
    mirrors: [],
    sha256: null,
    size_bytes: 10,
    path: "C:/Downloads/file.bin",
    filename: "file.bin",
    category: "downloads",
    mime_type: "application/octet-stream",
    media_metadata: null,
    torrent_metadata: null,
    tags: [],
    trust: null,
    state: "active",
    trash_path: null,
    imported: false,
    downloaded_at: "2026-07-14T10:00:00Z",
    created_at: "2026-07-14T10:00:00Z",
    updated_at: "2026-07-14T10:00:00Z",
    ...overrides,
  };
}

describe("groupLibraryDuplicates", () => {
  it("groups matching checksums", () => {
    const groups = groupLibraryDuplicates([
      entry({ id: "a", path: "C:/A/file.bin", sha256: "ABC" }),
      entry({ id: "b", path: "D:/B/file.bin", sha256: "abc" }),
      entry({ id: "c", filename: "other.bin", sha256: "different" }),
    ]);

    expect(groups).toHaveLength(1);
    expect(groups[0]?.reason).toBe("checksum");
    expect(groups[0]?.entries.map((item) => item.id)).toEqual(["a", "b"]);
  });

  it("uses name and size only when a checksum is unavailable", () => {
    const groups = groupLibraryDuplicates([
      entry({ id: "a", filename: "same.zip", size_bytes: 42, sha256: null }),
      entry({ id: "b", filename: "same.zip", size_bytes: 42, sha256: null }),
    ]);

    expect(groups[0]?.reason).toBe("name and size");
  });
});

describe("sortLibraryEntries", () => {
  it("sorts by size descending", () => {
    const sorted = sortLibraryEntries([
      entry({ id: "small", size_bytes: 1 }),
      entry({ id: "large", size_bytes: 99 }),
    ], "size", "desc");

    expect(sorted.map((item) => item.id)).toEqual(["large", "small"]);
  });
});
