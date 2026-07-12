CREATE TABLE IF NOT EXISTS host_profiles (
    host TEXT PRIMARY KEY NOT NULL,
    successful_downloads INTEGER NOT NULL DEFAULT 0,
    failed_downloads INTEGER NOT NULL DEFAULT 0,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    average_throughput_bps INTEGER,
    range_failures INTEGER NOT NULL DEFAULT 0,
    circuit_open_until TEXT,
    last_error TEXT,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS host_profiles_circuit_idx
    ON host_profiles(circuit_open_until, updated_at);

ALTER TABLE jobs ADD COLUMN available_at TEXT;
CREATE INDEX IF NOT EXISTS jobs_available_idx
    ON jobs(status, available_at, priority DESC, created_at ASC);
