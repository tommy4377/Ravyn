CREATE TABLE download_presets (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE user_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    settings_patch_json TEXT NOT NULL,
    default_preset_id TEXT REFERENCES download_presets(id) ON DELETE SET NULL,
    active INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0,1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX user_profiles_active_idx ON user_profiles(active) WHERE active=1;

CREATE TABLE basket_items (
    id TEXT PRIMARY KEY NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    request_json TEXT NOT NULL,
    preset_id TEXT REFERENCES download_presets(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX basket_items_position_idx ON basket_items(position);
CREATE INDEX basket_items_created_idx ON basket_items(created_at, id);
