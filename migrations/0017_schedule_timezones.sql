-- Named zones are opt-in. Existing schedules retain their fixed offset and
-- therefore keep exactly the same next-run semantics after this migration.
ALTER TABLE schedules ADD COLUMN timezone_name TEXT
    CHECK(timezone_name IS NULL OR (length(timezone_name) BETWEEN 1 AND 64));
