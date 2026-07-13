ALTER TABLE setup_state ADD COLUMN installation_mode TEXT;
ALTER TABLE setup_state ADD COLUMN installed_exe TEXT;
ALTER TABLE setup_state ADD COLUMN installed_version TEXT;
ALTER TABLE setup_state ADD COLUMN installed_sha256 TEXT;
ALTER TABLE setup_state ADD COLUMN integration_completed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE setup_state ADD COLUMN integration_errors TEXT;
ALTER TABLE setup_state ADD COLUMN relaunch_pending INTEGER NOT NULL DEFAULT 0;
