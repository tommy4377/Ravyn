CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    destination TEXT NOT NULL,
    filename TEXT,
    status TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    total_bytes INTEGER,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    speed_limit_bps INTEGER,
    expected_sha256 TEXT,
    error TEXT,
    options_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT
);
CREATE INDEX IF NOT EXISTS jobs_status_priority_idx ON jobs(status, priority DESC, created_at ASC);
CREATE INDEX IF NOT EXISTS jobs_source_destination_idx ON jobs(source, destination);

CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);
CREATE TABLE IF NOT EXISTS job_tags (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY(job_id, tag_id)
);

CREATE TABLE IF NOT EXISTS rules (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    matcher_json TEXT NOT NULL,
    actions_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS rules_enabled_priority_idx ON rules(enabled, priority DESC);

CREATE TABLE IF NOT EXISTS schedules (
    id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    source TEXT NOT NULL,
    kind TEXT NOT NULL,
    destination TEXT NOT NULL,
    interval_seconds INTEGER,
    next_run_at TEXT NOT NULL,
    options_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS schedules_due_idx ON schedules(enabled, next_run_at);
