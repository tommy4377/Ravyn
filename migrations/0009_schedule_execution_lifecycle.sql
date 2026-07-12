DROP INDEX IF EXISTS idx_schedule_executions_state;
ALTER TABLE schedule_executions RENAME TO schedule_executions_v1;

CREATE TABLE schedule_executions (
    id TEXT PRIMARY KEY,
    schedule_id TEXT NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    intended_run_at TEXT NOT NULL,
    claim_token TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed', 'lease_lost', 'cancelled')),
    summary_json TEXT,
    error TEXT,
    cancellation_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancellation_requested IN (0, 1)),
    started_at TEXT NOT NULL,
    completed_at TEXT,
    UNIQUE(schedule_id, intended_run_at)
);

INSERT INTO schedule_executions(
    id,schedule_id,intended_run_at,claim_token,state,summary_json,error,
    cancellation_requested,started_at,completed_at
)
SELECT id,schedule_id,intended_run_at,claim_token,state,summary_json,error,0,started_at,completed_at
FROM schedule_executions_v1;

DROP TABLE schedule_executions_v1;
CREATE INDEX idx_schedule_executions_state ON schedule_executions(state, started_at);
