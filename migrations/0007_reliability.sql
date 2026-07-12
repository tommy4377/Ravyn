CREATE TABLE IF NOT EXISTS idempotency_keys (
    scope TEXT NOT NULL,
    key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (scope, key)
);

CREATE INDEX IF NOT EXISTS idx_idempotency_created_at
    ON idempotency_keys(created_at);

CREATE TABLE IF NOT EXISTS job_actions (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    action_index INTEGER NOT NULL,
    action_json TEXT NOT NULL,
    input_path TEXT NOT NULL,
    output_path TEXT,
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'completed', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(job_id, action_index)
);

CREATE INDEX IF NOT EXISTS idx_job_actions_job_state
    ON job_actions(job_id, state, action_index);

CREATE TABLE IF NOT EXISTS schedule_executions (
    id TEXT PRIMARY KEY,
    schedule_id TEXT NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    intended_run_at TEXT NOT NULL,
    claim_token TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed', 'lease_lost')),
    summary_json TEXT,
    error TEXT,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    UNIQUE(schedule_id, intended_run_at)
);

CREATE INDEX IF NOT EXISTS idx_schedule_executions_state
    ON schedule_executions(state, started_at);
