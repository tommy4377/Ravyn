PRAGMA foreign_keys=ON;

ALTER TABLE schedules ADD COLUMN timezone_offset_minutes INTEGER NOT NULL DEFAULT 0
    CHECK(timezone_offset_minutes BETWEEN -840 AND 840);
ALTER TABLE schedules ADD COLUMN overlap_policy TEXT NOT NULL DEFAULT 'queue'
    CHECK(overlap_policy IN ('skip','queue','replace','allow_parallel'));
ALTER TABLE schedules ADD COLUMN missed_run_policy TEXT NOT NULL DEFAULT 'run_once'
    CHECK(missed_run_policy IN ('skip','run_once','catch_up'));
ALTER TABLE schedules ADD COLUMN max_catch_up_runs INTEGER NOT NULL DEFAULT 1
    CHECK(max_catch_up_runs BETWEEN 1 AND 100);
ALTER TABLE schedules ADD COLUMN catch_up_runs INTEGER NOT NULL DEFAULT 0
    CHECK(catch_up_runs BETWEEN 0 AND 100);
ALTER TABLE schedules ADD COLUMN paused_until TEXT;

CREATE INDEX IF NOT EXISTS idx_schedules_due_policy
    ON schedules(enabled, next_run_at, paused_until, overlap_policy);
