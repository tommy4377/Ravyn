-- Completed jobs persisted whatever the last periodic progress flush wrote,
-- so a finished transfer could remain stored at e.g. 94% forever and render
-- a stale sub-100% bar after every reload. New completions snap their
-- counters at terminal time; this backfills history.
UPDATE jobs
SET downloaded_bytes = total_bytes
WHERE status = 'completed'
  AND total_bytes IS NOT NULL
  AND downloaded_bytes <> total_bytes;
