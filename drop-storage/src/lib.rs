use std::str::FromStr;
pub mod error;
pub mod types;
use slog::Logger;
use sqlx::{sqlite::SqliteConnectOptions, Connection, Sqlite, SqlitePool, Transaction};
use types::{
    DbTransferType, IncomingPath, IncomingPathCancelState, IncomingPathCompletedState,
    IncomingPathFailedState, IncomingPathPendingState, IncomingPathStartedState, OutgoingPath,
    OutgoingPathCancelState, OutgoingPathCompletedState, OutgoingPathFailedState,
    OutgoingPathPendingState, OutgoingPathStartedState, Transfer, TransferActiveState,
    TransferCancelState, TransferFailedState,
};

use crate::error::Error;
pub use crate::types::{TransferInfo, TransferPath, TransferType};

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
            .run(&conn)
            .await
            .map_err(|e| error::Error::InternalError(e.to_string()))?;

        Ok(Self {
            _logger: logger,
            conn,
        })
    }

    pub async fn insert_transfer(
        &self,
        transfer: TransferInfo,
        transfer_type: TransferType,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;
        let mut conn = conn.begin().await?;
        let transfer_type_int = transfer_type as u32;

        sqlx::query!(
            "INSERT OR IGNORE INTO peers (id) VALUES (?1)",
            transfer.peer
        )
        .execute(&mut conn)
        .await?;

        sqlx::query!(
            "INSERT OR IGNORE INTO transfers (id, peer_id, is_outgoing) VALUES (?1, ?2, ?3)",
            transfer.id,
            transfer.peer,
            transfer_type_int,
        )
        .execute(&mut conn)
        .await?;

        for path in transfer.files {
            Self::insert_path(transfer_type, transfer.id.clone(), path, &mut conn).await?;
        }

        conn.commit().await.map_err(error::Error::DBError)
    }

    async fn insert_path(
        transfer_type: TransferType,
        transfer_id: String,
        path: TransferPath,
        conn: &mut Transaction<'_, Sqlite>,
    ) -> Result<()> {
        match transfer_type {
            TransferType::Incoming => sqlx::query!(
                "INSERT INTO incoming_paths (transfer_id, path, path_hash, bytes) VALUES (?1, ?2, \
                 ?3, ?4)",
                transfer_id,
                path.path,
                path.id,
                path.size,
            )
            .execute(conn)
            .await
            .map_err(error::Error::DBError)?,
            TransferType::Outgoing => sqlx::query!(
                "INSERT INTO outgoing_paths (transfer_id, path, path_hash, bytes) VALUES (?1, ?2, \
                 ?3, ?4)",
                transfer_id,
                path.path,
                path.id,
                path.size,
            )
            .execute(conn)
            .await
            .map_err(error::Error::DBError)?,
        };

        Ok(())
    }

    pub async fn insert_transfer_active_state(&self, transfer_id: String) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO transfer_active_states (transfer_id) VALUES (?1)",
            transfer_id,
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_transfer_failed_state(
        &self,
        transfer_id: String,
        error: u32,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO transfer_failed_states (transfer_id, status_code) VALUES (?1, ?2)",
            transfer_id,
            error,
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_transfer_cancel_state(
        &self,
        transfer_id: String,
        by_peer: bool,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO transfer_cancel_states (transfer_id, by_peer) VALUES (?1, ?2)",
            transfer_id,
            by_peer,
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_outgoing_path_pending_state(
        &self,
        transfer_id: String,
        file_path: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_pending_states (path_id) VALUES ((SELECT id FROM \
             outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2))",
            transfer_id,
            file_path
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_incoming_path_pending_state(
        &self,
        transfer_id: String,
        path_id: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_pending_states (path_id) VALUES ((SELECT id FROM \
             incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2))",
            transfer_id,
            path_id,
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_outgoing_path_started_state(
        &self,
        transfer_id: String,
        path_id: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_started_states (path_id, bytes_sent) VALUES ((SELECT id \
             FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3)",
            transfer_id,
            path_id,
            0
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_incoming_path_started_state(
        &self,
        transfer_id: String,
        path_id: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_started_states (path_id, bytes_received) VALUES ((SELECT \
             id FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3)",
            transfer_id,
            path_id,
            0
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_outgoing_path_cancel_state(
        &self,
        transfer_id: String,
        path_id: String,
        by_peer: bool,
        bytes_sent: i64,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_cancel_states (path_id, by_peer, bytes_sent) VALUES \
             ((SELECT id FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            transfer_id,
            path_id,
            by_peer,
            bytes_sent
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_incoming_path_cancel_state(
        &self,
        transfer_id: String,
        path_id: String,
        by_peer: bool,
        bytes_received: i64,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_cancel_states (path_id, by_peer, bytes_received) VALUES \
             ((SELECT id FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3, ?4)",
            transfer_id,
            path_id,
            by_peer,
            bytes_received
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_incoming_path_failed_state(
        &self,
        transfer_id: String,
        path_id: String,
        error: u32,
        bytes_received: i64,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_failed_states (path_id, status_code, bytes_received) \
             VALUES ((SELECT id FROM incoming_paths WHERE transfer_id = ?2 AND path_hash = ?2), \
             ?3, ?4)",
            transfer_id,
            path_id,
            error,
            bytes_received
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_outgoing_path_failed_state(
        &self,
        transfer_id: String,
        path_id: String,
        error: u32,
        bytes_sent: i64,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_failed_states (path_id, status_code, bytes_sent) VALUES \
             ((SELECT id FROM outgoing_paths WHERE transfer_id = ?2 AND path_hash = ?2), ?3, ?4)",
            transfer_id,
            path_id,
            error,
            bytes_sent
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_outgoing_path_completed_state(
        &self,
        transfer_id: String,
        path_id: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_completed_states (path_id) VALUES ((SELECT id FROM \
             outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2))",
            transfer_id,
            path_id,
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_incoming_path_completed_state(
        &self,
        transfer_id: String,
        path_id: String,
        final_path: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_completed_states (path_id, final_path) VALUES ((SELECT id \
             from incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2), ?3)",
            transfer_id,
            path_id,
            final_path
        )
        .execute(&mut conn)
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
        .execute(&mut conn)
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
            .execute(&mut conn)
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

    pub async fn get_transfers(&self, since_timestamp: i64) -> Result<Vec<Transfer>> {
        let mut conn = self.conn.acquire().await?;

        let mut transfers = sqlx::query!(
            "SELECT * FROM transfers WHERE created_at >= datetime(?1, 'unixepoch')",
            since_timestamp
        )
        .fetch_all(&mut conn)
        .await
        .map_err(error::Error::DBError)?
        .iter()
        .map(|t| {
            let transfer_type = match t.is_outgoing {
                0 => DbTransferType::Incoming(vec![]),
                1 => DbTransferType::Outgoing(vec![]),
                _ => unreachable!(),
            };

            Transfer {
                id: t.id.clone(),
                peer_id: t.peer_id.clone(),
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
                    transfer.transfer_type = DbTransferType::Incoming(
                        self.get_incoming_paths(transfer.id.clone()).await?,
                    )
                }
                DbTransferType::Outgoing(_) => {
                    transfer.transfer_type = DbTransferType::Outgoing(
                        self.get_outgoing_paths(transfer.id.clone()).await?,
                    )
                }
            }

            transfer.active_states = sqlx::query!(
                "SELECT * FROM transfer_active_states WHERE transfer_id = ?1",
                transfer.id
            )
            .fetch_all(&mut conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| TransferActiveState {
                transfer_id: s.transfer_id.clone(),
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            transfer.cancel_states = sqlx::query!(
                "SELECT * FROM transfer_cancel_states WHERE transfer_id = ?1",
                transfer.id
            )
            .fetch_all(&mut conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| TransferCancelState {
                transfer_id: s.transfer_id.clone(),
                by_peer: s.by_peer,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            transfer.failed_states = sqlx::query!(
                "SELECT * FROM transfer_failed_states WHERE transfer_id = ?1",
                transfer.id
            )
            .fetch_all(&mut conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| TransferFailedState {
                transfer_id: s.transfer_id.clone(),
                status_code: s.status_code,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();
        }

        Ok(transfers)
    }

    async fn get_outgoing_paths(&self, transfer_id: String) -> Result<Vec<OutgoingPath>> {
        let mut conn = self.conn.acquire().await?;

        let mut paths = sqlx::query!(
            "SELECT * FROM outgoing_paths WHERE transfer_id = ?1",
            transfer_id
        )
        .fetch_all(&mut conn)
        .await
        .map_err(error::Error::DBError)?
        .iter()
        .map(|p| OutgoingPath {
            id: p.id,
            transfer_id: p.transfer_id.clone(),
            path: p.path.clone(),
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
            .fetch_all(&mut conn)
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
            .fetch_all(&mut conn)
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
            .fetch_all(&mut conn)
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
            .fetch_all(&mut conn)
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
            .fetch_all(&mut conn)
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

    async fn get_incoming_paths(&self, transfer_id: String) -> Result<Vec<IncomingPath>> {
        let mut conn = self.conn.acquire().await?;

        let mut paths = sqlx::query!(
            "SELECT * FROM incoming_paths WHERE transfer_id = ?1",
            transfer_id
        )
        .fetch_all(&mut conn)
        .await
        .map_err(error::Error::DBError)?
        .iter()
        .map(|p| IncomingPath {
            id: p.id,
            transfer_id: p.transfer_id.clone(),
            path: p.path.clone(),
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
            .fetch_all(&mut conn)
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
            .fetch_all(&mut conn)
            .await
            .map_err(error::Error::DBError)?
            .iter()
            .map(|s| IncomingPathStartedState {
                path_id: s.path_id,
                bytes_received: s.bytes_received,
                created_at: s.created_at.timestamp_millis(),
            })
            .collect();

            path.cancel_states = sqlx::query!(
                "SELECT * FROM incoming_path_cancel_states WHERE path_id = ?1",
                path.id
            )
            .fetch_all(&mut conn)
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
            .fetch_all(&mut conn)
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
            .fetch_all(&mut conn)
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
        // let storage = Storage::new(logger, "/tmp/mem.sqlite").await.unwrap();

        {
            let transfer = TransferInfo {
                id: "transfer_id_1".to_string(),
                peer: "1.2.3.4".to_string(),
                files: vec![
                    TransferPath {
                        id: "id1".to_string(),
                        path: "/dir/1".to_string(),
                        size: 1024,
                    },
                    TransferPath {
                        id: "id2".to_string(),
                        path: "/dir/2".to_string(),
                        size: 2048,
                    },
                ],
            };

            storage
                .insert_transfer(transfer, TransferType::Incoming)
                .await
                .unwrap();
        }

        {
            let transfer = TransferInfo {
                id: "transfer_id_2".to_string(),
                peer: "5.6.7.8".to_string(),
                files: vec![
                    TransferPath {
                        id: "id3".to_string(),
                        path: "/dir/3".to_string(),
                        size: 1024,
                    },
                    TransferPath {
                        id: "id4".to_string(),
                        path: "/dir/4".to_string(),
                        size: 2048,
                    },
                ],
            };

            storage
                .insert_transfer(transfer, TransferType::Outgoing)
                .await
                .unwrap();
        }

        {
            let transfers = storage.get_transfers(0).await.unwrap();
            assert_eq!(transfers.len(), 2);

            let incoming_transfer = &transfers[0];
            let outgoing_transfer = &transfers[1];

            assert_eq!(incoming_transfer.id, "transfer_id_1");
            assert_eq!(outgoing_transfer.id, "transfer_id_2");

            assert_eq!(incoming_transfer.peer_id, "1.2.3.4".to_string());
            assert_eq!(outgoing_transfer.peer_id, "5.6.7.8".to_string());
        }

        storage
            .purge_transfers(vec![
                "transfer_id_1".to_string(),
                "transfer_id_2".to_string(),
            ])
            .await
            .unwrap();

        let transfers = storage.get_transfers(0).await.unwrap();

        println!("transfers: {:?}", transfers);
        assert_eq!(transfers.len(), 0);
    }
}
