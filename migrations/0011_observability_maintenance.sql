CREATE TABLE job_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    timestamp TEXT NOT NULL,
    source_module TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('debug','info','warn','error')),
    code TEXT NOT NULL,
    message TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX job_logs_job_time_idx ON job_logs(job_id, timestamp DESC, id DESC);

CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT,
    outcome TEXT NOT NULL CHECK (outcome IN ('success','failure')),
    metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX audit_log_time_idx ON audit_log(timestamp DESC, id DESC);
