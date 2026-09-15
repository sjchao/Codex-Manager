CREATE TABLE IF NOT EXISTS request_token_daily_model_stats (
  day_key TEXT NOT NULL,
  key_id TEXT NOT NULL,
  model_family TEXT NOT NULL,
  request_count INTEGER NOT NULL DEFAULT 0,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  cached_input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(day_key, key_id, model_family)
);

CREATE INDEX IF NOT EXISTS idx_request_token_daily_model_stats_key_id_day_key
  ON request_token_daily_model_stats(key_id, day_key DESC);

-- 中文注释：回填原始表保留窗口内的 DeepSeek 历史用量，之后的增量由写入路径累计。
INSERT OR IGNORE INTO request_token_daily_model_stats (
  day_key,
  key_id,
  model_family,
  request_count,
  input_tokens,
  cached_input_tokens,
  output_tokens,
  total_tokens,
  reasoning_output_tokens
)
SELECT
  date(created_at, 'unixepoch', 'localtime') AS day_key,
  key_id,
  'deepseek' AS model_family,
  COUNT(1) AS request_count,
  IFNULL(SUM(CASE WHEN input_tokens > 0 THEN input_tokens ELSE 0 END), 0),
  IFNULL(SUM(CASE WHEN cached_input_tokens > 0 THEN cached_input_tokens ELSE 0 END), 0),
  IFNULL(SUM(CASE WHEN output_tokens > 0 THEN output_tokens ELSE 0 END), 0),
  IFNULL(
    SUM(
      CASE
        WHEN IFNULL(total_tokens, 0) > 0 THEN total_tokens
        ELSE MAX(
          IFNULL(input_tokens, 0) - IFNULL(cached_input_tokens, 0) + IFNULL(output_tokens, 0),
          0
        )
      END
    ),
    0
  ),
  IFNULL(SUM(CASE WHEN reasoning_output_tokens > 0 THEN reasoning_output_tokens ELSE 0 END), 0)
FROM request_token_stats
WHERE key_id IS NOT NULL
  AND TRIM(key_id) <> ''
  AND model IS NOT NULL
  AND LOWER(TRIM(model)) LIKE 'deepseek%'
GROUP BY day_key, key_id;
