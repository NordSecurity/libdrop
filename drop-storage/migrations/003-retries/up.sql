-- Add migration script here

PRAGMA foreign_keys = ON;


ALTER TABLE outgoing_paths RENAME COLUMN base_path TO uri;

UPDATE outgoing_paths SET uri = 'file:///' || uri || '/' || relative_path;


CREATE TABLE IF NOT EXISTS sync_transfer (
  sync_id INTEGER PRIMARY KEY AUTOINCREMENT, -- use separate primary key for cascade to work across sync_ tables
  transfer_id TEXT NOT NULL,
  local_state INTEGER NOT NULL,
  remote_state INTEGER NOT NULL,
  FOREIGN KEY(transfer_id) REFERENCES transfers(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS sync_outgoing_files (
  sync_id INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  local_state INTEGER NOT NULL,
  remote_state INTEGER NOT NULL,
  PRIMARY KEY(sync_id, path_id)
  FOREIGN KEY(sync_id) REFERENCES sync_transfer(sync_id) ON DELETE CASCADE ON UPDATE CASCADE
  FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS sync_incoming_files (
  sync_id INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  local_state INTEGER NOT NULL,
  remote_state INTEGER NOT NULL,
  PRIMARY KEY(sync_id, path_id)
  FOREIGN KEY(sync_id) REFERENCES sync_transfer(sync_id) ON DELETE CASCADE ON UPDATE CASCADE
  FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS sync_incoming_files_inflight (
  sync_id INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  base_dir TEXT NOT NULL,
  FOREIGN KEY(sync_id, path_id) REFERENCES sync_incoming_files(sync_id, path_id) ON DELETE CASCADE ON UPDATE CASCADE
);


 -- paths soft deletion
ALTER TABLE incoming_paths ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT FALSE CHECK (is_deleted IN (FALSE, TRUE));
ALTER TABLE outgoing_paths ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT FALSE CHECK (is_deleted IN (FALSE, TRUE));


 -- storing progress
ALTER TABLE incoming_path_reject_states ADD COLUMN bytes_received INTEGER NOT NULL DEFAULT 0;
ALTER TABLE outgoing_path_reject_states ADD COLUMN bytes_sent     INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS incoming_path_paused_states (
  path_id INTEGER NOT NULL,
  bytes_received INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS outgoing_path_paused_states (
  path_id INTEGER NOT NULL,
  bytes_sent INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);


