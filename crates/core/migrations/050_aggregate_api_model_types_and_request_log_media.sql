ALTER TABLE aggregate_apis
  ADD COLUMN model_type TEXT NOT NULL DEFAULT 'text';

ALTER TABLE request_logs
  ADD COLUMN model_type TEXT NOT NULL DEFAULT 'text';

ALTER TABLE request_logs
  ADD COLUMN image_count INTEGER;

ALTER TABLE request_logs
  ADD COLUMN image_size TEXT;

UPDATE aggregate_apis
SET model_type = 'text'
WHERE model_type IS NULL OR TRIM(model_type) = '';

UPDATE request_logs
SET model_type = 'text'
WHERE model_type IS NULL OR TRIM(model_type) = '';

CREATE INDEX IF NOT EXISTS idx_request_logs_model_type_created_at
  ON request_logs(model_type, created_at DESC, id DESC);
