CREATE TABLE IF NOT EXISTS qoder_members (
  member_id TEXT PRIMARY KEY,
  user_id TEXT,
  name TEXT,
  email TEXT,
  role TEXT,
  status TEXT,
  joined_at TEXT,
  deleted_at TEXT,
  synced_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_qoder_members_status
  ON qoder_members(status);

CREATE TABLE IF NOT EXISTS qoder_usage_events (
  event_id TEXT PRIMARY KEY,
  member_id TEXT,
  user_id TEXT,
  user_email TEXT,
  source TEXT,
  operation TEXT,
  model_tier TEXT,
  credits REAL NOT NULL DEFAULT 0,
  cost REAL,
  occurred_at INTEGER NOT NULL,
  day TEXT NOT NULL,
  synced_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_qoder_usage_events_day
  ON qoder_usage_events(day DESC);

CREATE INDEX IF NOT EXISTS idx_qoder_usage_events_member_day
  ON qoder_usage_events(member_id, day);

CREATE INDEX IF NOT EXISTS idx_qoder_usage_events_occurred_at
  ON qoder_usage_events(occurred_at DESC);

CREATE TABLE IF NOT EXISTS qoder_sync_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  last_sync_at INTEGER,
  last_sync_status TEXT,
  last_sync_error TEXT,
  covered_until INTEGER,
  backfill_from INTEGER,
  last_event_at INTEGER,
  updated_at INTEGER NOT NULL
);
