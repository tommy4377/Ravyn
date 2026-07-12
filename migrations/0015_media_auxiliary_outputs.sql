PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS media_item_outputs (
    media_item_id TEXT NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    output_id TEXT NOT NULL REFERENCES job_outputs(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK(role IN ('primary','video','audio','subtitle','thumbnail','metadata','description','chapter','auxiliary')),
    created_at TEXT NOT NULL,
    PRIMARY KEY(media_item_id, output_id)
);

CREATE INDEX IF NOT EXISTS idx_media_item_outputs_output
    ON media_item_outputs(output_id);
CREATE INDEX IF NOT EXISTS idx_media_item_outputs_role
    ON media_item_outputs(media_item_id, role);
