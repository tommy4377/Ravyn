CREATE TABLE library_move_transactions (
    id TEXT PRIMARY KEY NOT NULL,
    source_root TEXT NOT NULL,
    destination_root TEXT NOT NULL,
    conflict_policy TEXT NOT NULL CHECK (conflict_policy IN ('fail', 'reuse_identical')),
    state TEXT NOT NULL CHECK (state IN (
        'running', 'cancelling', 'cancelled', 'failed',
        'restart_required', 'completed', 'rolled_back'
    )),
    total_files INTEGER NOT NULL DEFAULT 0 CHECK (total_files >= 0),
    total_bytes INTEGER NOT NULL DEFAULT 0 CHECK (total_bytes >= 0),
    copied_files INTEGER NOT NULL DEFAULT 0 CHECK (copied_files >= 0),
    copied_bytes INTEGER NOT NULL DEFAULT 0 CHECK (copied_bytes >= 0),
    verified_files INTEGER NOT NULL DEFAULT 0 CHECK (verified_files >= 0),
    reused_files INTEGER NOT NULL DEFAULT 0 CHECK (reused_files >= 0),
    missing_files INTEGER NOT NULL DEFAULT 0 CHECK (missing_files >= 0),
    external_entries INTEGER NOT NULL DEFAULT 0 CHECK (external_entries >= 0),
    conflict_files INTEGER NOT NULL DEFAULT 0 CHECK (conflict_files >= 0),
    cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0,1)),
    restart_required INTEGER NOT NULL DEFAULT 0 CHECK (restart_required IN (0,1)),
    error TEXT,
    started_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT
);

CREATE UNIQUE INDEX library_move_single_active_idx
    ON library_move_transactions((1))
    WHERE state IN ('running', 'cancelling', 'restart_required');

CREATE INDEX library_move_updated_idx
    ON library_move_transactions(updated_at DESC, id DESC);

CREATE TABLE library_move_items (
    transaction_id TEXT NOT NULL REFERENCES library_move_transactions(id) ON DELETE CASCADE,
    entry_id TEXT NOT NULL REFERENCES library_entries(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    destination_path TEXT NOT NULL,
    source_entry_path TEXT NOT NULL,
    destination_entry_path TEXT NOT NULL,
    was_trashed INTEGER NOT NULL DEFAULT 0 CHECK (was_trashed IN (0,1)),
    expected_sha256 TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
    state TEXT NOT NULL CHECK (state IN (
        'pending', 'copying', 'committing', 'verified', 'reused', 'missing',
        'source_removed', 'failed'
    )),
    created_destination INTEGER NOT NULL DEFAULT 0 CHECK (created_destination IN (0,1)),
    error TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(transaction_id, entry_id)
);

CREATE INDEX library_move_items_state_idx
    ON library_move_items(transaction_id, state, entry_id);
