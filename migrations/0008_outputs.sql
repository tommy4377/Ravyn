CREATE TABLE job_outputs (
    id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    output_type TEXT NOT NULL CHECK (output_type IN ('primary','video','audio','subtitle','thumbnail','metadata','torrent_file','extracted_file','converted_file','archive','directory','temporary','other')),
    original_path TEXT NOT NULL,
    current_path TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    mime_type TEXT,
    checksum_algorithm TEXT,
    checksum_value TEXT,
    state TEXT NOT NULL CHECK (state IN ('planned','creating','ready','failed','deleted','moved','replaced')),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('http','media','torrent','post_process')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(job_id, original_path)
);

CREATE INDEX job_outputs_job_idx ON job_outputs(job_id, created_at, id);
CREATE INDEX job_outputs_state_idx ON job_outputs(state, updated_at);
CREATE INDEX job_outputs_current_path_idx ON job_outputs(current_path);
