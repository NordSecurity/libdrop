use std::str::FromStr;
pub mod error;
use crate::error::Error;
use slog::{debug, Logger};
use sqlx::{
    migrate::MigrateDatabase, sqlite::SqliteConnectOptions, ConnectOptions, Row, Sqlite, SqlitePool,
};

type Result<T> = std::result::Result<T, Error>;
// SQLite storage wrapper
pub struct Storage {
    _logger: Logger,
    conn: SqlitePool,
}

#[derive(Debug, strum::Display, strum::EnumString, PartialEq, Eq)]
pub enum State {
    #[strum(serialize = "active")]
    Active,
    #[strum(serialize = "canceled")]
    Canceled,
    #[strum(serialize = "canceled_by_peer")]
    CanceledByPeer,
    #[strum(serialize = "failed")]
    Failed,
}

#[derive(Debug)]
pub struct Transfer {
    pub id: String,
    pub peer: String,
    pub state: State,
}

#[derive(Debug)]
pub struct OutgoingTransferPath {
    pub id: i64,
    pub outgoing_transfer: String,
    pub path: String,
}

#[derive(Debug)]
pub struct OutgoingTraversedPath {
    pub id: i64,
    pub outgoing_path: i64,
    pub path: String,
    pub size: Option<i64>,
    pub state: State,
}

#[derive(Debug)]
pub struct IncomingTransferPath {
    pub id: i64,
    pub incoming_transfer: String,
    pub path: String,
    pub size: i64,
    pub state: State,
}

#[derive(serde::Serialize, Debug)]
pub enum SerializedTransferStorage {
    Incoming {
        id: String,
        paths: Vec<String>,
    },
    Outgoing {
        id: String,
        path: Vec<(String, Vec<String>)>,
    },
}

impl Storage {
    pub async fn new(logger: Logger, path: &str) -> Result<Self> {
        if !Sqlite::database_exists(path)
            .await
            .map_err(|e| error::Error::InternalError(e.to_string()))?
        {
            debug!(logger, "SQLite not existing. Creating database at {}", path);
            Sqlite::create_database(path)
                .await
                .map_err(|e| error::Error::InternalError(e.to_string()))?;
        }

        debug!(logger, "SQLite existing: {}", path);

        let mut options = SqliteConnectOptions::from_str(path)?;
        options.log_statements(log::LevelFilter::Debug);

        let conn = SqlitePool::connect_with(options)
            .await
            .map_err(|e| error::Error::InternalError(e.to_string()))
            .map_err(|e| error::Error::InternalError(e.to_string()))?;

        sqlx::migrate!("./migrations")
            .run(&conn)
            .await
            .map_err(|e| error::Error::InternalError(e.to_string()))?;

        Ok(Self {
            _logger: logger,
            conn,
        })
    }

    pub async fn get_serialized_transfer_data(&self) -> Result<Vec<SerializedTransferStorage>> {
        let mut transfers = Vec::new();

        let incoming_transfers = self.get_incoming_transfers().await?;
        for transfer in incoming_transfers {
            let paths = self.get_incoming_transfer_paths(&transfer.id).await?;
            transfers.push(SerializedTransferStorage::Incoming {
                id: transfer.id,
                paths: paths.into_iter().map(|p| p.path).collect(),
            });
        }

        let outgoing_transfers = self.get_outgoing_transfers().await?;
        for transfer in outgoing_transfers {
            let paths = self.get_outgoing_transfer_paths(&transfer.id).await?;
            let mut path_data = Vec::new();
            for path in paths {
                let traversed_paths = self.get_outgoing_traversed_paths(path.id).await?;
                path_data.push((
                    path.path,
                    traversed_paths.into_iter().map(|p| p.path).collect(),
                ));
            }
            transfers.push(SerializedTransferStorage::Outgoing {
                id: transfer.id,
                path: path_data,
            });
        }

        Ok(transfers)
    }

    pub async fn get_outgoing_transfers(&self) -> Result<Vec<Transfer>> {
        let query = sqlx::query!("SELECT * FROM outgoing_transfers");
        let res = query.fetch_all(&self.conn).await?;

        let transfers: Vec<Transfer> = res.into_iter().try_fold(Vec::new(), |mut acc, r| {
            let transfer = Transfer {
                id: r.id,
                peer: r.peer,
                state: State::from_str(&r.state)
                    .map_err(|e| error::Error::InternalError(e.to_string()))?,
            };
            acc.push(transfer);
            Ok::<Vec<Transfer>, error::Error>(acc)
        })?;

        Ok(transfers)
    }

    pub async fn get_incoming_transfers(&self) -> Result<Vec<Transfer>> {
        let query = sqlx::query!("SELECT * FROM incoming_transfers");
        let res = query.fetch_all(&self.conn).await?;

        Ok(res.into_iter().try_fold(Vec::new(), |mut acc, r| {
            let transfer = Transfer {
                id: r.id,
                peer: r.peer,
                state: State::from_str(&r.state)
                    .map_err(|e| error::Error::InternalError(e.to_string()))?,
            };
            acc.push(transfer);
            Ok::<Vec<Transfer>, error::Error>(acc)
        })?)
    }

    pub async fn get_outgoing_transfer(&self, id: &str) -> Result<Transfer> {
        let query = sqlx::query!("SELECT * FROM outgoing_transfers WHERE id = ?", id);

        let rec = query.fetch_one(&self.conn).await.map_err(|e| {
            if matches!(e, sqlx::Error::RowNotFound) {
                Error::RowNotFound
            } else {
                Error::DBError(e)
            }
        })?;

        Ok(Transfer {
            id: rec.id,
            peer: rec.peer,
            state: State::from_str(&rec.state)
                .map_err(|e| error::Error::InternalError(e.to_string()))?,
        })
    }

    pub async fn get_outgoing_transfer_paths(
        &self,
        outgoing_transfer: &str,
    ) -> Result<Vec<OutgoingTransferPath>> {
        let query = sqlx::query!(
            "SELECT * FROM outgoing_transfer_paths WHERE outgoing_transfer = ?",
            outgoing_transfer
        );

        let res = query.fetch_all(&self.conn).await?;

        Ok(res
            .into_iter()
            .map(|r| OutgoingTransferPath {
                id: r.id,
                outgoing_transfer: r.outgoing_transfer,
                path: r.path,
            })
            .collect())
    }

    pub async fn get_outgoing_traversed_paths(
        &self,
        id: i64,
    ) -> Result<Vec<OutgoingTraversedPath>> {
        let query = sqlx::query!("SELECT * FROM outgoing_traversed_paths WHERE id = ?", id);
        let res = query.fetch_all(&self.conn).await?;

        Ok(res.into_iter().try_fold(Vec::new(), |mut acc, r| {
            let path = OutgoingTraversedPath {
                id: r.id,
                path: r.path,
                outgoing_path: r.outgoing_path,
                size: r.size,
                state: State::from_str(&r.state)
                    .map_err(|e| error::Error::InternalError(e.to_string()))?,
            };
            acc.push(path);
            Ok::<Vec<OutgoingTraversedPath>, error::Error>(acc)
        })?)
    }

    pub async fn get_incoming_transfer(&self, id: &str) -> Result<Transfer> {
        let query = sqlx::query("SELECT * FROM incoming_transfers WHERE id = ?")
            .bind(id)
            .map(|row: sqlx::sqlite::SqliteRow| {
                Ok(Transfer {
                    id: row.try_get::<String, _>("id")?,
                    peer: row.try_get::<String, _>("peer")?,
                    state: State::from_str(row.try_get::<&str, _>("state")?).map_err(|e| {
                        error::Error::InternalError(format!("failed to parse: {}", e))
                    })?,
                })
            });

        query.fetch_one(&self.conn).await.map_err(|e| {
            if matches!(e, sqlx::Error::RowNotFound) {
                Error::RowNotFound
            } else {
                Error::DBError(e)
            }
        })?
    }

    pub async fn get_incoming_transfer_paths(
        &self,
        incoming_transfer: &str,
    ) -> Result<Vec<IncomingTransferPath>> {
        let query =
            sqlx::query("SELECT * FROM incoming_transfer_paths WHERE incoming_transfer = ?")
                .bind(incoming_transfer)
                .map(
                    |row: sqlx::sqlite::SqliteRow| -> Result<IncomingTransferPath> {
                        Ok(IncomingTransferPath {
                            id: row.try_get::<i64, _>("id")?,
                            incoming_transfer: row.try_get::<String, _>("incoming_transfer")?,
                            path: row.try_get::<String, _>("path")?,
                            size: row.try_get::<i64, _>("size")?,
                            state: State::from_str(row.try_get::<&str, _>("state")?).map_err(
                                |e| error::Error::InternalError(format!("failed to parse: {}", e)),
                            )?,
                        })
                    },
                );

        let res = query.fetch_all(&self.conn).await?;

        Ok(res
            .into_iter()
            .map(|r| r.expect("failed t get incoming transfer paths"))
            .collect::<Vec<IncomingTransferPath>>())
    }

    pub async fn insert_outgoing_transfer(&mut self, id: &str, peer: &str) -> Result<Transfer> {
        let query =
            sqlx::query("INSERT INTO outgoing_transfers (id, peer, state) VALUES (?, ?, ?)")
                .bind(id)
                .bind(peer)
                .bind(State::Active.to_string());

        query.execute(&self.conn).await?;

        Ok(Transfer {
            id: id.to_string(),
            peer: peer.to_string(),
            state: State::Active,
        })
    }

    pub async fn insert_outgoing_transfer_path(
        &mut self,
        outgoing_transfer: &str,
        path: &str,
    ) -> Result<OutgoingTransferPath> {
        let query = sqlx::query(
            "INSERT INTO outgoing_transfer_paths (outgoing_transfer, path) VALUES (?, ?)",
        )
        .bind(outgoing_transfer)
        .bind(path);

        let res = query.execute(&self.conn).await?;

        Ok(OutgoingTransferPath {
            id: res.last_insert_rowid(),
            outgoing_transfer: outgoing_transfer.to_string(),
            path: path.to_string(),
        })
    }

    pub async fn insert_outgoing_traversed_paths(
        &mut self,
        id: i64,
        paths: Vec<(&str, i64)>,
    ) -> Result<()> {
        for path in paths {
            let query = sqlx::query(
                "INSERT INTO outgoing_traversed_paths (outgoing_path, path, size, state) VALUES \
                 (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(path.0)
            .bind(path.1)
            .bind(State::Active.to_string());

            query
                .execute(&self.conn)
                .await
                .expect("unable to add outgoing traversed path to storage");
        }

        Ok(())
    }

    pub async fn insert_incoming_transfer(&mut self, id: &str, peer: &str) -> Result<Transfer> {
        let query =
            sqlx::query("INSERT INTO incoming_transfers (id, peer, state) VALUES (?, ?, ?)")
                .bind(id.to_string())
                .bind(peer)
                .bind(State::Active.to_string());

        query.execute(&self.conn).await?;

        Ok(Transfer {
            id: id.to_string(),
            peer: peer.to_string(),
            state: State::Active,
        })
    }

    pub async fn insert_incoming_transfer_paths(
        &mut self,
        id: &str,
        paths: Vec<(&str, i64)>,
    ) -> Result<()> {
        // TODO: batch insert
        for path in paths {
            let query = sqlx::query(
                "INSERT INTO incoming_transfer_paths (incoming_transfer, path, size, state) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(path.0)
            .bind(path.1)
            .bind(State::Active.to_string());

            query
                .execute(&self.conn)
                .await
                .expect("unable to add incoming_transfer to storage");
        }

        Ok(())
    }
}

// unit test the function above
#[cfg(test)]
mod tests {

    use sqlx::Row;

    use super::*;

    #[tokio::test]
    async fn test_outgoing_transfers() {
        let logger = Logger::root(slog::Discard, slog::o!());
        let mut storage = Storage::new(logger, ":memory:").await.unwrap();

        storage
            .insert_outgoing_transfer("id0", "1.2.3.4")
            .await
            .unwrap();

        storage
            .insert_outgoing_transfer("id1", "2.3.4.5")
            .await
            .unwrap();

        {
            let path0 = storage
                .insert_outgoing_transfer_path("id0", "/tmp/file")
                .await
                .unwrap();

            let path1 = storage
                .insert_outgoing_transfer_path("id0", "/tmp/files")
                .await
                .unwrap();

            let paths = storage.get_outgoing_transfer_paths("id0").await.unwrap();

            assert_eq!(paths.len(), 2);

            assert_eq!(paths[0].id, path0.id);
            assert_eq!(paths[0].path, "/tmp/file");

            assert_eq!(paths[1].id, path1.id);
            assert_eq!(paths[1].path, "/tmp/files");
        }
    }

    #[tokio::test]
    async fn test_incoming_transfers() {
        // insert a transfer, then insert 2 paths to it and retrieve those
        let logger = Logger::root(slog::Discard, slog::o!());
        let mut storage = Storage::new(logger, ":memory:").await.unwrap();

        storage
            .insert_incoming_transfer("id0", "1.2.3.4")
            .await
            .unwrap();

        storage
            .insert_incoming_transfer_paths("id0", vec![("/tmp/file", 100)])
            .await
            .unwrap();

        storage
            .insert_incoming_transfer_paths(
                "id0",
                vec![
                    ("/tmp/files/file1", 100),
                    ("/tmp/files/file2", 200),
                    ("/tmp/files/file3", 300),
                ],
            )
            .await
            .unwrap();

        let paths = storage.get_incoming_transfer_paths("id0").await.unwrap();

        assert_eq!(paths.len(), 4);

        assert_eq!(paths[0].id, 1);
        assert_eq!(paths[0].path, "/tmp/file");

        assert_eq!(paths[1].id, 2);
        assert_eq!(paths[1].path, "/tmp/files/file1");

        assert_eq!(paths[2].id, 3);
        assert_eq!(paths[2].path, "/tmp/files/file2");

        assert_eq!(paths[3].id, 4);
        assert_eq!(paths[3].path, "/tmp/files/file3");
    }
}
