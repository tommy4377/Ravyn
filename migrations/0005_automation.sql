ALTER TABLE schedules ADD COLUMN cron_expression TEXT;
ALTER TABLE schedules ADD COLUMN mode TEXT NOT NULL DEFAULT 'download';
ALTER TABLE schedules ADD COLUMN automation_json TEXT NOT NULL DEFAULT 'null';
ALTER TABLE schedules ADD COLUMN last_run_at TEXT;
ALTER TABLE schedules ADD COLUMN failure_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE schedules ADD COLUMN last_error TEXT;

CREATE TABLE IF NOT EXISTS browser_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    allowed_origins_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT
);
CREATE INDEX IF NOT EXISTS browser_tokens_active_idx
    ON browser_tokens(revoked_at, created_at DESC);

CREATE TABLE IF NOT EXISTS page_resources (
    page_url TEXT NOT NULL,
    resource_url TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    last_imported_at TEXT,
    PRIMARY KEY(page_url, resource_url)
);
CREATE INDEX IF NOT EXISTS page_resources_seen_idx
    ON page_resources(page_url, last_seen_at DESC);
