-- Add migration script here
--- todo: the ALTER TABLE DROP COLUMN is only supported since 2021-03-12 (3.35.0)
--- ALTER TABLE sync_transfer DROP COLUMN remote_state;

CREATE TABLE IF NOT EXISTS sync_transfer_new (
  sync_id INTEGER PRIMARY KEY AUTOINCREMENT, -- use separate primary key for cascade to work across sync_ tables
  transfer_id TEXT NOT NULL,
  local_state INTEGER NOT NULL,
  FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE CASCADE ON UPDATE CASCADE
);

INSERT INTO sync_transfer_new (sync_id, transfer_id, local_state) SELECT sync_id, transfer_id, local_state FROM sync_transfer;

DROP TABLE sync_transfer;

ALTER TABLE sync_transfer_new RENAME TO sync_transfer;

