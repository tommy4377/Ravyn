ALTER TABLE audit_log ADD COLUMN previous_hash TEXT;
ALTER TABLE audit_log ADD COLUMN entry_hash TEXT;

CREATE UNIQUE INDEX audit_log_entry_hash_idx
    ON audit_log(entry_hash)
    WHERE entry_hash IS NOT NULL;

CREATE TABLE audit_chain_head (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    hash TEXT,
    anchor_hash TEXT
);
INSERT INTO audit_chain_head(id, hash, anchor_hash) VALUES(1, NULL, NULL);
