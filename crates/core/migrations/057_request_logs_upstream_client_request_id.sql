ALTER TABLE request_logs ADD COLUMN upstream_client_request_id TEXT;

CREATE INDEX IF NOT EXISTS idx_request_logs_aggregate_api_upstream_client_request_id
  ON request_logs(aggregate_api_id, upstream_client_request_id);
