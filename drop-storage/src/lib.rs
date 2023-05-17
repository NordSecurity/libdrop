use std::str::FromStr;
pub mod error;
pub mod types;
use slog::Logger;
use sqlx::{pool::PoolConnection, sqlite::SqliteConnectOptions, Sqlite, SqlitePool};
use types::*;

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
        let transfer_type_int: i32 = transfer_type.into();
        let transfer_state: i32 = TransferState::Active.into();

        sqlx::query!(
            "INSERT OR IGNORE INTO peers (id) VALUES (?1)",
            transfer.peer
        )
        .execute(&mut conn)
        .await?;

        sqlx::query!(
            "INSERT OR IGNORE INTO transfers (id, peer_id, is_outgoing, state) VALUES (?1, ?2, ?3, ?4)",
            transfer.id,
            transfer.peer,
            transfer_type_int,
            transfer_state,
        )
        .execute(&mut conn)
        .await?;

        for path in transfer.files {
            self.insert_path(transfer_type, transfer.id.clone(), path, &mut conn)
                .await?;
        }

        Ok(())
    }

    async fn insert_path(
        &self,
        transfer_type: TransferType,
        transfer_id: String,
        path: TransferPath,
        conn: &mut PoolConnection<Sqlite>,
    ) -> Result<()> {
        match transfer_type {
            TransferType::Incoming => sqlx::query!(
                "INSERT INTO incoming_paths (transfer_id, path, bytes) VALUES (?1, ?2, ?3)",
                transfer_id,
                path.path,
                path.size,
            )
            .execute(conn)
            .await
            .map_err(error::Error::DBError)?,
            TransferType::Outgoing => sqlx::query!(
                "INSERT INTO outgoing_paths (transfer_id, path, bytes) VALUES (?1, ?2, ?3)",
                transfer_id,
                path.path,
                path.size,
            )
            .execute(conn)
            .await
            .map_err(error::Error::DBError)?,
        };

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
             outgoing_paths WHERE transfer_id = ?1 AND path = ?2))",
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
        file_path: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_pending_states (path_id) VALUES ((SELECT id FROM \
             incoming_paths WHERE transfer_id = ?1 AND path = ?2))",
            transfer_id,
            file_path
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_outgoing_path_started_state(
        &self,
        transfer_id: String,
        file_path: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_started_states (path_id, bytes_sent) VALUES ((SELECT id \
             FROM outgoing_paths WHERE transfer_id = ?1 AND path = ?2), ?3)",
            transfer_id,
            file_path,
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
        file_path: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_started_states (path_id, bytes_received) VALUES ((SELECT \
             id FROM incoming_paths WHERE transfer_id = ?1 AND path = ?2), ?3)",
            transfer_id,
            file_path,
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
        file_path: String,
        by_peer: bool,
        bytes_sent: i64,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_cancel_states (path_id, by_peer, bytes_sent) VALUES \
             ((SELECT id FROM outgoing_paths WHERE transfer_id = ?1 AND path = ?2), ?3, ?4)",
            transfer_id,
            file_path,
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
        file_path: String,
        by_peer: bool,
        bytes_received: i64,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_cancel_states (path_id, by_peer, bytes_received) VALUES \
             ((SELECT id FROM incoming_paths WHERE transfer_id = ?1 AND path = ?2), ?3, ?4)",
            transfer_id,
            file_path,
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
        file_path: String,
        error: u32,
        bytes_received: i64,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_failed_states (path_id, status_code, bytes_received) \
             VALUES ((SELECT id FROM incoming_paths WHERE transfer_id = ?2 AND path = ?2), ?3, ?4)",
            transfer_id,
            file_path,
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
        file_path: String,
        error: u32,
        bytes_sent: i64,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_failed_states (path_id, status_code, bytes_sent) VALUES \
             ((SELECT id FROM outgoing_paths WHERE transfer_id = ?2 AND path = ?2), ?3, ?4)",
            transfer_id,
            file_path,
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
        file_path: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO outgoing_path_completed_states (path_id) VALUES ((SELECT id FROM \
             outgoing_paths WHERE transfer_id = ?1 AND path = ?2))",
            transfer_id,
            file_path,
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn insert_incoming_path_completed_state(
        &self,
        transfer_id: String,
        file_path: String,
        final_path: String,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;

        sqlx::query!(
            "INSERT INTO incoming_path_completed_states (path_id, final_path) VALUES ((SELECT id \
             from incoming_paths WHERE transfer_id = ?1 AND path = ?2), ?3)",
            transfer_id,
            file_path,
            final_path
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn update_transfer_state(
        &self,
        transfer_id: String,
        state: TransferState,
    ) -> Result<()> {
        let mut conn = self.conn.acquire().await?;
        let state: i32 = state.into();

        sqlx::query!(
            "UPDATE transfers SET state = ?1 WHERE id = ?2",
            state,
            transfer_id
        )
        .execute(&mut conn)
        .await
        .map_err(error::Error::DBError)?;

        Ok(())
    }

    pub async fn get_transfers(&self) -> Result<Vec<Transfer>> {
        let mut conn = self.conn.acquire().await?;

        let mut transfers = sqlx::query!("SELECT * FROM transfers")
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

                let state = match t.state {
                    0 => TransferState::Active,
                    1 => TransferState::Canceled,
                    2 => TransferState::Failed,
                    _ => unreachable!(),
                };

                Transfer {
                    id: t.id.clone(),
                    peer_id: t.peer_id.clone(),
                    state,
                    transfer_type,
                    created_at: t.created_at,
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
            created_at: p.created_at,
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
                created_at: s.created_at,
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
                created_at: s.created_at,
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
                created_at: s.created_at,
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
                created_at: s.created_at,
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
                created_at: s.created_at,
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
            created_at: p.created_at,
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
                created_at: s.created_at,
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
                created_at: s.created_at,
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
                created_at: s.created_at,
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
                created_at: s.created_at,
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
                created_at: s.created_at,
            })
            .collect();
        }

        Ok(paths)
    }
}
