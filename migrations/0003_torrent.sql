CREATE TABLE IF NOT EXISTS torrent_jobs (
    job_id TEXT PRIMARY KEY NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    torrent_id TEXT NOT NULL,
    info_hash TEXT,
    name TEXT,
    state TEXT NOT NULL DEFAULT 'initializing',
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    uploaded_bytes INTEGER NOT NULL DEFAULT 0,
    total_bytes INTEGER,
    download_speed_bps INTEGER NOT NULL DEFAULT 0,
    upload_speed_bps INTEGER NOT NULL DEFAULT 0,
    peers_connected INTEGER NOT NULL DEFAULT 0,
    seeders INTEGER NOT NULL DEFAULT 0,
    leechers INTEGER NOT NULL DEFAULT 0,
    raw_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS torrent_jobs_torrent_id_idx ON torrent_jobs(torrent_id);
CREATE INDEX IF NOT EXISTS torrent_jobs_state_idx ON torrent_jobs(state, updated_at);
