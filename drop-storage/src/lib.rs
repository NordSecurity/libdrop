pub mod error;
pub mod types;

use std::str::FromStr;

use slog::Logger;
use sqlx::{sqlite::SqliteConnectOptions, SqliteConnection, SqlitePool};
use types::{
    DbTransferType, IncomingPath, IncomingPathCancelState, IncomingPathCompletedState,
    IncomingPathFailedState, IncomingPathPendingState, IncomingPathStartedState, OutgoingPath,
    OutgoingPathCancelState, OutgoingPathCompletedState, OutgoingPathFailedState,
    OutgoingPathPendingState, OutgoingPathStartedState, Transfer, TransferActiveState,
    TransferCancelState, TransferFailedState, TransferFiles, TransferIncomingPath,
    TransferOutgoingPath,
};
use uuid::{fmt::Hyphenated, Uuid};

use crate::error::Error;
pub use crate::types::{Event, FileChecksum, TransferInfo, TransferType};

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

    pub async fn insert_transfer(&self, transfer: &TransferInfo) -> Result<()> {
        let transfer_type_int = match &transfer.files {
            TransferFiles::Incoming(_) => TransferType::Incoming as u32,
            TransferFiles::Outgoing(_) => TransferType::Outgoing as u32,
        };

        let mut conn = self.conn.begin().await?;

        sqlx::query!(
            "INSERT OR IGNORE INTO peers (id) VALUES (?1)",
            transfer.peer
        )
        .execute(&mut *conn)
        .await?;

        let tid = transfer.id.hyphenated();

        sqlx::query!(
            "INSERT OR IGNORE INTO transfers (id, peer_id, is_outgoing) VALUES (?1, ?2, ?3)",
            tid,
            transfer.peer,
            transfer_type_int,
        )
        .execute(&mut *conn)
        .await?;

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

        conn.commit().await.map_err(error::Error::DBError)
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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
            "INSERT INTO outgoing_path_pending_states (path_id) VALUES ((SELECT id FROM \
             outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2))",
            tid,
            file_id
        )
        .execute(&mut *conn)
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

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
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn purge_transfers_until(&self, until_timestamp: i64) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "DELETE FROM transfers WHERE created_at < datetime(?1, 'unixepoch')",
            until_timestamp
        )
        .execute(&mut *conn)
        .await
        .map_err(error::Error::DBError)
        .map(|_| ())
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
            .await
            .map_err(error::Error::DBError)
            .map(|_| ())
    }

    pub async fn purge_transfers(&self, transfer_ids: Vec<String>) -> Result<()> {
        for id in transfer_ids {
            self.purge_transfer(id).await?;
        }

        Ok(())
    }

    pub async fn transfers_since(&self, since_timestamp: i64) -> Result<Vec<Transfer>> {
        let mut conn = self.conn.acquire().await?;

        let mut transfers = sqlx::query!(
            r#"
            SELECT id as "id: Hyphenated", peer_id, created_at, is_outgoing FROM transfers
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
                peer_id: t.peer_id,
                transfer_type,
                created_at: t.created_at.timestamp_millis(),
                active_states: vec![],
                cancel_states: vec![],
                failed_states: vec![],
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

            transfer.active_states = sqlx::query!(
                "SELECT created_at  FROM transfer_active_states WHERE transfer_id = ?1",
                tid
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| TransferActiveState {
                transfer_id: transfer.id,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            transfer.cancel_states = sqlx::query!(
                "SELECT by_peer, created_at FROM transfer_cancel_states WHERE transfer_id = ?1",
                tid
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| TransferCancelState {
                transfer_id: transfer.id,
                by_peer: s.by_peer,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            transfer.failed_states = sqlx::query!(
                "SELECT status_code, created_at FROM transfer_failed_states WHERE transfer_id = ?1",
                tid
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| TransferFailedState {
                transfer_id: transfer.id,
                status_code: s.status_code,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();
        }

        Ok(transfers)
    }

    async fn get_outgoing_paths(&self, transfer_id: Uuid) -> Result<Vec<OutgoingPath>> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        let mut paths = sqlx::query!("SELECT * FROM outgoing_paths WHERE transfer_id = ?1", tid)
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .map(|p| OutgoingPath {
                id: p.id,
                transfer_id,
                base_path: p.base_path,
                relative_path: p.relative_path,
                file_id: p.path_hash,
                bytes: p.bytes,
                created_at: p.created_at.timestamp_millis(),
                pending_states: vec![],
                started_states: vec![],
                cancel_states: vec![],
                failed_states: vec![],
                completed_states: vec![],
            })
            .collect::<Vec<_>>();

        for path in &mut paths {
            path.pending_states = sqlx::query!(
                "SELECT * FROM outgoing_path_pending_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| OutgoingPathPendingState {
                path_id: s.path_id,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            path.started_states = sqlx::query!(
                "SELECT * FROM outgoing_path_started_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| OutgoingPathStartedState {
                path_id: s.path_id,
                bytes_sent: s.bytes_sent,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            path.cancel_states = sqlx::query!(
                "SELECT * FROM outgoing_path_cancel_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| OutgoingPathCancelState {
                path_id: s.path_id,
                by_peer: s.by_peer,
                bytes_sent: s.bytes_sent,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            path.failed_states = sqlx::query!(
                "SELECT * FROM outgoing_path_failed_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| OutgoingPathFailedState {
                path_id: s.path_id,
                status_code: s.status_code,
                bytes_sent: s.bytes_sent,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            path.completed_states = sqlx::query!(
                "SELECT * FROM outgoing_path_completed_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| OutgoingPathCompletedState {
                path_id: s.path_id,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();
        }

        Ok(paths)
    }

    async fn get_incoming_paths(&self, transfer_id: Uuid) -> Result<Vec<IncomingPath>> {
        let tid = transfer_id.hyphenated();

        let mut conn = self.conn.acquire().await?;

        let mut paths = sqlx::query!("SELECT * FROM incoming_paths WHERE transfer_id = ?1", tid)
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .map(|p| IncomingPath {
                id: p.id,
                transfer_id,
                relative_path: p.relative_path,
                file_id: p.path_hash,
                bytes: p.bytes,
                created_at: p.created_at.timestamp_millis(),
                pending_states: vec![],
                started_states: vec![],
                cancel_states: vec![],
                failed_states: vec![],
                completed_states: vec![],
            })
            .collect::<Vec<_>>();

        for path in &mut paths {
            path.pending_states = sqlx::query!(
                "SELECT * FROM incoming_path_pending_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| IncomingPathPendingState {
                path_id: s.path_id,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            path.started_states = sqlx::query!(
                "SELECT * FROM incoming_path_started_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| IncomingPathStartedState {
                path_id: s.path_id,
                base_dir: s.base_dir.clone(),
                bytes_received: s.bytes_received,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            path.cancel_states = sqlx::query!(
                "SELECT * FROM incoming_path_cancel_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| IncomingPathCancelState {
                path_id: s.path_id,
                by_peer: s.by_peer,
                bytes_received: s.bytes_received,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            path.failed_states = sqlx::query!(
                "SELECT * FROM incoming_path_failed_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| IncomingPathFailedState {
                path_id: s.path_id,
                status_code: s.status_code,
                bytes_received: s.bytes_received,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            path.completed_states = sqlx::query!(
                "SELECT * FROM incoming_path_completed_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut *conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| IncomingPathCompletedState {
                path_id: s.path_id,
                final_path: s.final_path.clone(),
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();
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
                        relative_path: "/dir/1".to_string(),
                        size: 1024,
                    },
                    TransferIncomingPath {
                        file_id: "id2".to_string(),
                        relative_path: "/dir/2".to_string(),
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
                        relative_path: "/dir/3".to_string(),
                        size: 1024,
                        base_path: "3".to_string(),
                    },
                    TransferOutgoingPath {
                        file_id: "id4".to_string(),
                        relative_path: "/dir/4".to_string(),
                        size: 2048,
                        base_path: "4".to_string(),
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
