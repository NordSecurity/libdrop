use std::str::FromStr;
pub mod error;
pub mod types;
use slog::Logger;
use sqlx::{pool::PoolConnection, sqlite::SqliteConnectOptions, Sqlite, SqlitePool};

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
        println!("insert_transfer");
        let mut conn = self.conn.acquire().await?;
        let int_type: i32 = (&transfer_type).into();

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
            int_type
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
}
