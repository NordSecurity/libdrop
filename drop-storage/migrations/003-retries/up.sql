-- Add migration script here

ALTER TABLE outgoing_paths RENAME COLUMN base_path TO uri;

UPDATE outgoing_paths SET uri = 'file:///' || uri || '/' || relative_path;


