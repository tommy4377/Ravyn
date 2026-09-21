CREATE TABLE setup_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    completed INTEGER NOT NULL DEFAULT 0,
    completed_at TEXT,
    app_version TEXT,
    library_root TEXT,
    updated_at TEXT NOT NULL
);
