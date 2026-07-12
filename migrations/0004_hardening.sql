ALTER TABLE jobs ADD COLUMN transfer_mode TEXT NOT NULL DEFAULT 'none';
ALTER TABLE schedules ADD COLUMN claim_token TEXT;
ALTER TABLE schedules ADD COLUMN claim_until TEXT;
CREATE INDEX IF NOT EXISTS schedules_claim_idx ON schedules(enabled, next_run_at, claim_until);
