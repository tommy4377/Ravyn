import { describe, expect, it } from "vitest";
import type { TorrentRecord } from "../api/types";
import { buildTorrentFileTree, extractTrackers, formatTorrentEta, torrentEtaSeconds, torrentProgress } from "./torrentPresentation";

function torrent(overrides: Partial<TorrentRecord>): TorrentRecord {
  return {
    job_id: "job",
    torrent_id: "torrent",
    info_hash: "hash",
    name: "Example",
    state: "downloading",
    downloaded_bytes: 50,
    uploaded_bytes: 0,
    total_bytes: 100,
    download_speed_bps: 10,
    upload_speed_bps: 0,
    peers_connected: 1,
    seeders: 1,
    leechers: 1,
    raw: null,
    updated_at: "2026-07-14T10:00:00Z",
    ...overrides,
  };
}

describe("torrent progress and ETA", () => {
  it("calculates progress and remaining time", () => {
    const value = torrent({});
    expect(torrentProgress(value)).toBe(50);
    expect(torrentEtaSeconds(value)).toBe(5);
    expect(formatTorrentEta(3600)).toBe("1 h");
  });
});

describe("buildTorrentFileTree", () => {
  it("creates nested folders and aggregate file indexes", () => {
    const tree = buildTorrentFileTree([
      { index: 1, path: "Season 1/Episode 1.mkv", size_bytes: 10 },
      { index: 2, path: "Season 1/Subs/Episode 1.srt", size_bytes: 2 },
    ]);
    expect(tree.directories[0]?.name).toBe("Season 1");
    expect(tree.directories[0]?.descendantFileIndexes).toEqual([1, 2]);
    expect(tree.descendantSizeBytes).toBe(12);
  });
});

describe("extractTrackers", () => {
  it("finds tracker URLs in nested raw data", () => {
    expect(extractTrackers({ trackers: [{ announce: "udp://tracker.example:80" }] })).toEqual(["udp://tracker.example:80"]);
  });
});
