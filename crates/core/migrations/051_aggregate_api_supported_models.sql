PRAGMA foreign_keys = OFF;

CREATE TABLE aggregate_apis_new (
  id TEXT PRIMARY KEY,
  provider_type TEXT NOT NULL DEFAULT 'codex',
  supported_models_json TEXT NOT NULL DEFAULT '[]',
  supplier_name TEXT,
  sort INTEGER NOT NULL DEFAULT 0,
  weight INTEGER NOT NULL DEFAULT 100,
  url TEXT NOT NULL,
  auth_type TEXT NOT NULL DEFAULT 'apikey',
  auth_params_json TEXT,
  action TEXT,
  status TEXT NOT NULL DEFAULT 'active',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  last_test_at INTEGER,
  last_test_status TEXT,
  last_test_error TEXT
);

INSERT INTO aggregate_apis_new (
  id,
  provider_type,
  supported_models_json,
  supplier_name,
  sort,
  weight,
  url,
  auth_type,
  auth_params_json,
  action,
  status,
  created_at,
  updated_at,
  last_test_at,
  last_test_status,
  last_test_error
)
SELECT
  id,
  COALESCE(NULLIF(TRIM(provider_type), ''), 'codex'),
  '[]',
  supplier_name,
  COALESCE(sort, 0),
  CASE WHEN weight IS NULL OR weight <= 0 THEN 100 ELSE weight END,
  url,
  COALESCE(NULLIF(TRIM(auth_type), ''), 'apikey'),
  auth_params_json,
  action,
  COALESCE(NULLIF(TRIM(status), ''), 'active'),
  created_at,
  updated_at,
  last_test_at,
  last_test_status,
  last_test_error
FROM aggregate_apis;

DROP TABLE aggregate_apis;
ALTER TABLE aggregate_apis_new RENAME TO aggregate_apis;

CREATE INDEX IF NOT EXISTS idx_aggregate_apis_created_at ON aggregate_apis(created_at DESC);

PRAGMA foreign_keys = ON;
