pub mod error;
pub mod types;

use std::vec;

use include_dir::{include_dir, Dir};
use lazy_static::lazy_static;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension, Transaction};
use rusqlite_migration::Migrations;
use slog::Logger;
use types::{
    DbTransferType, IncomingPath, IncomingPathStateEvent, IncomingPathStateEventData, OutgoingPath,
    OutgoingPathStateEvent, OutgoingPathStateEventData, Transfer, TransferFiles,
    TransferIncomingPath, TransferOutgoingPath, TransferStateEvent,
};
use uuid::Uuid;

use crate::error::Error;
pub use crate::types::{
    FileChecksum, FileToRetry, IncomingTransferInfo, TransferInfo, TransferToRetry, TransferType,
};

type Result<T> = std::result::Result<T, Error>;
type QueryResult<T> = std::result::Result<T, rusqlite::Error>;
// SQLite storage wrapper
pub struct Storage {
    _logger: Logger,
    pool: Pool<SqliteConnectionManager>,
}

static MIGRATIONS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations");

lazy_static! {
    static ref MIGRATIONS: Migrations<'static> = Migrations::from_directory(&MIGRATIONS_DIR)
        .expect("Failed to gather migrations from directory");
}

impl Storage {
    pub fn new(logger: Logger, path: &str) -> Result<Self> {
        let manager = match path {
            ":memory:" => SqliteConnectionManager::memory(),
            _ => SqliteConnectionManager::file(path),
        };
        let pool = Pool::new(manager)?;

        let mut conn = pool.get()?;
        MIGRATIONS
            .to_latest(&mut conn)
            .map_err(|e| Error::InternalError(e.to_string()))?;

        Ok(Self {
            _logger: logger,
            pool,
        })
    }

    pub fn insert_transfer(&self, transfer: &TransferInfo) -> Result<()> {
        let transfer_type_int = match &transfer.files {
            TransferFiles::Incoming(_) => TransferType::Incoming as u32,
            TransferFiles::Outgoing(_) => TransferType::Outgoing as u32,
        };

        let tid = transfer.id.hyphenated();

        let mut conn = self.pool.get()?;
        let conn = conn.transaction()?;

        conn.execute(
            "INSERT INTO transfers (id, peer, is_outgoing) VALUES (?1, ?2, ?3)",
            params![tid.as_uuid().to_string(), transfer.peer, transfer_type_int],
        )?;

        match &transfer.files {
            TransferFiles::Incoming(files) => {
                for file in files {
                    Self::insert_incoming_path(&conn, transfer.id, file)?;
                }
            }
            TransferFiles::Outgoing(files) => {
                for file in files {
                    Self::insert_outgoing_path(&conn, transfer.id, file)?;
                }
            }
        }

        conn.commit()?;

        Ok(())
    }

    pub fn incoming_transfer_info(
        &self,
        transfer_id: Uuid,
    ) -> Result<Option<IncomingTransferInfo>> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.pool.get()?;
        let conn = conn.transaction()?;

        let record: Option<String> = conn
            .query_row(
                "SELECT peer FROM transfers WHERE id = ?1 AND is_outgoing = ?2",
                params![tid.as_uuid().to_string(), TransferType::Incoming as u32],
                |row| row.get("peer"),
            )
            .optional()?;

        let peer = if let Some(record) = record {
            record
        } else {
            return Ok(None);
        };

        let mut stmt = conn.prepare(
            "SELECT relative_path, path_hash as file_id, bytes as size
            FROM incoming_paths
            WHERE transfer_id = ?1",
        )?;
        let files = stmt
            .query_map(params![tid.as_uuid().to_string()], |row| {
                Ok(TransferIncomingPath {
                    relative_path: row.get("relative_path")?,
                    file_id: row.get("file_id")?,
                    size: row.get("size")?,
                })
            })?
            .collect::<QueryResult<Vec<_>>>()?;

        Ok(Some(IncomingTransferInfo { peer, files }))
    }

    fn insert_incoming_path(
        conn: &Transaction<'_>,
        transfer_id: Uuid,
        path: &TransferIncomingPath,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        conn.execute(
            "INSERT INTO incoming_paths (transfer_id, relative_path, path_hash, bytes)
            VALUES (?1, ?2, ?3, ?4)",
            params![
                tid.as_uuid().to_string(),
                path.relative_path,
                path.file_id,
                path.size
            ],
        )?;

        Ok(())
    }

    fn insert_outgoing_path(
        conn: &Transaction<'_>,
        transfer_id: Uuid,
        path: &TransferOutgoingPath,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        conn.execute(
            "INSERT INTO outgoing_paths (transfer_id, relative_path, path_hash, bytes, base_path)
            VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                tid.as_uuid().to_string(),
                path.relative_path,
                path.file_id,
                path.size,
                path.base_path
            ],
        )?;

        Ok(())
    }

    pub fn save_checksum(&self, transfer_id: Uuid, file_id: &str, checksum: &[u8]) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "UPDATE incoming_paths SET checksum = ?3 WHERE transfer_id = ?1 AND path_hash = ?2",
            params![tid.as_uuid().to_string(), file_id, checksum],
        )?;

        Ok(())
    }

    pub fn fetch_checksums(&self, transfer_id: Uuid) -> Result<Vec<FileChecksum>> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        let out = conn
            .prepare(
                "SELECT path_hash as file_id, checksum FROM incoming_paths WHERE transfer_id = ?1",
            )?
            .query_map(params![tid.as_uuid().to_string()], |row| {
                Ok(FileChecksum {
                    file_id: row.get("file_id")?,
                    checksum: row.get("checksum")?,
                })
            })?
            .collect::<QueryResult<Vec<_>>>()?;

        Ok(out)
    }

    pub fn insert_transfer_active_state(&self, transfer_id: Uuid) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO transfer_active_states (transfer_id) VALUES (?1)",
            params![tid.as_uuid().to_string()],
        )?;

        Ok(())
    }

    pub fn insert_transfer_failed_state(&self, transfer_id: Uuid, error: u32) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO transfer_failed_states (transfer_id, status_code) VALUES (?1, ?2)",
            params![tid.as_uuid().to_string(), error],
        )?;

        Ok(())
    }

    pub fn insert_transfer_cancel_state(&self, transfer_id: Uuid, by_peer: bool) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO transfer_cancel_states (transfer_id, by_peer) VALUES (?1, ?2)",
            params![tid.as_uuid().to_string(), by_peer],
        )?;

        Ok(())
    }

    pub fn insert_outgoing_path_pending_state(
        &self,
        transfer_id: Uuid,
        file_id: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO outgoing_path_pending_states (path_id) VALUES ((SELECT id FROM \
             outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2))",
            params![tid.as_uuid().to_string(), file_id],
        )?;

        Ok(())
    }

    pub fn insert_incoming_path_pending_state(
        &self,
        transfer_id: Uuid,
        file_id: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO incoming_path_pending_states (path_id) VALUES ((SELECT id FROM \
             incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2))",
            params![tid.as_uuid().to_string(), file_id],
        )?;

        Ok(())
    }

    pub fn insert_outgoing_path_started_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO outgoing_path_started_states (path_id, bytes_sent) VALUES ((SELECT id \
             FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3)",
            params![tid.as_uuid().to_string(), path_id, 0],
        )?;

        Ok(())
    }

    pub fn insert_incoming_path_started_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        base_dir: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO incoming_path_started_states (path_id, base_dir, bytes_received) VALUES \
             ((SELECT id FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            params![tid.as_uuid().to_string(), path_id, base_dir, 0],
        )?;

        Ok(())
    }

    pub fn insert_outgoing_path_cancel_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        by_peer: bool,
        bytes_sent: i64,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO outgoing_path_cancel_states (path_id, by_peer, bytes_sent) VALUES \
             ((SELECT id FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            params![tid.as_uuid().to_string(), path_id, by_peer, bytes_sent],
        )?;

        Ok(())
    }

    pub fn insert_incoming_path_cancel_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        by_peer: bool,
        bytes_received: i64,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO incoming_path_cancel_states (path_id, by_peer, bytes_received) VALUES \
             ((SELECT id FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            params![tid.as_uuid().to_string(), path_id, by_peer, bytes_received],
        )?;

        Ok(())
    }

    pub fn insert_incoming_path_failed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        error: u32,
        bytes_received: i64,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO incoming_path_failed_states (path_id, status_code, bytes_received) \
             VALUES ((SELECT id FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), \
             ?3, ?4)",
            params![tid.as_uuid().to_string(), path_id, error, bytes_received],
        )?;

        Ok(())
    }

    pub fn insert_outgoing_path_failed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        error: u32,
        bytes_sent: i64,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO outgoing_path_failed_states (path_id, status_code, bytes_sent) VALUES \
             ((SELECT id FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            params![tid.as_uuid().to_string(), path_id, error, bytes_sent],
        )?;

        Ok(())
    }

    pub fn insert_outgoing_path_completed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO outgoing_path_completed_states (path_id) VALUES ((SELECT id FROM \
             outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2))",
            params![tid.as_uuid().to_string(), path_id],
        )?;

        Ok(())
    }

    pub fn insert_incoming_path_completed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        final_path: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO incoming_path_completed_states (path_id, final_path) VALUES ((SELECT id \
             FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3)",
            params![tid.as_uuid().to_string(), path_id, final_path],
        )?;

        Ok(())
    }

    pub fn insert_outgoing_path_reject_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        by_peer: bool,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO outgoing_path_reject_states (path_id, by_peer) VALUES ((SELECT id FROM \
             outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3)",
            params![tid.as_uuid().to_string(), path_id, by_peer],
        )?;

        Ok(())
    }

    pub fn insert_incoming_path_reject_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        by_peer: bool,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO incoming_path_reject_states (path_id, by_peer) VALUES ((SELECT id FROM \
             incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3)",
            params![tid.as_uuid().to_string(), path_id, by_peer],
        )?;

        Ok(())
    }

    pub fn purge_transfers_until(&self, until_timestamp: i64) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute(
            "DELETE FROM transfers WHERE created_at < datetime(?1, 'unixepoch')",
            params![until_timestamp],
        )?;

        Ok(())
    }

    fn purge_transfer(&self, transfer_id: String) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute("DELETE FROM transfers WHERE id = ?1", params![transfer_id])?;

        Ok(())
    }

    pub fn purge_transfers(&self, transfer_ids: Vec<String>) -> Result<()> {
        for id in transfer_ids {
            self.purge_transfer(id)?;
        }

        Ok(())
    }

    pub fn transfers_to_retry(&self) -> Result<Vec<TransferToRetry>> {
        let conn = self.pool.get()?;
        let rec_transfers = conn
            .prepare(
                r#"
            SELECT transfers.id as id, peer, COUNT(transfer_cancel_states.id) as cancel_count
            FROM transfers
            LEFT JOIN transfer_cancel_states ON transfers.id = transfer_cancel_states.transfer_id
            WHERE
                is_outgoing = ?1
            GROUP BY transfers.id
            HAVING cancel_count = 0
            "#,
            )?
            .query_map(params![TransferType::Outgoing as u32], |row| {
                Ok((row.get("id")?, row.get("peer")?))
            })?
            .collect::<QueryResult<Vec<(String, String)>>>()?;

        let mut out = Vec::with_capacity(rec_transfers.len());
        for rec_transfer in rec_transfers {
            let (id, peer) = rec_transfer;

            let files = conn
                .prepare(
                    r#"
                SELECT relative_path, base_path, path_hash, bytes
                FROM outgoing_paths
                WHERE
                    transfer_id = ?1
                "#,
                )?
                .query_map(params![id], |row| {
                    Ok(FileToRetry {
                        file_id: row.get("path_hash")?,
                        basepath: row.get("base_path")?,
                        subpath: row.get("relative_path")?,
                        size: row.get("bytes")?,
                    })
                })?
                .collect::<QueryResult<Vec<FileToRetry>>>()?;

            out.push(TransferToRetry {
                uuid: Uuid::parse_str(&id).map_err(|e| Error::InternalError(e.to_string()))?,
                peer,
                files,
            });
        }

        // conn.commit().await?;

        Ok(out)
    }

    pub fn incoming_files_to_resume(&self, transfer_id: Uuid) -> Result<Vec<FileToRetry>> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.pool.get()?;
        let conn = conn.transaction()?;
        let out = conn
            .prepare(
                r#"
                   SELECT
                path_hash as file_id,
                relative_path as subpath,
                bytes as size,
                strt.base_dir as basedir,
                MAX(strt.created_at) as last_start,
                MAX(canc.created_at) as last_cancel,
                MAX(fail.created_at) as last_fail,
                MAX(comp.created_at) as last_complete,
                COUNT(rej.created_at) as rej_count
            FROM incoming_paths p
            INNER JOIN incoming_path_started_states strt ON p.id = strt.path_id
            LEFT JOIN incoming_path_cancel_states canc ON p.id = canc.path_id
            LEFT JOIN incoming_path_failed_states fail ON p.id = fail.path_id
            LEFT JOIN incoming_path_completed_states comp ON p.id = comp.path_id
            LEFT JOIN incoming_path_reject_states rej ON p.id = rej.path_id
            WHERE transfer_id = ?1
            GROUP BY p.id
            HAVING
                rej_count = 0
                AND (last_cancel IS NULL OR last_start > last_cancel)
                AND (last_fail IS NULL OR last_start > last_fail)
                AND (last_complete IS NULL OR last_start > last_complete)
            "#,
            )?
            .query_map(params![tid.as_uuid().to_string()], |row| {
                Ok(FileToRetry {
                    file_id: row.get("file_id")?,
                    basepath: row.get("basedir")?,
                    subpath: row.get("subpath")?,
                    size: row.get("size")?,
                })
            })?
            .collect::<QueryResult<Vec<FileToRetry>>>()?;

        conn.commit()?;

        Ok(out)
    }

    pub fn transfers_since(&self, since_timestamp: i64) -> Result<Vec<Transfer>> {
        let conn = self.pool.get()?;
        let mut transfers = conn
            .prepare(
                r#"
                SELECT id, peer, created_at, is_outgoing FROM transfers
                WHERE created_at >= datetime(?1, 'unixepoch')
                "#,
            )?
            .query_map(params![since_timestamp], |row| {
                let transfer_type = match row.get::<_, u32>("is_outgoing")? {
                    0 => DbTransferType::Incoming(vec![]),
                    1 => DbTransferType::Outgoing(vec![]),
                    _ => unreachable!(),
                };

                let id: String = row.get("id")?;

                Ok(Transfer {
                    id: Uuid::parse_str(&id).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    peer_id: row.get("peer")?,
                    transfer_type,
                    created_at: row.get("created_at")?,
                    states: vec![],
                })
            })?
            .collect::<QueryResult<Vec<Transfer>>>()?;

        for transfer in &mut transfers {
            match transfer.transfer_type {
                DbTransferType::Incoming(_) => {
                    transfer.transfer_type =
                        DbTransferType::Incoming(self.get_incoming_paths(transfer.id)?)
                }
                DbTransferType::Outgoing(_) => {
                    transfer.transfer_type =
                        DbTransferType::Outgoing(self.get_outgoing_paths(transfer.id)?)
                }
            }

            let tid = transfer.id.hyphenated();

            transfer.states.extend(
                conn.prepare(
                    r#"
                    SELECT created_at FROM transfer_active_states WHERE transfer_id = ?1
                    "#,
                )?
                .query_map(params![tid.as_uuid().to_string()], |row| {
                    Ok(TransferStateEvent {
                        transfer_id: transfer.id,
                        created_at: row.get("created_at")?,
                        data: types::TransferStateEventData::Active,
                    })
                })?
                .collect::<QueryResult<Vec<TransferStateEvent>>>()?,
            );

            transfer.states.extend(
                conn.prepare(
                    r#"
                    SELECT created_at, by_peer FROM transfer_cancel_states WHERE transfer_id = ?1
                    "#,
                )?
                .query_map(params![tid.as_uuid().to_string()], |row| {
                    Ok(TransferStateEvent {
                        transfer_id: transfer.id,
                        created_at: row.get("created_at")?,
                        data: types::TransferStateEventData::Cancel {
                            by_peer: row.get("by_peer")?,
                        },
                    })
                })?
                .collect::<QueryResult<Vec<TransferStateEvent>>>()?,
            );

            transfer.states.extend(
                conn.prepare(
                    r#"
                    SELECT created_at, status_code FROM transfer_failed_states WHERE transfer_id = ?1
                    "#,
                )?
                .query_map(params![tid.as_uuid().to_string()], |row| {
                    Ok(TransferStateEvent {
                        transfer_id: transfer.id,
                        created_at: row.get("created_at")?,
                        data: types::TransferStateEventData::Failed {
                            status_code: row.get("status_code")?,
                        },
                    })
                })?
                .collect::<QueryResult<Vec<TransferStateEvent>>>()?,
            );

            transfer
                .states
                .sort_by(|a, b| a.created_at.cmp(&b.created_at));
        }

        Ok(transfers)
    }

    fn get_outgoing_paths(&self, transfer_id: Uuid) -> Result<Vec<OutgoingPath>> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        let mut paths = conn
            .prepare(
                r#"
                SELECT * FROM outgoing_paths WHERE transfer_id = ?1
                "#,
            )?
            .query_map(params![tid.as_uuid().to_string()], |row| {
                Ok(OutgoingPath {
                    id: row.get("id")?,
                    transfer_id,
                    base_path: row.get("base_path")?,
                    relative_path: row.get("relative_path")?,
                    file_id: row.get("path_hash")?,
                    bytes: row.get("bytes")?,
                    created_at: row.get("created_at")?,
                    states: vec![],
                })
            })?
            .collect::<QueryResult<Vec<OutgoingPath>>>()?;

        for path in &mut paths {
            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM outgoing_path_pending_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(OutgoingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: OutgoingPathStateEventData::Pending,
                    })
                })?
                .collect::<QueryResult<Vec<OutgoingPathStateEvent>>>()?,
            );

            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM outgoing_path_started_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(OutgoingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: OutgoingPathStateEventData::Started {
                            bytes_sent: row.get("bytes_sent")?,
                        },
                    })
                })?
                .collect::<QueryResult<Vec<OutgoingPathStateEvent>>>()?,
            );

            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM outgoing_path_cancel_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(OutgoingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: OutgoingPathStateEventData::Cancel {
                            by_peer: row.get("by_peer")?,
                            bytes_sent: row.get("bytes_sent")?,
                        },
                    })
                })?
                .collect::<QueryResult<Vec<OutgoingPathStateEvent>>>()?,
            );

            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM outgoing_path_failed_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(OutgoingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: OutgoingPathStateEventData::Failed {
                            status_code: row.get("status_code")?,
                            bytes_sent: row.get("bytes_sent")?,
                        },
                    })
                })?
                .collect::<QueryResult<Vec<OutgoingPathStateEvent>>>()?,
            );

            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM outgoing_path_completed_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(OutgoingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: OutgoingPathStateEventData::Completed,
                    })
                })?
                .collect::<QueryResult<Vec<OutgoingPathStateEvent>>>()?,
            );

            path.states.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        }

        Ok(paths)
    }

    fn get_incoming_paths(&self, transfer_id: Uuid) -> Result<Vec<IncomingPath>> {
        let tid = transfer_id.hyphenated();

        let conn = self.pool.get()?;
        let mut paths = conn
            .prepare(
                r#"
                SELECT * FROM incoming_paths WHERE transfer_id = ?1
                "#,
            )?
            .query_map(params![tid.as_uuid().to_string()], |row| {
                Ok(IncomingPath {
                    id: row.get("id")?,
                    transfer_id,
                    relative_path: row.get("relative_path")?,
                    file_id: row.get("path_hash")?,
                    bytes: row.get("bytes")?,
                    created_at: row.get("created_at")?,
                    states: vec![],
                })
            })?
            .collect::<QueryResult<Vec<IncomingPath>>>()?;

        for path in &mut paths {
            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM incoming_path_pending_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(IncomingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: IncomingPathStateEventData::Pending,
                    })
                })?
                .collect::<QueryResult<Vec<IncomingPathStateEvent>>>()?,
            );

            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM incoming_path_started_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(IncomingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: IncomingPathStateEventData::Started {
                            bytes_received: row.get("bytes_received")?,
                            base_dir: row.get("base_dir")?,
                        },
                    })
                })?
                .collect::<QueryResult<Vec<IncomingPathStateEvent>>>()?,
            );

            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM incoming_path_cancel_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(IncomingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: IncomingPathStateEventData::Cancel {
                            by_peer: row.get("by_peer")?,
                            bytes_received: row.get("bytes_received")?,
                        },
                    })
                })?
                .collect::<QueryResult<Vec<IncomingPathStateEvent>>>()?,
            );

            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM incoming_path_failed_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(IncomingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: IncomingPathStateEventData::Failed {
                            status_code: row.get("status_code")?,
                            bytes_received: row.get("bytes_received")?,
                        },
                    })
                })?
                .collect::<QueryResult<Vec<IncomingPathStateEvent>>>()?,
            );

            path.states.extend(
                conn.prepare(
                    r#"
                    SELECT * FROM incoming_path_completed_states WHERE path_id = ?1
                    "#,
                )?
                .query_map(params![path.id], |row| {
                    Ok(IncomingPathStateEvent {
                        path_id: row.get("path_id")?,
                        created_at: row.get("created_at")?,
                        data: IncomingPathStateEventData::Completed {
                            final_path: row.get("final_path")?,
                        },
                    })
                })?
                .collect::<QueryResult<Vec<IncomingPathStateEvent>>>()?,
            );

            path.states.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        }

        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_transfer() {
        let logger = slog::Logger::root(slog::Discard, slog::o!());
        let storage = Storage::new(logger, ":memory:").unwrap();

        let transfer_id_1: Uuid = "23e488a4-0521-11ee-be56-0242ac120002".parse().unwrap();
        let transfer_id_2: Uuid = "23e48d7c-0521-11ee-be56-0242ac120002".parse().unwrap();

        {
            let transfer = TransferInfo {
                id: transfer_id_1,
                peer: "1.2.3.4".to_string(),
                files: TransferFiles::Incoming(vec![
                    TransferIncomingPath {
                        file_id: "id1".to_string(),
                        relative_path: "1".to_string(),
                        size: 1024,
                    },
                    TransferIncomingPath {
                        file_id: "id2".to_string(),
                        relative_path: "2".to_string(),
                        size: 2048,
                    },
                ]),
            };

            storage.insert_transfer(&transfer).unwrap();
        }

        {
            let transfer = TransferInfo {
                id: transfer_id_2,
                peer: "5.6.7.8".to_string(),
                files: TransferFiles::Outgoing(vec![
                    TransferOutgoingPath {
                        file_id: "id3".to_string(),
                        size: 1024,
                        base_path: "/dir".to_string(),
                        relative_path: "3".to_string(),
                    },
                    TransferOutgoingPath {
                        file_id: "id4".to_string(),
                        relative_path: "4".to_string(),
                        base_path: "/dir".to_string(),
                        size: 2048,
                    },
                ]),
            };

            storage.insert_transfer(&transfer).unwrap();
        }

        {
            let transfers = storage.transfers_since(0).unwrap();
            assert_eq!(transfers.len(), 2);

            let incoming_transfer = &transfers[0];
            let outgoing_transfer = &transfers[1];

            assert_eq!(incoming_transfer.id, transfer_id_1);
            assert_eq!(outgoing_transfer.id, transfer_id_2);

            assert_eq!(incoming_transfer.peer_id, "1.2.3.4".to_string());
            assert_eq!(outgoing_transfer.peer_id, "5.6.7.8".to_string());
        }

        storage
            .purge_transfers(vec![transfer_id_1.to_string(), transfer_id_2.to_string()])
            .unwrap();

        let transfers = storage.transfers_since(0).unwrap();

        assert_eq!(transfers.len(), 0);
    }
}
