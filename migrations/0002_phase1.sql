CREATE TABLE IF NOT EXISTS job_segments (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    segment_index INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    completed INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(job_id, segment_index)
);
CREATE INDEX IF NOT EXISTS job_segments_job_idx ON job_segments(job_id, segment_index);

ALTER TABLE jobs ADD COLUMN etag TEXT;
ALTER TABLE jobs ADD COLUMN last_modified TEXT;
ALTER TABLE jobs ADD COLUMN final_url TEXT;
