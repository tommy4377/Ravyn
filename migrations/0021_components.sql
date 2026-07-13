CREATE TABLE component_states (
    component TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL DEFAULT 'not_installed',
    managed_version TEXT,
    managed_path TEXT,
    custom_path TEXT,
    error_message TEXT,
    last_checked_at TEXT,
    install_started_at TEXT,
    install_completed_at TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE feature_selections (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    setup_profile TEXT NOT NULL DEFAULT 'minimal',
    features_json TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL
);
