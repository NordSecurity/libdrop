pub mod error;
pub mod types;

use std::str::FromStr;

use slog::Logger;
use sqlx::{sqlite::SqliteConnectOptions, SqliteConnection, SqlitePool};
use types::{
    DbTransferType, IncomingPath, IncomingPathStateEvent, IncomingPathStateEventData, OutgoingPath,
    OutgoingPathStateEvent, OutgoingPathStateEventData, Transfer, TransferFiles,
    TransferIncomingPath, TransferOutgoingPath, TransferStateEvent,
};
use uuid::{fmt::Hyphenated, Uuid};

use crate::error::Error;
pub use crate::types::{
    FileChecksum, FileToRetry, FinishedFile, TransferIncomingMode, TransferInfo, TransferToRetry,
    TransferType,
};

type Result<T> = std::result::Result<T, Error>;
// SQLite storage wrapper
pub struct Storage {
    _logger: Logger,
    conn: SqlitePool,
}

impl Storage {
    pub async fn new(logger: Logger, path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(path)?.create_if_missing(true);
        let conn = SqlitePool::connect_with(options).await?;

        sqlx::migrate!("./migrations")
            .run(&mut conn.acquire().await?)
            .await
            .map_err(|e| error::Error::InternalError(format!("Failed to run migrations: {e}")))?;

        Ok(Self {
            _logger: logger,
            conn,
        })
    }

    pub async fn insert_transfer(&self, transfer: &TransferInfo) -> Result<TransferIncomingMode> {
        let transfer_type_int = match &transfer.files {
            TransferFiles::Incoming(_) => TransferType::Incoming as u32,
            TransferFiles::Outgoing(_) => TransferType::Outgoing as u32,
        };

        let tid = transfer.id.hyphenated();

        let mut conn = self.conn.begin().await?;

        let affected = sqlx::query!(
            "INSERT OR IGNORE INTO transfers (id, peer, is_outgoing) VALUES (?1, ?2, ?3)",
            tid,
            transfer.peer,
            transfer_type_int,
        )
        .execute(&mut *conn)
        .await?
        .rows_affected();

        let mode = if affected == 0 {
            TransferIncomingMode::Resume
        } else {
            TransferIncomingMode::New
        };

        match &transfer.files {
            TransferFiles::Incoming(files) => {
                for file in files {
                    Self::insert_incoming_path(&mut conn, transfer.id, file).await?;
                }
            }
            TransferFiles::Outgoing(files) => {
                for file in files {
                    Self::insert_outgoing_path(&mut conn, transfer.id, file).await?;
                }
            }
        }

        conn.commit().await?;

        Ok(mode)
    }

    async fn insert_incoming_path(
        conn: &mut SqliteConnection,
        transfer_id: Uuid,
        path: &TransferIncomingPath,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        sqlx::query!(
            r#"
            INSERT INTO incoming_paths (transfer_id, relative_path, path_hash, bytes)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT DO NOTHING
            "#,
            tid,
            path.relative_path,
            path.file_id,
            path.size,
        )
        .execute(conn)
        .await?;

        Ok(())
    }

    async fn insert_outgoing_path(
        conn: &mut SqliteConnection,
        transfer_id: Uuid,
        path: &TransferOutgoingPath,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        sqlx::query!(
            r#"
            INSERT INTO outgoing_paths (transfer_id, relative_path, path_hash, bytes, base_path)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            tid,
            path.relative_path,
            path.file_id,
            path.size,
            path.base_path,
        )
        .execute(conn)
        .await?;

        Ok(())
    }

    pub async fn save_checksum(
        &self,
        transfer_id: Uuid,
        file_id: &str,
        checksum: &[u8],
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "UPDATE incoming_paths SET checksum = ?3 WHERE transfer_id = ?1 AND path_hash = ?2",
            tid,
            file_id,
            checksum,
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn fetch_checksums(&self, transfer_id: Uuid) -> Result<Vec<FileChecksum>> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        let out = sqlx::query_as!(
            FileChecksum,
            "SELECT path_hash as file_id, checksum FROM incoming_paths WHERE transfer_id = ?1",
            tid,
        )
        .fetch_all(&mut *conn)
        .await?;

        Ok(out)
    }

    pub async fn insert_transfer_active_state(&self, transfer_id: Uuid) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO transfer_active_states (transfer_id) VALUES (?1)",
            tid,
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_transfer_failed_state(&self, transfer_id: Uuid, error: u32) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO transfer_failed_states (transfer_id, status_code) VALUES (?1, ?2)",
            tid,
            error,
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_transfer_cancel_state(
        &self,
        transfer_id: Uuid,
        by_peer: bool,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO transfer_cancel_states (transfer_id, by_peer) VALUES (?1, ?2)",
            tid,
            by_peer,
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_outgoing_path_pending_state(
        &self,
        transfer_id: Uuid,
        file_id: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            r#"
            INSERT INTO outgoing_path_pending_states (path_id)
            SELECT id FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2
            "#,
            tid,
            file_id
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_incoming_path_pending_state(
        &self,
        transfer_id: Uuid,
        file_id: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_pending_states (path_id) VALUES ((SELECT id FROM \
             incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2))",
            tid,
            file_id,
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_outgoing_path_started_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_started_states (path_id, bytes_sent) VALUES ((SELECT id \
             FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3)",
            tid,
            path_id,
            0
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_incoming_path_started_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        base_dir: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_started_states (path_id, base_dir, bytes_received) VALUES \
             ((SELECT id FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            tid,
            path_id,
            base_dir,
            0
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_outgoing_path_cancel_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        by_peer: bool,
        bytes_sent: i64,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_cancel_states (path_id, by_peer, bytes_sent) VALUES \
             ((SELECT id FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            tid,
            path_id,
            by_peer,
            bytes_sent
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_incoming_path_cancel_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        by_peer: bool,
        bytes_received: i64,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_cancel_states (path_id, by_peer, bytes_received) VALUES \
             ((SELECT id FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            tid,
            path_id,
            by_peer,
            bytes_received
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_incoming_path_failed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        error: u32,
        bytes_received: i64,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_failed_states (path_id, status_code, bytes_received) \
             VALUES ((SELECT id FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), \
             ?3, ?4)",
            tid,
            path_id,
            error,
            bytes_received
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_outgoing_path_failed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        error: u32,
        bytes_sent: i64,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_failed_states (path_id, status_code, bytes_sent) VALUES \
             ((SELECT id FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            tid,
            path_id,
            error,
            bytes_sent
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_outgoing_path_completed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_completed_states (path_id) VALUES ((SELECT id FROM \
             outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2))",
            tid,
            path_id,
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn insert_incoming_path_completed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        final_path: &str,
    ) -> Result<()> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_completed_states (path_id, final_path) VALUES ((SELECT id \
             from incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3)",
            tid,
            path_id,
            final_path
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn purge_transfers_until(&self, until_timestamp: i64) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "DELETE FROM transfers WHERE created_at < datetime(?1, 'unixepoch')",
            until_timestamp
        )
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    /// From the FAQ:
    /// ### How can I do a `SELECT ... WHERE foo IN (...)` query?
    /// In the future SQLx will support binding arrays as a comma-separated list
    /// for every database, but unfortunately there's no general solution
    /// for that currently in SQLx itself. You would need to manually
    /// generate the query, at which point it cannot be used with the
    /// macros.
    async fn purge_transfer(&self, transfer_id: String) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!("DELETE FROM transfers WHERE id = $1", transfer_id)
            .execute(&mut *conn)
            .await?;

        Ok(())
    }

    pub async fn purge_transfers(&self, transfer_ids: Vec<String>) -> Result<()> {
        for id in transfer_ids {
            self.purge_transfer(id).await?;
        }

        Ok(())
    }

    pub async fn transfers_to_retry(&self) -> Result<Vec<TransferToRetry>> {
        let mut conn = self.conn.begin().await?;

        let rec_transfers = sqlx::query!(
            r#"
            SELECT transfers.id as  "id: Hyphenated", peer, COUNT(transfer_cancel_states.id) as cancel_count
            FROM transfers
            LEFT JOIN transfer_cancel_states ON transfers.id = transfer_cancel_states.transfer_id
            WHERE
                is_outgoing = ?1
            GROUP BY transfers.id
            HAVING cancel_count = 0
            "#,
            TransferType::Outgoing as u32
        )
        .fetch_all(&mut *conn)
        .await?;

        let mut out = Vec::with_capacity(rec_transfers.len());
        for rec_transfer in rec_transfers {
            let files = sqlx::query!(
                "SELECT relative_path, base_path, path_hash, bytes FROM outgoing_paths WHERE \
                 transfer_id = ?1",
                rec_transfer.id
            )
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .map(|rec_file| FileToRetry {
                file_id: rec_file.path_hash,
                basepath: rec_file.base_path,
                subpath: rec_file.relative_path,
                size: rec_file.bytes as _,
            })
            .collect();

            out.push(TransferToRetry {
                uuid: rec_transfer.id.into_uuid(),
                peer: rec_transfer.peer,
                files,
            });
        }

        conn.commit().await?;

        Ok(out)
    }

    pub async fn incoming_files_to_resume(&self, transfer_id: Uuid) -> Result<Vec<FileToRetry>> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.begin().await?;

        let out = sqlx::query!(
            r#"
            SELECT 
                path_hash as "file_id!",
                relative_path as "subpath!",
                bytes as "size!",
                strt.base_dir as "basedir!",
                MAX(strt.created_at) as last_start,
                MAX(canc.created_at) as last_cancel,
                MAX(fail.created_at) as last_fail,
                MAX(comp.created_at) as last_complete
            FROM incoming_paths p
            INNER JOIN incoming_path_started_states strt ON p.id = strt.path_id
            LEFT JOIN incoming_path_cancel_states canc ON p.id = canc.path_id
            LEFT JOIN incoming_path_failed_states fail ON p.id = fail.path_id
            LEFT JOIN incoming_path_completed_states comp ON p.id = comp.path_id
            WHERE transfer_id = ?1
            GROUP BY p.id
            HAVING 
                last_start IS NOT NULL
                AND (last_cancel IS NULL OR last_start > last_cancel)
                AND (last_fail IS NULL OR last_start > last_fail)
                AND (last_complete IS NULL OR last_start > last_complete)
            "#,
            tid
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|r| FileToRetry {
            file_id: r.file_id,
            subpath: r.subpath,
            basepath: r.basedir,
            size: r.size as _,
        })
        .collect();

        conn.commit().await?;

        Ok(out)
    }

    pub async fn transfers_since(&self, since_timestamp: i64) -> Result<Vec<Transfer>> {
        let mut conn = self.conn.acquire().await?;

        let mut transfers = sqlx::query!(
            r#"
            SELECT id as "id: Hyphenated", peer, created_at, is_outgoing FROM transfers
            WHERE created_at >= datetime(?1, 'unixepoch')
            "#,
            since_timestamp
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|t| {
            let transfer_type = match t.is_outgoing {
                0 => DbTransferType::Incoming(vec![]),
                1 => DbTransferType::Outgoing(vec![]),
                _ => unreachable!(),
            };

            Transfer {
                id: t.id.into_uuid(),
                peer_id: t.peer,
                transfer_type,
                created_at: t.created_at,
                states: vec![],
            }
        })
        .collect::<Vec<_>>();

        for transfer in &mut transfers {
            match transfer.transfer_type {
                DbTransferType::Incoming(_) => {
                    transfer.transfer_type =
                        DbTransferType::Incoming(self.get_incoming_paths(transfer.id).await?)
                }
                DbTransferType::Outgoing(_) => {
                    transfer.transfer_type =
                        DbTransferType::Outgoing(self.get_outgoing_paths(transfer.id).await?)
                }
            }

            let tid = transfer.id.hyphenated();

            transfer.states.extend(
                sqlx::query!(
                    "SELECT created_at  FROM transfer_active_states WHERE transfer_id = ?1",
                    tid
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| TransferStateEvent {
                    transfer_id: transfer.id,
                    created_at: s.created_at,
                    data: types::TransferStateEventData::Active,
                }),
            );

            transfer.states.extend(
                sqlx::query!(
                    "SELECT by_peer, created_at FROM transfer_cancel_states WHERE transfer_id = ?1",
                    tid
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| TransferStateEvent {
                    transfer_id: transfer.id,
                    created_at: s.created_at,
                    data: types::TransferStateEventData::Cancel {
                        by_peer: s.by_peer != 0,
                    },
                }),
            );

            transfer.states.extend(
                sqlx::query!(
                    "SELECT status_code, created_at FROM transfer_failed_states WHERE transfer_id \
                     = ?1",
                    tid
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| TransferStateEvent {
                    transfer_id: transfer.id,
                    created_at: s.created_at,
                    data: types::TransferStateEventData::Failed {
                        status_code: s.status_code,
                    },
                }),
            );

            transfer
                .states
                .sort_by(|a, b| a.created_at.cmp(&b.created_at));
        }

        Ok(transfers)
    }

    pub async fn finished_incoming_files(&self, transfer_id: Uuid) -> Result<Vec<FinishedFile>> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        let files = sqlx::query_as!(
            FinishedFile,
            r#"
            SELECT relative_path, final_path FROM incoming_paths pp
            INNER JOIN incoming_path_completed_states cs ON pp.id = cs.path_id
            WHERE transfer_id = ?1
            "#,
            tid
        )
        .fetch_all(&mut *conn)
        .await?;

        Ok(files)
    }

    async fn get_outgoing_paths(&self, transfer_id: Uuid) -> Result<Vec<OutgoingPath>> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        let mut paths = sqlx::query!(
            r#"SELECT id as "path_id!", * FROM outgoing_paths WHERE transfer_id = ?1"#,
            tid
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|p| OutgoingPath {
            id: p.path_id,
            transfer_id,
            base_path: p.base_path,
            relative_path: p.relative_path,
            file_id: p.path_hash,
            bytes: p.bytes,
            created_at: p.created_at,
            states: vec![],
        })
        .collect::<Vec<_>>();

        for path in &mut paths {
            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM outgoing_path_pending_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| OutgoingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: OutgoingPathStateEventData::Pending,
                }),
            );

            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM outgoing_path_started_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| OutgoingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: OutgoingPathStateEventData::Started {
                        bytes_sent: s.bytes_sent,
                    },
                }),
            );

            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM outgoing_path_cancel_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| OutgoingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: OutgoingPathStateEventData::Cancel {
                        by_peer: s.by_peer != 0,
                        bytes_sent: s.bytes_sent,
                    },
                }),
            );

            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM outgoing_path_failed_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| OutgoingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: OutgoingPathStateEventData::Failed {
                        status_code: s.status_code,
                        bytes_sent: s.bytes_sent,
                    },
                }),
            );

            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM outgoing_path_completed_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| OutgoingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: OutgoingPathStateEventData::Completed,
                }),
            );

            path.states.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        }

        Ok(paths)
    }

    async fn get_incoming_paths(&self, transfer_id: Uuid) -> Result<Vec<IncomingPath>> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        let mut paths = sqlx::query!(
            r#"SELECT id as "path_id!", * FROM incoming_paths WHERE transfer_id = ?1"#,
            tid
        )
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|p| IncomingPath {
            id: p.path_id,
            transfer_id,
            relative_path: p.relative_path,
            file_id: p.path_hash,
            bytes: p.bytes,
            created_at: p.created_at,
            states: vec![],
        })
        .collect::<Vec<_>>();

        for path in &mut paths {
            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM incoming_path_pending_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| IncomingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: IncomingPathStateEventData::Pending,
                }),
            );

            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM incoming_path_started_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .into_iter()
                .map(|s| IncomingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: IncomingPathStateEventData::Started {
                        base_dir: s.base_dir,
                        bytes_received: s.bytes_received,
                    },
                }),
            );

            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM incoming_path_cancel_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| IncomingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: IncomingPathStateEventData::Cancel {
                        by_peer: s.by_peer != 0,
                        bytes_received: s.bytes_received,
                    },
                }),
            );

            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM incoming_path_failed_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .iter()
                .map(|s| IncomingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: IncomingPathStateEventData::Failed {
                        status_code: s.status_code,
                        bytes_received: s.bytes_received,
                    },
                }),
            );

            path.states.extend(
                sqlx::query!(
                    "SELECT * FROM incoming_path_completed_states WHERE path_id = ?1",
                    path.id
                )
                .fetch_all(&mut *conn)
                .await?
                .into_iter()
                .map(|s| IncomingPathStateEvent {
                    path_id: s.path_id,
                    created_at: s.created_at,
                    data: IncomingPathStateEventData::Completed {
                        final_path: s.final_path,
                    },
                }),
            );

            path.states.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        }

        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_transfer() {
        let logger = slog::Logger::root(slog::Discard, slog::o!());
        let storage = Storage::new(logger, ":memory:").await.unwrap();

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

            storage.insert_transfer(&transfer).await.unwrap();
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

            storage.insert_transfer(&transfer).await.unwrap();
        }

        {
            let transfers = storage.transfers_since(0).await.unwrap();
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
            .await
            .unwrap();

        let transfers = storage.transfers_since(0).await.unwrap();

        assert_eq!(transfers.len(), 0);
    }
}
