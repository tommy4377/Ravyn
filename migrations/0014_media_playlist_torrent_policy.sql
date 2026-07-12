CREATE TABLE media_items (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    item_key TEXT NOT NULL,
    extractor TEXT,
    media_id TEXT,
    title TEXT,
    webpage_url TEXT,
    playlist_id TEXT,
    playlist_title TEXT,
    playlist_index INTEGER CHECK (playlist_index IS NULL OR playlist_index >= 0),
    playlist_count INTEGER CHECK (playlist_count IS NULL OR playlist_count >= 0),
    extension TEXT,
    state TEXT NOT NULL CHECK (state IN ('planned','downloading','completed','failed','skipped')),
    output_path TEXT,
    output_id TEXT REFERENCES job_outputs(id) ON DELETE SET NULL,
    retry_job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    error TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(job_id, item_key)
);

CREATE INDEX media_items_job_state_idx ON media_items(job_id, state, playlist_index, id);
CREATE INDEX media_items_identity_idx ON media_items(extractor, media_id);
CREATE INDEX media_items_output_idx ON media_items(output_id);
CREATE INDEX media_items_retry_idx ON media_items(retry_job_id);

CREATE TABLE media_archive (
    extractor TEXT NOT NULL,
    media_id TEXT NOT NULL,
    first_job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    last_job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    last_output_id TEXT REFERENCES job_outputs(id) ON DELETE SET NULL,
    webpage_url TEXT,
    downloaded_at TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY(extractor, media_id)
);

CREATE INDEX media_archive_downloaded_idx ON media_archive(downloaded_at DESC);
CREATE INDEX media_archive_job_idx ON media_archive(last_job_id);

CREATE TABLE torrent_seeding_state (
    job_id TEXT PRIMARY KEY NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    torrent_id TEXT NOT NULL,
    started_at TEXT NOT NULL,
    stopped_at TEXT,
    stop_reason TEXT,
    last_ratio REAL CHECK (last_ratio IS NULL OR last_ratio >= 0),
    updated_at TEXT NOT NULL
);

CREATE INDEX torrent_seeding_active_idx ON torrent_seeding_state(stopped_at, started_at);
