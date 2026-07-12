ALTER TABLE job_outputs ADD COLUMN parent_output_id TEXT REFERENCES job_outputs(id) ON DELETE SET NULL;
ALTER TABLE job_outputs ADD COLUMN producing_action_index INTEGER CHECK (producing_action_index IS NULL OR producing_action_index >= 0);
ALTER TABLE job_outputs ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}';

CREATE INDEX job_outputs_parent_idx ON job_outputs(parent_output_id);
CREATE INDEX job_outputs_action_idx ON job_outputs(job_id, producing_action_index);
