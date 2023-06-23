-- Add migration script here

CREATE TABLE IF NOT EXISTS incoming_path_reject_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  by_peer INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES incoming_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS outgoing_path_reject_states (
  id INTEGER PRIMARY KEY AUTOINCREMENT, 
  path_id INTEGER NOT NULL,
  by_peer INTEGER NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT(STRFTIME('%Y-%m-%d %H:%M:%f', 'NOW')),
  FOREIGN KEY(path_id) REFERENCES outgoing_paths(id) ON DELETE CASCADE ON UPDATE CASCADE
);
