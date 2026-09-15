ALTER TABLE request_token_daily_stats ADD COLUMN actual_cost_usd REAL NOT NULL DEFAULT 0.0;

UPDATE request_token_daily_stats
SET actual_cost_usd = IFNULL((
  SELECT SUM(IFNULL(r.upstream_actual_cost, 0.0))
  FROM request_logs r
  WHERE r.key_id = request_token_daily_stats.key_id
    AND date(r.created_at, 'unixepoch', 'localtime') = request_token_daily_stats.day_key
), 0.0);
