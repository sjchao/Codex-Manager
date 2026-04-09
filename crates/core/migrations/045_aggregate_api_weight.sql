ALTER TABLE aggregate_apis
  ADD COLUMN weight INTEGER NOT NULL DEFAULT 100;

UPDATE aggregate_apis
SET weight = 100
WHERE weight IS NULL OR weight <= 0;
