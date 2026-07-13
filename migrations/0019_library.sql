CREATE TABLE library_entries (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    source_url TEXT NOT NULL,
    mirrors_json TEXT NOT NULL DEFAULT '[]',
    sha256 TEXT,
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    path TEXT NOT NULL,
    filename TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN (
        'downloads','videos','music','documents','images','archives',
        'torrents','playlists','temporary','other'
    )),
    mime_type TEXT,
    media_metadata_json TEXT NOT NULL DEFAULT '{}',
    torrent_metadata_json TEXT NOT NULL DEFAULT '{}',
    tags_json TEXT NOT NULL DEFAULT '[]',
    trust_json TEXT,
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active','trashed','missing')),
    trash_path TEXT,
    imported INTEGER NOT NULL DEFAULT 0 CHECK (imported IN (0,1)),
    downloaded_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX library_entries_live_path_idx
    ON library_entries(path) WHERE state != 'trashed';
CREATE INDEX library_entries_job_idx ON library_entries(job_id);
CREATE INDEX library_entries_sha256_idx ON library_entries(sha256) WHERE sha256 IS NOT NULL;
CREATE INDEX library_entries_size_idx ON library_entries(size_bytes) WHERE size_bytes IS NOT NULL;
CREATE INDEX library_entries_category_idx ON library_entries(category, downloaded_at DESC, id);
CREATE INDEX library_entries_state_idx ON library_entries(state, updated_at DESC, id);
CREATE INDEX library_entries_filename_idx ON library_entries(filename COLLATE NOCASE);
CREATE INDEX library_entries_downloaded_idx ON library_entries(downloaded_at DESC, id);

CREATE TABLE stat_counters (
    key TEXT PRIMARY KEY NOT NULL,
    value INTEGER NOT NULL DEFAULT 0 CHECK (value >= 0)
);

CREATE TABLE library_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    cleanup_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
