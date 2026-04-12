CREATE TABLE IF NOT EXISTS request_token_daily_stats (
  day_key TEXT NOT NULL,
  key_id TEXT NOT NULL,
  request_count INTEGER NOT NULL DEFAULT 0,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  cached_input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
  estimated_cost_usd REAL NOT NULL DEFAULT 0.0,
  PRIMARY KEY(day_key, key_id)
);

CREATE INDEX IF NOT EXISTS idx_request_token_daily_stats_key_id_day_key
  ON request_token_daily_stats(key_id, day_key DESC);

CREATE INDEX IF NOT EXISTS idx_request_token_daily_stats_day_key
  ON request_token_daily_stats(day_key DESC);
