ALTER TABLE qoder_sync_state ADD COLUMN shared_package_limit REAL;
ALTER TABLE qoder_sync_state ADD COLUMN shared_package_remaining REAL;
ALTER TABLE qoder_sync_state ADD COLUMN shared_package_synced_at INTEGER;
