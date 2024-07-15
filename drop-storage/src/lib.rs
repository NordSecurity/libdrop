pub mod error;
pub mod sync;
pub mod types;

use std::{
    collections::{
        hash_map::Entry::{Occupied, Vacant},
        HashMap,
    },
    io,
    path::Path,
    vec,
};

use include_dir::{include_dir, Dir};
use rusqlite::{params, Connection, OpenFlags, Transaction};
use rusqlite_migration::Migrations;
use slog::{debug, error, trace, warn, Logger};
use tokio::sync::Mutex;
use types::{
    DbTransferType, FileSyncState, IncomingFileToRetry, IncomingPath, IncomingPathStateEvent,
    IncomingPathStateEventData, IncomingTransferToRetry, OutgoingFileToRetry, OutgoingPath,
    OutgoingPathStateEvent, OutgoingPathStateEventData, TempFileLocation, Transfer, TransferFiles,
    TransferIncomingPath, TransferOutgoingPath, TransferStateEvent, TransferType,
};
use uuid::Uuid;

use crate::error::Error;
pub use crate::types::{FileChecksum, FinishedIncomingFile, OutgoingTransferToRetry, TransferInfo};

type Result<T> = std::result::Result<T, Error>;
type QueryResult<T> = std::result::Result<T, rusqlite::Error>;

// SQLite storage wrapper
pub struct Storage {
    conn: Mutex<Connection>,
    logger: Logger,
}

const MIGRATIONS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations");

#[cfg(unix)]
fn prepare_sqlite_file(path: &str) -> io::Result<OpenFlags> {
    use std::os::unix::prelude::{OpenOptionsExt, PermissionsExt};

    const MODE: u32 = 0o600;
    let mut flags = OpenFlags::default();

    if path == ":memory:" {
        return Ok(flags);
    }
    // disalbe file creation since we are going to do it ourselves
    flags &= !OpenFlags::SQLITE_OPEN_CREATE;

    match std::fs::metadata(path) {
        Ok(meta) => {
            let mut perm = meta.permissions();
            if perm.mode() != MODE {
                perm.set_mode(MODE);
                std::fs::set_permissions(path, perm)?;
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            // create new one
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(MODE)
                .open(path)?;
        }
        Err(err) => return Err(err),
    }

    Ok(flags)
}

#[cfg(not(unix))]
fn prepare_sqlite_file(_: &str) -> io::Result<OpenFlags> {
    Ok(OpenFlags::default())
}

impl Storage {
    pub fn new(logger: Logger, path: &str) -> Result<Self> {
        let flags = prepare_sqlite_file(path)?;
        let mut conn = Connection::open_with_flags(path, flags)?;

        Migrations::from_directory(&MIGRATIONS_DIR)
            .map_err(|e| {
                Error::InternalError(format!("Failed to gather migrations from directory: {e}"))
            })?
            .to_latest(&mut conn)
            .map_err(|e| Error::InternalError(format!("Failed to run migrations: {e}")))?;

        Ok(Self {
            logger,
            conn: Mutex::new(conn),
        })
    }

    pub async fn insert_transfer(&self, transfer: &TransferInfo) -> Option<()> {
        let transfer_type_int = match &transfer.files {
            TransferFiles::Incoming(_) => TransferType::Incoming as u32,
            TransferFiles::Outgoing(_) => TransferType::Outgoing as u32,
        };

        let tid = transfer.id.to_string();
        trace!(
            self.logger,
            "Inserting transfer";
            "transfer_id" => &tid,
            "transfer_type" => transfer_type_int,
        );

        let task = async {
            let mut conn = self.conn.lock().await;
            let conn = conn.transaction()?;

            let inserted = conn.execute(
                "INSERT INTO transfers (id, peer, is_outgoing) VALUES (?1, ?2, ?3) ON CONFLICT DO \
                 NOTHING",
                params![tid, transfer.peer, transfer_type_int],
            )?;

            if inserted < 1 {
                return Ok(None);
            }

            let is_incoming = match &transfer.files {
                TransferFiles::Incoming(files) => {
                    trace!(
                        self.logger,
                        "Inserting transfer::Incoming files len {}",
                        files.len()
                    );

                    for file in files {
                        Self::insert_incoming_path(&self.logger, &conn, transfer.id, file);
                    }

                    true
                }
                TransferFiles::Outgoing(files) => {
                    trace!(
                        self.logger,
                        "Inserting transfer::Outgoing files len {}",
                        files.len()
                    );

                    for file in files {
                        Self::insert_outgoing_path(&self.logger, &conn, transfer.id, file);
                    }

                    false
                }
            };

            sync::insert_transfer(&conn, transfer.id, is_incoming)?;

            conn.commit()?;

            Ok::<_, Error>(Some(()))
        };

        match task.await {
            Err(e) => {
                error!(self.logger, "Failed to insert transfer"; "error" => %e);
                // DB error. Let's pretend the transfer was inserted
                Some(())
            }
            Ok(res) => res,
        }
    }

    pub async fn update_transfer_sync_states(&self, transfer_id: Uuid, local: sync::TransferState) {
        let task = async {
            let conn = self.conn.lock().await;
            sync::transfer_set_local_state(&conn, transfer_id, local)?;
            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to update transfer sync states"; "error" => %e);
        }
    }

    pub async fn transfer_sync_state(&self, transfer_id: Uuid) -> Option<sync::Transfer> {
        let task = async {
            let conn = self.conn.lock().await;
            sync::transfer_state(&conn, transfer_id)
        };

        match task.await {
            Ok(state) => state,
            Err(e) => {
                error!(self.logger, "Failed to get transfer sync state"; "error" => %e);
                None
            }
        }
    }

    pub async fn transfer_sync_clear(&self, transfer_id: Uuid) -> Option<()> {
        let task = async {
            let conn = self.conn.lock().await;
            sync::transfer_clear(&conn, transfer_id)
        };

        match task.await {
            Ok(state) => state,
            Err(e) => {
                error!(self.logger, "Failed to clear transfer sync state"; "error" => %e);
                None
            }
        }
    }

    pub async fn outgoing_file_sync_state(
        &self,
        transfer_id: Uuid,
        file_id: &str,
    ) -> Option<FileSyncState> {
        let tid = transfer_id.to_string();

        let task = async {
            let mut conn = self.conn.lock().await;

            let conn = conn.transaction()?;

            let sync = sync::outgoing_file_local_state(&conn, transfer_id, file_id)?;

            let sync = if let Some(sync) = sync {
                sync
            } else {
                return Ok::<_, Error>(None);
            };

            let res = conn.query_row(
                r#"
                SELECT
                    EXISTS (
                        SELECT 1
                        FROM outgoing_path_failed_states opfs
                        INNER JOIN outgoing_paths op ON op.id = opfs.path_id
                        WHERE op.transfer_id = ?1 AND op.path_hash = ?2
                        LIMIT 1
                    ) as is_failed,
                    EXISTS (
                        SELECT 1
                        FROM outgoing_path_completed_states opcs
                        INNER JOIN outgoing_paths op ON op.id = opcs.path_id
                        WHERE op.transfer_id = ?1 AND op.path_hash = ?2
                        LIMIT 1
                    ) as is_completed,
                    EXISTS (
                        SELECT 1
                        FROM outgoing_path_reject_states oprs
                        INNER JOIN outgoing_paths op ON op.id = oprs.path_id
                        WHERE op.transfer_id = ?1 AND op.path_hash = ?2
                        LIMIT 1
                    ) as is_rejected
                "#,
                params![tid, file_id],
                |r| {
                    let is_failed = r.get("is_failed")?;
                    let is_success = r.get("is_completed")?;
                    let is_rejected = r.get("is_rejected")?;

                    Ok(FileSyncState {
                        sync,
                        is_rejected,
                        is_success,
                        is_failed,
                    })
                },
            )?;

            conn.commit()?;

            Ok::<_, Error>(Some(res))
        };

        match task.await {
            Ok(state) => state,
            Err(e) => {
                error!(self.logger, "Failed to get outgoing file sync state"; "error" => %e);
                None
            }
        }
    }

    pub async fn update_outgoing_file_sync_states(
        &self,
        transfer_id: Uuid,
        file_id: &str,
        local: sync::FileState,
    ) {
        let task = async {
            let conn = self.conn.lock().await;
            sync::outgoing_file_set_local_state(&conn, transfer_id, file_id, local)?;
            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to update outgoing file sync states"; "error" => %e);
        }
    }

    pub async fn incoming_file_sync_state(
        &self,
        transfer_id: Uuid,
        file_id: &str,
    ) -> Option<FileSyncState> {
        let tid = transfer_id.to_string();

        let task = async {
            let mut conn = self.conn.lock().await;

            let conn = conn.transaction()?;

            let sync = sync::incoming_file_local_state(&conn, transfer_id, file_id)?;
            let sync = if let Some(sync) = sync {
                sync
            } else {
                return Ok::<_, Error>(None);
            };

            let res = conn.query_row(
                r#"
                SELECT
                    EXISTS (
                        SELECT 1
                        FROM incoming_path_failed_states ipfs
                        INNER JOIN incoming_paths ip ON ip.id = ipfs.path_id
                        WHERE ip.transfer_id = ?1 AND ip.path_hash = ?2
                        LIMIT 1
                    ) as is_failed,
                    EXISTS (
                        SELECT 1
                        FROM incoming_path_completed_states ipcs
                        INNER JOIN incoming_paths ip ON ip.id = ipcs.path_id
                        WHERE ip.transfer_id = ?1 AND ip.path_hash = ?2
                        LIMIT 1
                    ) as is_completed,
                    EXISTS (
                        SELECT 1
                        FROM incoming_path_reject_states iprs
                        INNER JOIN incoming_paths ip ON ip.id = iprs.path_id
                        WHERE ip.transfer_id = ?1 AND ip.path_hash = ?2
                        LIMIT 1
                    ) as is_rejected
                "#,
                params![tid, file_id],
                |r| {
                    let is_failed = r.get("is_failed")?;
                    let is_success = r.get("is_completed")?;
                    let is_rejected = r.get("is_rejected")?;

                    Ok(FileSyncState {
                        sync,
                        is_rejected,
                        is_success,
                        is_failed,
                    })
                },
            )?;

            conn.commit()?;

            Ok::<_, Error>(Some(res))
        };

        match task.await {
            Ok(state) => state,
            Err(e) => {
                error!(self.logger, "Failed to get incoming file sync state"; "error" => %e);
                None
            }
        }
    }

    pub async fn stop_incoming_file(&self, transfer_id: Uuid, file_id: &str) -> Option<()> {
        let conn = self.conn.lock().await;

        if let Err(e) = sync::incoming_file_set_local_state(
            &conn,
            transfer_id,
            file_id,
            sync::FileState::Terminal,
        ) {
            error!(self.logger, "Failed to update incoming file sync states"; "error" => %e);
        }

        match sync::stop_incoming_file(&conn, transfer_id, file_id) {
            Ok(state) => state,
            Err(e) => {
                error!(self.logger, "Failed to stop incoming file sync state"; "error" => %e);
                None
            }
        }
    }

    pub async fn start_incoming_file(&self, transfer_id: Uuid, file_id: &str, base_dir: &str) {
        let task = async {
            let conn = self.conn.lock().await;

            if sync::start_incoming_file(&conn, transfer_id, file_id, base_dir)?.is_some() {
                Self::insert_incoming_path_pending_state(&conn, transfer_id, file_id, base_dir)?;
            }

            Result::Ok(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to start incoming file sync state"; "error" => %e);
        }
    }

    fn insert_incoming_path(
        logger: &Logger,
        conn: &Transaction<'_>,
        transfer_id: Uuid,
        path: &TransferIncomingPath,
    ) {
        let tid = transfer_id.to_string();

        let task = || {
            conn.execute(
                "INSERT INTO incoming_paths (transfer_id, relative_path, path_hash, bytes)
            VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
                params![tid, path.relative_path, path.file_id, path.size],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task() {
            error!(logger, "Failed to insert incoming path"; "error" => %e);
        }
    }

    fn insert_outgoing_path(
        logger: &Logger,
        conn: &Transaction<'_>,
        transfer_id: Uuid,
        path: &TransferOutgoingPath,
    ) {
        let tid = transfer_id.to_string();
        let uri = path.uri.as_str();

        let task = || {
            conn.execute(
                r#"
            INSERT INTO outgoing_paths (transfer_id, relative_path, path_hash, bytes, uri)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
                params![tid, path.relative_path, path.file_id, path.size, uri,],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task() {
            error!(logger, "Failed to insert outgoing path"; "error" => %e);
        }
    }

    pub async fn save_checksum(&self, transfer_id: Uuid, file_id: &str, checksum: &[u8]) {
        let tid = transfer_id.to_string();

        trace!(
            self.logger,
            "Saving checksum";
            "transfer_id" => &tid,
            "file_id" => file_id,
        );

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                "UPDATE incoming_paths SET checksum = ?3 WHERE transfer_id = ?1 AND path_hash = ?2",
                params![tid, file_id, checksum],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to save checksum"; "error" => %e);
        }
    }

    pub async fn fetch_checksums(&self, transfer_id: Uuid) -> Vec<FileChecksum> {
        let tid = transfer_id.to_string();
        trace!(
            self.logger,
            "Fetching checksums";
            "transfer_id" => &tid);

        let task = async {
            let conn = self.conn.lock().await;
            let out = conn
                .prepare(
                    "SELECT path_hash as file_id, checksum FROM incoming_paths WHERE transfer_id \
                     = ?1",
                )?
                .query_map(params![tid], |row| {
                    Ok(FileChecksum {
                        file_id: row.get("file_id")?,
                        checksum: row.get("checksum")?,
                    })
                })?
                .collect::<QueryResult<Vec<_>>>()?;

            Ok::<Vec<_>, Error>(out)
        };

        match task.await {
            Ok(out) => out,
            Err(e) => {
                error!(self.logger, "Failed to fetch checksums"; "error" => %e);
                vec![]
            }
        }
    }

    pub async fn insert_transfer_failed_state(&self, transfer_id: Uuid, error: u32) {
        let tid = transfer_id.to_string();

        trace!(
            self.logger,
            "Inserting transfer failed state";
            "transfer_id" => &tid,
            "error" => error);

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                "INSERT INTO transfer_failed_states (transfer_id, status_code) VALUES (?1, ?2)",
                params![tid, error],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert transfer failed state"; "error" => %e);
        }
    }

    pub async fn insert_transfer_cancel_state(&self, transfer_id: Uuid, by_peer: bool) {
        let tid = transfer_id.to_string();

        trace!(
            self.logger,
            "Inserting transfer cancel state";
            "transfer_id" => &tid,
            "by_peer" => by_peer);

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                "INSERT INTO transfer_cancel_states (transfer_id, by_peer) VALUES (?1, ?2)",
                params![tid, by_peer],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert transfer cancel state"; "error" => %e);
        }
    }

    fn insert_incoming_path_pending_state(
        conn: &Connection,
        transfer_id: Uuid,
        path_id: &str,
        base_dir: &str,
    ) -> Result<()> {
        let tid = transfer_id.to_string();

        conn.execute(
            r#"
            INSERT INTO incoming_path_pending_states (path_id, base_dir)
            SELECT id, ?3
            FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2
            "#,
            params![tid, path_id, base_dir],
        )?;

        Ok(())
    }

    pub async fn insert_outgoing_path_started_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        bytes_sent: i64,
    ) {
        let tid = transfer_id.to_string();

        trace!(
            self.logger,
            "Inserting outgoing path started state";
            "transfer_id" => &tid,
            "path_id" => path_id);

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO outgoing_path_started_states (path_id, bytes_sent)
                SELECT id, ?3
                FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id, bytes_sent],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert outgoing path started state"; "error" => %e);
        }
    }

    pub async fn insert_incoming_path_started_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        bytes_received: i64,
    ) {
        let tid = transfer_id.to_string();

        trace!(
        self.logger,
        "Inserting incoming path started state";
        "transfer_id" => &tid,
        "path_id" => path_id,
        );

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO incoming_path_started_states (path_id, bytes_received)
                SELECT id, ?3
                FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id, bytes_received],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert incoming path started state"; "error" => %e);
        }
    }

    pub async fn insert_incoming_path_failed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        error: u32,
        bytes_received: i64,
    ) {
        let tid = transfer_id.to_string();

        trace!(
            self.logger,
            "Inserting incoming path failed state";
            "transfer_id" => &tid,
            "path_id" => path_id,
            "error" => error,
            "bytes_received" => bytes_received);

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO incoming_path_failed_states (path_id, status_code, bytes_received)
                SELECT id, ?3, ?4
                FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id, error, bytes_received],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert incoming path failed state"; "error" => %e);
        }
    }

    pub async fn insert_outgoing_path_failed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        error: u32,
        bytes_sent: i64,
    ) {
        let tid = transfer_id.to_string();
        trace!(
            self.logger,
            "Inserting outgoing path failed state";
            "transfer_id" => &tid,
            "path_id" => path_id,
            "error" => error,
            "bytes_sent" => bytes_sent);

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO outgoing_path_failed_states (path_id, status_code, bytes_sent)
                SELECT id, ?3, ?4
                FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id, error, bytes_sent],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert outgoing path failed state"; "error" => %e);
        }
    }

    pub async fn insert_outgoing_path_completed_state(&self, transfer_id: Uuid, path_id: &str) {
        let tid = transfer_id.to_string();
        trace!(
            self.logger,
            "Inserting outgoing path completed state";
            "transfer_id" => &tid,
            "path_id" => path_id);

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO outgoing_path_completed_states (path_id)
                SELECT id
                FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert outgoing path completed state"; "error" => %e);
        }
    }

    pub async fn insert_incoming_path_completed_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        final_path: &str,
    ) {
        let tid = transfer_id.to_string();
        trace!(
            self.logger,
            "Inserting incoming path completed state";
            "transfer_id" => &tid,
            "path_id" => path_id,
            "final_path" => final_path);

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO incoming_path_completed_states (path_id, final_path)
                SELECT id, ?3
                FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id, final_path],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert incoming path completed state"; "error" => %e);
        }
    }

    pub async fn insert_outgoing_path_reject_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        by_peer: bool,
        bytes_sent: i64,
    ) {
        let tid = transfer_id.to_string();

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO outgoing_path_reject_states (path_id, by_peer, bytes_sent)
                SELECT id, ?3, ?4
                FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id, by_peer, bytes_sent],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert outgoing path reject state"; "error" => %e);
        }
    }

    pub async fn insert_incoming_path_reject_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        by_peer: bool,
        bytes_received: i64,
    ) {
        let tid = transfer_id.to_string();

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO incoming_path_reject_states (path_id, by_peer, bytes_received)
                SELECT id, ?3, ?4
                FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id, by_peer, bytes_received],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert incoming path reject state"; "error" => %e);
        }
    }

    pub async fn insert_outgoing_path_paused_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        bytes_sent: i64,
    ) {
        let tid = transfer_id.to_string();

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO outgoing_path_paused_states (path_id, bytes_sent)
                SELECT id, ?3
                FROM outgoing_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id, bytes_sent],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert outgoing path paused state"; "error" => %e);
        }
    }

    pub async fn insert_incoming_path_paused_state(
        &self,
        transfer_id: Uuid,
        path_id: &str,
        bytes_received: i64,
    ) {
        let tid = transfer_id.to_string();

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                INSERT INTO incoming_path_paused_states (path_id, bytes_received)
                SELECT id, ?3
                FROM incoming_paths WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                params![tid, path_id, bytes_received],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to insert incoming path paused state"; "error" => %e);
        }
    }

    pub async fn purge_transfers_until(&self, until_timestamp: i64) {
        trace!(
            self.logger,
            "Purging transfers until timestamp";
            "until_timestamp" => until_timestamp);

        let task = async {
            let conn = self.conn.lock().await;
            conn.execute(
                r#"
                UPDATE transfers SET is_deleted = TRUE
                WHERE created_at < datetime(?1, 'unixepoch')
                    AND (
                        id IN(SELECT transfer_id FROM transfer_cancel_states) OR
                        id IN(SELECT transfer_id FROM transfer_failed_states)
                    )
                "#,
                params![until_timestamp],
            )?;

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to purge transfers"; "error" => %e);
        }
    }

    pub async fn purge_transfers(&self, transfer_ids: &[String]) {
        trace!(
            self.logger,
            "Purging transfers";
            "transfer_ids" => format!("{:?}", transfer_ids));

        let task = async {
            let conn = self.conn.lock().await;

            for id in transfer_ids {
                let count = conn.execute(
                    r#"
                    UPDATE transfers SET is_deleted = TRUE
                    WHERE id = ?1
                        AND (
                            id IN(SELECT transfer_id FROM transfer_cancel_states) OR
                            id IN(SELECT transfer_id FROM transfer_failed_states)
                        )
                    "#,
                    params![id],
                )?;

                if count < 1 {
                    warn!(
                        self.logger,
                        "Failed to purge transfer: {id}. It may not be in the terminal state"
                    );
                }
            }

            Ok::<(), Error>(())
        };

        if let Err(e) = task.await {
            error!(self.logger, "Failed to purge transfers"; "error" => %e);
        }
    }

    pub async fn outgoing_transfers_to_resume(&self) -> Vec<OutgoingTransferToRetry> {
        let task = async {
            let mut conn = self.conn.lock().await;
            let conn = conn.transaction()?;

            let rec_transfers = sync::transfers_to_resume(&conn, TransferType::Outgoing)?;

            let mut out = Vec::with_capacity(rec_transfers.len());
            for rec_transfer in rec_transfers {
                let files = conn
                    .prepare(
                        r#"
                    SELECT relative_path, uri, path_hash, bytes 
                    FROM outgoing_paths 
                    WHERE transfer_id = ?1
                    "#,
                    )?
                    .query_map(params![rec_transfer.tid], |r| {
                        Ok((
                            r.get("path_hash")?,
                            r.get::<_, String>("uri")?,
                            r.get("relative_path")?,
                            r.get("bytes")?,
                        ))
                    })?
                    .map(|row| {
                        let (file_id, uri, subpath, size) = row?;
                        Ok(OutgoingFileToRetry {
                            file_id,
                            uri: uri.parse()?,
                            subpath,
                            size,
                        })
                    })
                    .collect::<Result<_>>()?;

                out.push(OutgoingTransferToRetry {
                    uuid: rec_transfer.tid.parse().map_err(|err| {
                        crate::Error::InternalError(format!("Failed to parse UUID: {err}"))
                    })?,
                    peer: rec_transfer.peer,
                    files,
                });
            }

            conn.commit()?;

            Ok::<Vec<_>, Error>(out)
        };

        match task.await {
            Ok(transfers) => transfers,
            Err(e) => {
                error!(self.logger, "Failed to get outgoing transfers to resume"; "error" => %e);
                vec![]
            }
        }
    }

    pub async fn incoming_transfers_to_resume(&self) -> Vec<IncomingTransferToRetry> {
        let task = async {
            let mut conn = self.conn.lock().await;
            let conn = conn.transaction()?;

            let rec_transfers = sync::transfers_to_resume(&conn, TransferType::Incoming)?;

            let mut out = Vec::with_capacity(rec_transfers.len());
            for rec_transfer in rec_transfers {
                let files = conn
                    .prepare(
                        r#"
                    SELECT relative_path, path_hash, bytes 
                    FROM incoming_paths 
                    WHERE transfer_id = ?1
                    "#,
                    )?
                    .query_map(params![rec_transfer.tid], |r| {
                        Ok(IncomingFileToRetry {
                            file_id: r.get("path_hash")?,
                            subpath: r.get("relative_path")?,
                            size: r.get("bytes")?,
                        })
                    })?
                    .collect::<QueryResult<_>>()?;

                out.push(IncomingTransferToRetry {
                    uuid: rec_transfer.tid.parse().map_err(|err| {
                        crate::Error::InternalError(format!("Failed to parse UUID: {err}"))
                    })?,
                    peer: rec_transfer.peer,
                    files,
                });
            }

            conn.commit()?;
            Ok::<Vec<_>, Error>(out)
        };

        match task.await {
            Ok(transfers) => transfers,
            Err(e) => {
                error!(self.logger, "Failed to get incoming transfers to resume"; "error" => %e);
                vec![]
            }
        }
    }

    pub async fn incoming_files_to_resume(&self, transfer_id: Uuid) -> Vec<sync::FileInFlight> {
        let task = async {
            let conn = self.conn.lock().await;
            sync::incoming_files_in_flight(&conn, transfer_id)
        };

        match task.await {
            Ok(files) => files,
            Err(e) => {
                error!(self.logger, "Failed to get incoming files to resume"; "error" => %e);
                vec![]
            }
        }
    }

    pub async fn finished_incoming_files(&self, transfer_id: Uuid) -> Vec<FinishedIncomingFile> {
        let task = async {
            let conn = self.conn.lock().await;

            let paths = conn
                .prepare(
                    r#"
                SELECT relative_path as subpath, final_path
                FROM incoming_paths ip
                INNER JOIN incoming_path_completed_states ipcs ON ip.id = ipcs.path_id
                WHERE transfer_id = ?1
                "#,
                )?
                .query_map(params![transfer_id.to_string()], |r| {
                    Ok(FinishedIncomingFile {
                        subpath: r.get("subpath")?,
                        final_path: r.get("final_path")?,
                    })
                })?
                .collect::<QueryResult<_>>()?;

            Ok::<Vec<_>, Error>(paths)
        };

        match task.await {
            Ok(paths) => paths,
            Err(e) => {
                error!(self.logger, "Failed to get finished incoming files"; "error" => %e);
                vec![]
            }
        }
    }

    pub async fn transfers_since(&self, since_timestamp: i64) -> Vec<Transfer> {
        // Collect transfers since a given timestamp.
        // This performs 3 queries, fetching by insertion order:
        // 1. transfers with their states.
        // 2. outgoing paths with their states
        // 3. incoming paths with their states
        // Because a single query is used for transfers and their states
        // (the same applies to paths as well), a hashmap is used to collect states for
        // each transfer. For each state their transfer is taken from a hash map
        // (or inserted), and this transfers state list is appended.
        // For transfers, their rowid is selected as well and used to sort the
        // transfers. Because its not part of `Transfer` structure, a tuple is
        // used as hashmap value.
        trace!(
        self.logger,
        "Fetching transfers since timestamp";
        "since_timestamp" => since_timestamp);

        let task = async {
            let mut conn = self.conn.lock().await;
            let mut transfers_map: HashMap<Uuid, (u64, Transfer)> = HashMap::new();
            let tx = conn.transaction()?;
            // transfer_cancel_states.by_peer shares a type with
            // transfer_failed_states.status_code and transfer_cancel_states.
            // created_at with transfer_failed_states.created_at therefore the
            // same column can be used for them.
            let _ = tx
                .prepare(
                    r#"
                WITH ts AS  (
                    select 1, id, transfer_id, by_peer, created_at from transfer_cancel_states
                    union all
                    select 2, id, transfer_id, status_code, created_at from transfer_failed_states
                )
                select t.*, ts.*, t.rowid from transfers t
                    left join ts on ts.transfer_id = t.id
                    where not t.is_deleted and t.created_at >= datetime(?1, 'unixepoch')
                "#,
                )?
                .query_map(params![since_timestamp], |row| {
                    let id = Uuid::parse_str(row.get::<_, String>(0)?.as_str())
                        .map_err(|_| rusqlite::Error::InvalidQuery)?;
                    let rowid: u64 = row.get(10)?;
                    let transfer: &mut Transfer = &mut match transfers_map.entry(id) {
                        Occupied(e) => e.into_mut(),
                        Vacant(k) => {
                            let transfer_type = match row.get::<_, u32>(2)? {
                                0 => DbTransferType::Incoming(vec![]),
                                1 => DbTransferType::Outgoing(vec![]),
                                _ => unreachable!(),
                            };
                            let t = Transfer {
                                id,
                                peer_id: row.get(1)?,
                                transfer_type,
                                created_at: row.get(3)?,
                                states: vec![],
                            };
                            k.insert((rowid, t))
                        }
                    }
                    .1;
                    let status_type: Option<i64> = row.get(5)?;
                    match status_type {
                        Some(1) => transfer.states.push(TransferStateEvent {
                            transfer_id: transfer.id,
                            created_at: row.get(9)?,
                            data: types::TransferStateEventData::Cancel {
                                by_peer: row.get(8)?,
                            },
                        }),
                        Some(2) => transfer.states.push(TransferStateEvent {
                            transfer_id: transfer.id,
                            created_at: row.get(9)?,
                            data: types::TransferStateEventData::Failed {
                                status_code: row.get(8)?,
                            },
                        }),
                        Some(other) => warn!(
                                        self.logger,
                                        "Unexpected union member identifier for transfer state";
                                        "identifier" => other),
                        None => {
                            // This was a transfer without any states.
                        }
                    }
                    Ok(())
                })?
                .count();

            let mut outgoing_paths: HashMap<i64, OutgoingPath> = HashMap::new();
            // Here is the same situation as before - because the columns after created_at
            // are all integers, they can be shared.
            let _ = tx.prepare(r#"
            WITH ops AS (
                select 1, path_id, created_at, bytes_sent, null from outgoing_path_started_states
                union all
                select 2, path_id, created_at, status_code, bytes_sent from outgoing_path_failed_states
                union all
                select 3, path_id, created_at, null, null from outgoing_path_completed_states
                union all
                select 4, path_id, created_at, by_peer, bytes_sent from outgoing_path_reject_states
                union all
                select 5, path_id, created_at, bytes_sent, null from outgoing_path_paused_states
            )
            SELECT op.*, ops.*, op.rowid from outgoing_paths op
                left join ops on ops.path_id = op.id
                left join transfers t on t.id = op.transfer_id and not t.is_deleted and t.created_at >= datetime(?1, 'unixepoch')
                where not op.is_deleted
            "#)?.query_map(params![since_timestamp], |row| {
                let path_id: i64 = row.get(0)?;
                let path = match outgoing_paths.entry(path_id) {
                    Occupied(p) => p.into_mut(),
                    Vacant(e) => {
                        let transfer_id: String = row.get(1)?;
                        let mut res = OutgoingPath {
                            id: *e.key(),
                            transfer_id: Uuid::parse_str(&transfer_id).map_err(|_| rusqlite::Error::InvalidQuery)?,
                            content_uri: None,
                            base_path: None,
                            relative_path: row.get(2)?,
                            file_id: row.get(4)?,
                            bytes: row.get(5)?,
                            bytes_sent: 0,
                            created_at: row.get(6)?,
                            states: vec![],
                        };
                        let uri_str: String = row.get(3)?;
                        let uri = url::Url::parse(&uri_str).map_err(|_| rusqlite::Error::InvalidQuery)?; // Error handling like uuid

                        match uri.scheme() {
                            "content" => res.content_uri = Some(uri),
                            "file" => {
                                let mut path = uri.to_file_path().map_err(|_| rusqlite::Error::InvalidQuery)?; // Error handling like uuid

                                let count = Path::new(&res.relative_path).components().count();
                                for _ in 0..count {
                                    path.pop();
                                }

                                res.base_path = Some(path);
                            }
                            unknown => {
                                warn!(
                                        self.logger,
                                        "Unexpected URI scheme when decoding transfer outgoing path's base_path";
                                        "scheme" => unknown,
                                    "uri" => uri.to_string());
                                return Err(rusqlite::Error::InvalidQuery);
                            }
                        }
                        e.insert(res)
                    }
                };

                let opt_status_type: Option<i32> = row.get(8)?;
                if let Some(status_type) = opt_status_type {
                    let created_at = row.get(10)?;
                    match status_type {
                        1 => path.states.push(OutgoingPathStateEvent {
                            path_id,
                            created_at,
                            data: OutgoingPathStateEventData::Started {
                                bytes_sent: row.get(11)?,
                            },
                        }),
                        2 => path.states.push(OutgoingPathStateEvent {
                            path_id,
                            created_at,
                            data: OutgoingPathStateEventData::Failed {
                                status_code: row.get(11)?,
                                bytes_sent: row.get(12)?,
                            },
                        }),
                        3 => path.states.push(OutgoingPathStateEvent {
                            path_id,
                            created_at,
                            data: OutgoingPathStateEventData::Completed,
                        }),
                        4 => path.states.push(OutgoingPathStateEvent {
                            path_id,
                            created_at,
                            data: OutgoingPathStateEventData::Rejected {
                                by_peer: row.get(11)?,
                                bytes_sent: row.get(12)?,
                            },
                        }),
                        5 => path.states.push(OutgoingPathStateEvent {
                            path_id,
                            created_at,
                            data: OutgoingPathStateEventData::Paused {
                                bytes_sent: row.get(11)?
                            },
                        }),
                        other => warn!(
                                        self.logger,
                                        "Unexpected union member identifier for outgoing path status";
                                        "identifier" => other)
                    }
                }

                Ok(())
            })?.count();

            for (_, mut path) in outgoing_paths {
                path.states.sort_by(|a, b| a.created_at.cmp(&b.created_at));

                path.bytes_sent = path.states.last().map_or(0, |state| match state.data {
                    OutgoingPathStateEventData::Started { bytes_sent } => bytes_sent,
                    OutgoingPathStateEventData::Failed { bytes_sent, .. } => bytes_sent,
                    OutgoingPathStateEventData::Completed => path.bytes,
                    OutgoingPathStateEventData::Rejected { bytes_sent, .. } => bytes_sent,
                    OutgoingPathStateEventData::Paused { bytes_sent } => bytes_sent,
                });
                if let Some((_, t)) = transfers_map.get_mut(&path.transfer_id) {
                    if let DbTransferType::Outgoing(pp) = &mut t.transfer_type {
                        pp.push(path)
                    }
                }
            }

            let mut incoming_paths: HashMap<i64, IncomingPath> = HashMap::new();
            // And this is more interesting - base_ir and final_patch are text type. For
            // these fields a separate column will be used.
            let _ = tx.prepare(r#"
            WITH ips AS (
                select 1, path_id, created_at, null, null, base_dir from incoming_path_pending_states
                union all
                select 2, path_id, created_at, bytes_received, null, null from incoming_path_started_states
                union all
                select 3, path_id, created_at, status_code, bytes_received, null from incoming_path_failed_states
                union all
                select 4, path_id, created_at, null, null, final_path from incoming_path_completed_states
                union all
                select 5, path_id, created_at, by_peer, bytes_received, null from incoming_path_reject_states
                union all
                select 6, path_id, created_at, bytes_received, null, null from incoming_path_paused_states
            )
            SELECT ip.*, ips.* from incoming_paths ip
                left join ips on ips.path_id = ip.id
                left join transfers t on t.id = ip.transfer_id and not t.is_deleted and t.created_at >= datetime(?1, 'unixepoch')
                where not ip.is_deleted
                order by ip.rowid
            "#)?.query_map(params![since_timestamp], |row| {
                let path_id: i64 = row.get(0)?;
                let path = match incoming_paths.entry(path_id) {
                    Occupied(p) => p.into_mut(),
                    Vacant(e) => {
                        let transfer_id: String = row.get(1)?;
                        let res = IncomingPath {
                            id: *e.key(),
                            transfer_id: Uuid::parse_str(&transfer_id).map_err(|_| rusqlite::Error::InvalidQuery)?,
                            relative_path: row.get(2)?,
                            file_id: row.get(3)?,
                            bytes: row.get(4)?,
                            bytes_received: 0,
                            created_at: row.get(5)?,
                            states: vec![],
                        };
                        e.insert(res)
                    }
                };

                let opt_status_type: Option<i32> = row.get(8)?;
                if let Some(status_type) = opt_status_type {
                    let created_at = row.get(10)?;
                    match status_type {
                        1 => path.states.push(IncomingPathStateEvent {
                            path_id,
                            created_at,
                            data: IncomingPathStateEventData::Pending {
                                base_dir: row.get(13)?
                            },
                        }),
                        2 => path.states.push(IncomingPathStateEvent {
                            path_id,
                            created_at,
                            data: IncomingPathStateEventData::Started {
                                bytes_received: row.get(11)?
                            },
                        }),
                        3 => path.states.push(IncomingPathStateEvent {
                            path_id,
                            created_at,
                            data: IncomingPathStateEventData::Failed {
                                status_code: row.get(11)?,
                                bytes_received: row.get(12)?,
                            },
                        }),
                        4 => path.states.push(IncomingPathStateEvent {
                            path_id,
                            created_at,
                            data: IncomingPathStateEventData::Completed {
                                final_path: row.get(13)?
                            },
                        }),
                        5 => path.states.push(IncomingPathStateEvent {
                            path_id,
                            created_at,
                            data: IncomingPathStateEventData::Rejected {
                                by_peer: row.get(11)?,
                                bytes_received: row.get(12)?,
                            },
                        }),
                        6 => path.states.push(IncomingPathStateEvent {
                            path_id,
                            created_at,
                            data: IncomingPathStateEventData::Paused {
                                bytes_received: row.get(11)?
                            },
                        }),
                        _ => {}
                    }
                }

                Ok(())
            })?.count();

            for (_, mut path) in incoming_paths {
                path.states.sort_by(|a, b| a.created_at.cmp(&b.created_at));

                path.bytes_received = path
                    .states
                    .iter()
                    .rev()
                    .find_map(|state| match state.data {
                        IncomingPathStateEventData::Pending { .. } => None,
                        IncomingPathStateEventData::Started { bytes_received, .. } => {
                            Some(bytes_received)
                        }
                        IncomingPathStateEventData::Failed { bytes_received, .. } => {
                            Some(bytes_received)
                        }
                        IncomingPathStateEventData::Completed { .. } => Some(path.bytes),
                        IncomingPathStateEventData::Rejected { bytes_received, .. } => {
                            Some(bytes_received)
                        }
                        IncomingPathStateEventData::Paused { bytes_received } => {
                            Some(bytes_received)
                        }
                    })
                    .unwrap_or(0);

                if let Some((_, t)) = transfers_map.get_mut(&path.transfer_id) {
                    if let DbTransferType::Incoming(ip) = &mut t.transfer_type {
                        ip.push(path)
                    }
                }
            }
            drop(tx);
            drop(conn);
            let mut transfers: Vec<(u64, Transfer)> = transfers_map.into_values().collect();
            transfers.sort_by_key(|rt| rt.0);
            let mut transfers: Vec<Transfer> = transfers.into_iter().map(|rt| rt.1).collect();
            for transfer in &mut transfers {
                transfer
                    .states
                    .sort_by(|a, b| a.created_at.cmp(&b.created_at));
                match transfer.transfer_type {
                    DbTransferType::Incoming(ref mut p) => p.sort_by_key(|ip| ip.id),
                    DbTransferType::Outgoing(ref mut p) => p.sort_by_key(|op| op.id),
                };
            }
            Ok::<Vec<_>, Error>(transfers)
        };

        match task.await {
            Ok(transfers) => transfers,
            Err(e) => {
                error!(self.logger, "Failed to get transfers since timestamp"; "error" => %e);
                vec![]
            }
        }
    }

    pub async fn remove_transfer_file(&self, transfer_id: Uuid, file_id: &str) -> Option<()> {
        let tid = transfer_id.to_string();

        trace!(
            self.logger,
            "Removing transfer file";
            "transfer_id" => &tid,
            "file_id" => file_id,
        );

        let task = async {
            let conn = self.conn.lock().await;

            let mut count = 0;
            count += conn
                .prepare(
                    r#"
                UPDATE outgoing_paths
                SET is_deleted = TRUE
                WHERE transfer_id = ?1
                    AND path_hash = ?2
                    AND (
                        id IN(SELECT path_id FROM outgoing_path_reject_states) OR
                        id IN(SELECT path_id FROM outgoing_path_failed_states) OR
                        id IN(SELECT path_id FROM outgoing_path_completed_states)
                    )
            "#,
                )?
                .execute(params![tid, file_id])?;
            count += conn
                .prepare(
                    r#"
                UPDATE incoming_paths
                SET is_deleted = TRUE
                WHERE transfer_id = ?1
                    AND path_hash = ?2
                    AND (
                        id IN(SELECT path_id FROM incoming_path_reject_states) OR
                        id IN(SELECT path_id FROM incoming_path_failed_states) OR
                        id IN(SELECT path_id FROM incoming_path_completed_states)
                    )
            "#,
                )?
                .execute(params![tid, file_id])?;

            match count {
                0 => Ok::<Option<()>, Error>(None),
                1 => Ok(Some(())),
                _ => {
                    warn!(
                        self.logger,
                        "Deleted a file from both outgoing and incoming paths"
                    );
                    Ok(Some(()))
                }
            }
        };

        match task.await {
            Ok(res) => res,
            Err(e) => {
                error!(self.logger, "Failed to remove transfer file"; "error" => %e);
                None
            }
        }
    }

    pub async fn fetch_temp_locations(&self, transfer_id: Uuid) -> Vec<TempFileLocation> {
        let tid = transfer_id.to_string();

        trace!(
            self.logger,
            "Fetching temporary file locations";
            "transfer_id" => &tid
        );

        let task = async {
            let conn = self.conn.lock().await;

            let out = conn
                .prepare(
                    r#"
                SELECT DISTINCT path_hash, base_dir
                FROM incoming_paths ip
                INNER JOIN incoming_path_pending_states ipss ON ip.id = ipss.path_id 
                WHERE transfer_id = ?1
                "#,
                )?
                .query_map(params![tid], |row| {
                    Ok(TempFileLocation {
                        file_id: row.get("path_hash")?,
                        base_path: row.get("base_dir")?,
                    })
                })?
                .collect::<QueryResult<_>>()?;

            Ok::<Vec<_>, Error>(out)
        };

        match task.await {
            Ok(res) => res,
            Err(e) => {
                error!(self.logger, "Failed to fetch temporary file locations"; "error" => %e);
                vec![]
            }
        }
    }

    pub async fn fetch_base_dirs_for_file(&self, transfer_id: Uuid, file_id: &str) -> Vec<String> {
        let tid = transfer_id.to_string();

        trace!(
            self.logger,
            "Fetching temporary file locations";
            "transfer_id" => &tid
        );

        let task = async {
            let conn = self.conn.lock().await;

            let out = conn
                .prepare(
                    r#"
                SELECT DISTINCT base_dir
                FROM incoming_paths ip
                INNER JOIN incoming_path_pending_states ipss ON ip.id = ipss.path_id 
                WHERE transfer_id = ?1 AND path_hash = ?2
                "#,
                )?
                .query_map(params![tid, file_id], |row| row.get("base_dir"))?
                .collect::<QueryResult<_>>()?;

            Ok::<Vec<_>, Error>(out)
        };

        match task.await {
            Ok(res) => res,
            Err(e) => {
                error!(self.logger, "Failed to fetch temporary file locations for {file_id}"; "error" => %e);
                vec![]
            }
        }
    }

    pub async fn cleanup_garbage_transfers(&self) -> usize {
        trace!(self.logger, "Removing garbage transfers");

        let task = async {
            let conn = self.conn.lock().await;

            let count = conn.execute(
                r#"
                DELETE FROM transfers WHERE id IN (
                    SELECT t.id 
                    FROM transfers t
                    LEFT JOIN sync_transfer st ON t.id = st.transfer_id
                    WHERE t.is_deleted AND st.sync_id IS NULL                    
                )
                "#,
                params![],
            )?;

            debug!(self.logger, "Removed {count} garbage transfers");
            Result::Ok(count)
        };

        match task.await {
            Err(err) => {
                error!(self.logger, "Failed to remove garbage transfers: {err}");
                0
            }
            Ok(count) => count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_transfer() {
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

            storage.insert_transfer(&transfer).await;
        }

        {
            let transfer = TransferInfo {
                id: transfer_id_2,
                peer: "5.6.7.8".to_string(),
                files: TransferFiles::Outgoing(vec![
                    TransferOutgoingPath {
                        file_id: "id3".to_string(),
                        size: 1024,
                        uri: "file:///dir".parse().unwrap(),
                        relative_path: "3".to_string(),
                    },
                    TransferOutgoingPath {
                        file_id: "id4".to_string(),
                        relative_path: "4".to_string(),
                        uri: "file:///dir".parse().unwrap(),
                        size: 2048,
                    },
                ]),
            };

            storage.insert_transfer(&transfer).await;
        }

        {
            let transfers = storage.transfers_since(0).await;
            assert_eq!(transfers.len(), 2);

            let incoming_transfer = &transfers[0];
            let outgoing_transfer = &transfers[1];

            assert_eq!(incoming_transfer.id, transfer_id_1);
            assert_eq!(outgoing_transfer.id, transfer_id_2);

            assert_eq!(incoming_transfer.peer_id, "1.2.3.4".to_string());
            assert_eq!(outgoing_transfer.peer_id, "5.6.7.8".to_string());
        }

        storage
            .purge_transfers(&[transfer_id_1.to_string(), transfer_id_2.to_string()])
            .await;

        // Because the transfers haven't reached the terminal state
        let transfers = storage.transfers_since(0).await;
        assert_eq!(transfers.len(), 2);

        storage
            .insert_transfer_cancel_state(transfer_id_1, false)
            .await;
        storage
            .insert_transfer_failed_state(transfer_id_2, 42)
            .await;

        storage
            .purge_transfers(&[transfer_id_1.to_string(), transfer_id_2.to_string()])
            .await;

        let transfers = storage.transfers_since(0).await;
        assert_eq!(transfers.len(), 0);
    }

    #[tokio::test]
    async fn remove_outgoing_file() {
        let logger = slog::Logger::root(slog::Discard, slog::o!());
        let storage = Storage::new(logger, ":memory:").unwrap();

        let transfer_id: Uuid = "23e488a4-0521-11ee-be56-0242ac120002".parse().unwrap();

        let transfer = TransferInfo {
            id: transfer_id,
            peer: "5.6.7.8".to_string(),
            files: TransferFiles::Outgoing(vec![
                TransferOutgoingPath {
                    file_id: "id1".to_string(),
                    size: 1024,
                    uri: "file:///dir".parse().unwrap(),
                    relative_path: "1".to_string(),
                },
                TransferOutgoingPath {
                    file_id: "id2".to_string(),
                    size: 1024,
                    uri: "file:///dir".parse().unwrap(),
                    relative_path: "2".to_string(),
                },
                TransferOutgoingPath {
                    file_id: "id3".to_string(),
                    size: 1024,
                    uri: "file:///dir".parse().unwrap(),
                    relative_path: "3".to_string(),
                },
                TransferOutgoingPath {
                    file_id: "id4".to_string(),
                    relative_path: "4".to_string(),
                    uri: "file:///dir".parse().unwrap(),
                    size: 2048,
                },
            ]),
        };

        storage.insert_transfer(&transfer).await;
        storage
            .insert_outgoing_path_failed_state(transfer_id, "id1", 1, 123)
            .await;
        storage
            .insert_outgoing_path_completed_state(transfer_id, "id2")
            .await;
        storage
            .insert_outgoing_path_reject_state(transfer_id, "id3", false, 246)
            .await;

        let transfers = storage.transfers_since(0).await;
        assert_eq!(transfers.len(), 1);

        let paths = match &transfers[0].transfer_type {
            DbTransferType::Outgoing(out) => out,
            _ => panic!("Unexpected transfer type"),
        };
        assert_eq!(paths.len(), 4);

        assert!(storage
            .remove_transfer_file(transfer_id, "id1")
            .await
            .is_some());
        assert!(storage
            .remove_transfer_file(transfer_id, "id2")
            .await
            .is_some());
        assert!(storage
            .remove_transfer_file(transfer_id, "id3")
            .await
            .is_some());
        assert!(storage
            .remove_transfer_file(transfer_id, "id4")
            .await
            .is_none());

        let transfers = storage.transfers_since(0).await;
        assert_eq!(transfers.len(), 1);

        let paths = match &transfers[0].transfer_type {
            DbTransferType::Outgoing(out) => out,
            _ => panic!("Unexpected transfer type"),
        };
        assert_eq!(paths.len(), 1); // 1 since we removed one of them
        assert_eq!(paths[0].file_id, "id4");
    }

    #[tokio::test]
    async fn remove_incoming_file() {
        let logger = slog::Logger::root(slog::Discard, slog::o!());
        let storage = Storage::new(logger, ":memory:").unwrap();

        let transfer_id: Uuid = "23e488a4-0521-11ee-be56-0242ac120002".parse().unwrap();

        let transfer = TransferInfo {
            id: transfer_id,
            peer: "5.6.7.8".to_string(),
            files: TransferFiles::Incoming(vec![
                TransferIncomingPath {
                    file_id: "id1".to_string(),
                    size: 1024,
                    relative_path: "1".to_string(),
                },
                TransferIncomingPath {
                    file_id: "id2".to_string(),
                    size: 1024,
                    relative_path: "2".to_string(),
                },
                TransferIncomingPath {
                    file_id: "id3".to_string(),
                    size: 1024,
                    relative_path: "3".to_string(),
                },
                TransferIncomingPath {
                    file_id: "id4".to_string(),
                    relative_path: "4".to_string(),
                    size: 2048,
                },
            ]),
        };

        storage.insert_transfer(&transfer).await;
        storage
            .insert_incoming_path_failed_state(transfer_id, "id1", 1, 123)
            .await;
        storage
            .insert_incoming_path_completed_state(transfer_id, "id2", "/recv/id2")
            .await;
        storage
            .insert_incoming_path_reject_state(transfer_id, "id3", false, 246)
            .await;

        let transfers = storage.transfers_since(0).await;
        assert_eq!(transfers.len(), 1);

        let paths = match &transfers[0].transfer_type {
            DbTransferType::Incoming(inc) => inc,
            _ => panic!("Unexpected transfer type"),
        };
        assert_eq!(paths.len(), 4);

        assert!(storage
            .remove_transfer_file(transfer_id, "id1")
            .await
            .is_some());
        assert!(storage
            .remove_transfer_file(transfer_id, "id2")
            .await
            .is_some());
        assert!(storage
            .remove_transfer_file(transfer_id, "id3")
            .await
            .is_some());
        assert!(storage
            .remove_transfer_file(transfer_id, "id4")
            .await
            .is_none());

        let transfers = storage.transfers_since(0).await;
        assert_eq!(transfers.len(), 1);

        let paths = match &transfers[0].transfer_type {
            DbTransferType::Incoming(inc) => inc,
            _ => panic!("Unexpected transfer type"),
        };

        assert_eq!(paths.len(), 1); // 1 since we removed one of them
        assert_eq!(paths[0].file_id, "id4");
    }

    #[tokio::test]
    async fn check_storage_api() {
        let logger = slog::Logger::root(slog::Discard, slog::o!());
        let storage = Storage::new(logger, ":memory:").unwrap();

        let transfer1_id: Uuid = "23e488a4-0521-11ee-be56-0242ac120002".parse().unwrap();

        let transfer = TransferInfo {
            id: transfer1_id,
            peer: "5.6.7.8".to_string(),
            files: TransferFiles::Incoming(vec![
                TransferIncomingPath {
                    file_id: "idi1".to_string(),
                    size: 1024,
                    relative_path: "1".to_string(),
                },
                TransferIncomingPath {
                    file_id: "idi2".to_string(),
                    size: 1024,
                    relative_path: "2".to_string(),
                },
                TransferIncomingPath {
                    file_id: "idi3".to_string(),
                    size: 1024,
                    relative_path: "3".to_string(),
                },
                TransferIncomingPath {
                    file_id: "idi4".to_string(),
                    relative_path: "4".to_string(),
                    size: 2048,
                },
            ]),
        };

        storage.insert_transfer(&transfer).await;
        storage
            .insert_incoming_path_failed_state(transfer1_id, "idi1", 1, 123)
            .await;
        storage
            .start_incoming_file(transfer1_id, "idi2", "/recv/idi2")
            .await;
        storage
            .insert_incoming_path_completed_state(transfer1_id, "idi2", "/recv/idi2")
            .await;
        storage
            .insert_incoming_path_reject_state(transfer1_id, "idi3", false, 234)
            .await;
        storage
            .insert_incoming_path_started_state(transfer1_id, "idi4", 12345)
            .await;

        let transfer2_id: Uuid = "f333302e-584b-42f8-9f66-6a5ef400297d".parse().unwrap();

        let transfer = TransferInfo {
            id: transfer2_id,
            peer: "1.2.3.4".to_string(),
            files: TransferFiles::Outgoing(vec![
                TransferOutgoingPath {
                    file_id: "ido1".to_string(),
                    relative_path: "1".to_string(),
                    uri: "file:///dir/1".parse().unwrap(),
                    size: 1024,
                },
                TransferOutgoingPath {
                    file_id: "ido2".to_string(),
                    relative_path: "2".to_string(),
                    uri: "file:///dir/2".parse().unwrap(),
                    size: 1024,
                },
                TransferOutgoingPath {
                    file_id: "ido3".to_string(),
                    relative_path: "3".to_string(),
                    uri: "file:///dir/3".parse().unwrap(),
                    size: 1024,
                },
                TransferOutgoingPath {
                    file_id: "ido4".to_string(),
                    relative_path: "4".to_string(),
                    uri: "file:///dir/4".parse().unwrap(),
                    size: 2048,
                },
            ]),
        };

        storage.insert_transfer(&transfer).await;
        storage
            .insert_outgoing_path_failed_state(transfer2_id, "ido1", 1, 123)
            .await;
        storage
            .insert_outgoing_path_completed_state(transfer2_id, "ido2")
            .await;
        storage
            .insert_outgoing_path_reject_state(transfer2_id, "ido3", false, 234)
            .await;
        storage
            .insert_outgoing_path_started_state(transfer2_id, "ido4", 12345)
            .await;

        let transfers = storage.transfers_since(0).await;
        assert_eq!(transfers.len(), 2);

        assert_eq!(transfers[0].id, transfer1_id);
        assert_eq!(transfers[0].peer_id, "5.6.7.8");
        assert_eq!(transfers[0].states.len(), 0);

        match &transfers[0].transfer_type {
            DbTransferType::Incoming(inc) => {
                assert_eq!(inc[0].transfer_id, transfer1_id);
                assert_eq!(inc[0].relative_path, "1");
                assert_eq!(inc[0].bytes, 1024);
                assert_eq!(inc[0].bytes_received, 123);
                assert_eq!(inc[0].file_id, "idi1");
                assert_eq!(inc[0].states.len(), 1);

                assert!(matches!(
                    inc[0].states[0].data,
                    IncomingPathStateEventData::Failed {
                        status_code: 1,
                        bytes_received: 123
                    }
                ));

                assert_eq!(inc[1].transfer_id, transfer1_id);
                assert_eq!(inc[1].relative_path, "2");
                assert_eq!(inc[1].bytes, 1024);
                assert_eq!(inc[1].bytes_received, 1024);
                assert_eq!(inc[1].file_id, "idi2");
                assert_eq!(inc[1].states.len(), 2);

                assert!(matches!(
                    &inc[1].states[0].data,
                    IncomingPathStateEventData::Pending{
                        base_dir,
                    } if base_dir == "/recv/idi2",
                ));
                assert!(matches!(
                    &inc[1].states[1].data,
                    IncomingPathStateEventData::Completed {
                        final_path
                    } if final_path == "/recv/idi2"
                ));

                assert_eq!(inc[2].transfer_id, transfer1_id);
                assert_eq!(inc[2].relative_path, "3");
                assert_eq!(inc[2].bytes, 1024);
                assert_eq!(inc[2].bytes_received, 234);
                assert_eq!(inc[2].file_id, "idi3");
                assert_eq!(inc[2].states.len(), 1);

                assert!(matches!(
                    inc[2].states[0].data,
                    IncomingPathStateEventData::Rejected {
                        by_peer: false,
                        bytes_received: 234
                    }
                ));

                assert_eq!(inc[3].transfer_id, transfer1_id);
                assert_eq!(inc[3].relative_path, "4");
                assert_eq!(inc[3].bytes, 2048);
                assert_eq!(inc[3].bytes_received, 12345);
                assert_eq!(inc[3].file_id, "idi4");
                assert_eq!(inc[3].states.len(), 1);

                assert!(matches!(
                    &inc[3].states[0].data,
                    IncomingPathStateEventData::Started {
                        bytes_received: 12345
                    }
                ));
            }
            _ => panic!("Unexpected transfer type"),
        };

        assert_eq!(transfers[1].id, transfer2_id);
        assert_eq!(transfers[1].peer_id, "1.2.3.4");
        assert_eq!(transfers[1].states.len(), 0);

        match &transfers[1].transfer_type {
            DbTransferType::Outgoing(inc) => {
                assert_eq!(inc[0].transfer_id, transfer2_id);
                assert_eq!(inc[0].relative_path, "1");
                assert_eq!(inc[0].bytes, 1024);
                assert_eq!(inc[0].bytes_sent, 123);
                assert_eq!(inc[0].file_id, "ido1");
                assert_eq!(inc[0].base_path.as_deref(), Some(Path::new("/dir")));
                assert!(inc[0].content_uri.is_none());
                assert_eq!(inc[0].states.len(), 1);

                assert!(matches!(
                    inc[0].states[0].data,
                    OutgoingPathStateEventData::Failed {
                        status_code: 1,
                        bytes_sent: 123
                    }
                ));

                assert_eq!(inc[1].transfer_id, transfer2_id);
                assert_eq!(inc[1].relative_path, "2");
                assert_eq!(inc[1].bytes, 1024);
                assert_eq!(inc[1].bytes_sent, 1024);
                assert_eq!(inc[1].file_id, "ido2");
                assert_eq!(inc[1].base_path.as_deref(), Some(Path::new("/dir")));
                assert!(inc[1].content_uri.is_none());
                assert_eq!(inc[1].states.len(), 1);

                assert!(matches!(
                    inc[1].states[0].data,
                    OutgoingPathStateEventData::Completed
                ));

                assert_eq!(inc[2].transfer_id, transfer2_id);
                assert_eq!(inc[2].relative_path, "3");
                assert_eq!(inc[2].bytes, 1024);
                assert_eq!(inc[2].bytes_sent, 234);
                assert_eq!(inc[2].file_id, "ido3");
                assert_eq!(inc[2].base_path.as_deref(), Some(Path::new("/dir")));
                assert!(inc[2].content_uri.is_none());
                assert_eq!(inc[2].states.len(), 1);

                assert!(matches!(
                    inc[2].states[0].data,
                    OutgoingPathStateEventData::Rejected {
                        by_peer: false,
                        bytes_sent: 234
                    }
                ));

                assert_eq!(inc[3].transfer_id, transfer2_id);
                assert_eq!(inc[3].relative_path, "4");
                assert_eq!(inc[3].bytes, 2048);
                assert_eq!(inc[3].bytes_sent, 12345);
                assert_eq!(inc[3].file_id, "ido4");
                assert_eq!(inc[3].base_path.as_deref(), Some(Path::new("/dir")));
                assert!(inc[3].content_uri.is_none());
                assert_eq!(inc[3].states.len(), 1);

                assert!(matches!(
                    inc[3].states[0].data,
                    OutgoingPathStateEventData::Started { bytes_sent: 12345 }
                ));
            }
            _ => panic!("Unexpected transfer type"),
        };
    }

    #[tokio::test]
    async fn removing_garbage_transfers() {
        let logger = slog::Logger::root(slog::Discard, slog::o!());
        let storage = Storage::new(logger, ":memory:").unwrap();

        let transfer_id_1: Uuid = "23e488a4-0521-11ee-be56-0242ac120002".parse().unwrap();
        let transfer_id_2: Uuid = "23e48d7c-0521-11ee-be56-0242ac120002".parse().unwrap();

        let transfer = TransferInfo {
            id: transfer_id_1,
            peer: "1.2.3.4".to_string(),
            files: TransferFiles::Incoming(vec![]),
        };
        storage.insert_transfer(&transfer).await;

        let transfer = TransferInfo {
            id: transfer_id_2,
            peer: "5.6.7.8".to_string(),
            files: TransferFiles::Outgoing(vec![]),
        };
        storage.insert_transfer(&transfer).await;

        // Transfers need to be termiated before any purging is allowed
        storage
            .insert_transfer_cancel_state(transfer_id_1, false)
            .await;
        storage
            .insert_transfer_cancel_state(transfer_id_2, false)
            .await;

        // No garbage to collect
        let count = storage.cleanup_garbage_transfers().await;
        assert_eq!(count, 0);

        storage.purge_transfers(&[transfer_id_1.to_string()]).await;

        // Still the transfer was not synced
        let count = storage.cleanup_garbage_transfers().await;
        assert_eq!(count, 0);

        let cleared = storage.transfer_sync_clear(transfer_id_1).await;
        assert!(cleared.is_some());

        // Now the transfer can be garbage collected
        let count = storage.cleanup_garbage_transfers().await;
        assert_eq!(count, 1);

        let count = storage.cleanup_garbage_transfers().await;
        assert_eq!(count, 0);

        // Ensure we haven't deleted the second transfer
        let transfers = storage.transfers_since(0).await;
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].id, transfer_id_2);
    }
}
