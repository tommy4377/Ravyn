import type { TorrentFile, TorrentRecord } from "../api/types";

export type TorrentDetailTab = "overview" | "files" | "peers" | "trackers" | "advanced";

export interface TorrentFileTreeNode {
  name: string;
  path: string;
  directories: TorrentFileTreeNode[];
  files: TorrentFile[];
  descendantFileIndexes: number[];
  descendantSizeBytes: number;
}

export function torrentProgress(torrent: TorrentRecord): number {
  if (!torrent.total_bytes || torrent.total_bytes <= 0) {
    return torrent.state.toLowerCase().includes("complete") || torrent.state.toLowerCase().includes("seed") ? 100 : 0;
  }
  return Math.max(0, Math.min(100, torrent.downloaded_bytes / torrent.total_bytes * 100));
}

export function torrentRatio(uploaded: number, downloaded: number): number | null {
  if (downloaded <= 0) return uploaded > 0 ? null : 0;
  return uploaded / downloaded;
}

export function torrentEtaSeconds(torrent: TorrentRecord): number | null {
  if (!torrent.total_bytes || torrent.download_speed_bps <= 0) return null;
  const remaining = Math.max(0, torrent.total_bytes - torrent.downloaded_bytes);
  return remaining / torrent.download_speed_bps;
}

export function formatTorrentEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return "—";
  if (seconds <= 0) return "Done";
  if (seconds < 60) return "< 1 min";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  if (hours < 24) return remainingMinutes ? `${hours} h ${remainingMinutes} min` : `${hours} h`;
  const days = Math.floor(hours / 24);
  return `${days} d ${hours % 24} h`;
}

export function buildTorrentFileTree(files: TorrentFile[], query = ""): TorrentFileTreeNode {
  const root: TorrentFileTreeNode = {
    name: "Files",
    path: "",
    directories: [],
    files: [],
    descendantFileIndexes: [],
    descendantSizeBytes: 0,
  };
  const normalizedQuery = query.trim().toLowerCase();

  for (const file of files) {
    if (normalizedQuery && !file.path.toLowerCase().includes(normalizedQuery)) continue;
    const parts = file.path.split(/[\\/]/).filter(Boolean);
    const filename = parts.pop() ?? file.path;
    let node = root;
    let currentPath = "";
    for (const part of parts) {
      currentPath = currentPath ? `${currentPath}/${part}` : part;
      let directory = node.directories.find((candidate) => candidate.name === part);
      if (!directory) {
        directory = {
          name: part,
          path: currentPath,
          directories: [],
          files: [],
          descendantFileIndexes: [],
          descendantSizeBytes: 0,
        };
        node.directories.push(directory);
      }
      node = directory;
    }
    node.files.push({ ...file, path: filename });
  }

  function finalize(node: TorrentFileTreeNode): void {
    node.directories.sort((a, b) => a.name.localeCompare(b.name));
    node.files.sort((a, b) => a.path.localeCompare(b.path));
    for (const directory of node.directories) finalize(directory);
    node.descendantFileIndexes = [
      ...node.files.map((file) => file.index),
      ...node.directories.flatMap((directory) => directory.descendantFileIndexes),
    ];
    node.descendantSizeBytes = node.files.reduce((sum, file) => sum + (file.size_bytes ?? 0), 0)
      + node.directories.reduce((sum, directory) => sum + directory.descendantSizeBytes, 0);
  }

  finalize(root);
  return root;
}

export function extractTrackers(raw: unknown): string[] {
  const results = new Set<string>();
  const visited = new Set<object>();

  function visit(value: unknown, keyHint = ""): void {
    if (typeof value === "string") {
      const normalized = value.trim();
      if ((keyHint.includes("tracker") || keyHint.includes("announce")) && /^(https?|udp|wss?):\/\//i.test(normalized)) results.add(normalized);
      return;
    }
    if (!value || typeof value !== "object" || visited.has(value)) return;
    visited.add(value);
    if (Array.isArray(value)) {
      for (const item of value) visit(item, keyHint);
      return;
    }
    for (const [key, nested] of Object.entries(value as Record<string, unknown>)) visit(nested, key.toLowerCase());
  }

  visit(raw);
  return [...results].sort();
}
