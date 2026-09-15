ALTER TABLE sub2api_accounts ADD COLUMN created_at INTEGER;

-- 中文注释：历史数据没有创建时间，优先从形如 sub2api_{unix_secs}_{seq} 的主键还原，还原失败时退回 updated_at。
UPDATE sub2api_accounts
SET created_at = CASE
  WHEN substr(id, 1, 8) = 'sub2api_'
    AND length(substr(id, 9, 10)) = 10
    AND substr(id, 9, 10) NOT GLOB '*[^0-9]*'
  THEN CAST(substr(id, 9, 10) AS INTEGER)
  ELSE updated_at
END
WHERE created_at IS NULL;
