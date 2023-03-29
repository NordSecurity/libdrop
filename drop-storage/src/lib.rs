use std::str::FromStr;

use anyhow::{anyhow, Result};
use slog::{debug, Logger};
use sqlx::{
    migrate::MigrateDatabase, sqlite::SqliteConnectOptions, ConnectOptions, Row, Sqlite, SqlitePool,
};

// SQLite storage wrapper
pub struct Storage {
    logger: Logger,
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
pub struct OutgoingTransfer {
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
    pub size: i64,
    pub state: State,
}

#[derive(Debug)]
pub struct IncomingTransfer {
    pub id: String,
    pub peer: String,
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

impl Storage {
    pub async fn new(logger: Logger, path: &str) -> Result<Self> {
        if !Sqlite::database_exists(path)
            .await
            .map_err(|e| anyhow!(e))?
        {
            debug!(logger, "SQLite not existing. Creating database at {}", path);
            Sqlite::create_database(path)
                .await
                .map_err(|e| anyhow!(e))?;
        }

        debug!(logger, "SQLite existing: {}", path);

        let mut options = SqliteConnectOptions::from_str(path)?;
        options.log_statements(log::LevelFilter::Debug);

        let conn = SqlitePool::connect_with(options)
            .await
            .map_err(|e| anyhow!(e))
            .map_err(|e| anyhow!(e))?;

        sqlx::migrate!("db/migrations")
            .run(&conn)
            .await
            .map_err(|e| anyhow!(e))?;

        Ok(Self { logger, conn })
    }

    pub async fn get_outgoing_transfer(&self, id: &str) -> Result<OutgoingTransfer> {
        let query = sqlx::query("SELECT * FROM outgoing_transfers WHERE id = ?")
            .bind(id)
            .map(|row: sqlx::sqlite::SqliteRow| {
                Ok(OutgoingTransfer {
                    id: row.try_get::<String, _>("id")?, // TODO: _?
                    peer: row.try_get::<String, _>("peer")?,
                    state: State::from_str(row.try_get::<&str, _>("state")?)?,
                })
            });

        // println!("query: {:?}", query);
        query.fetch_one(&self.conn).await?
    }

    pub async fn get_outgoing_transfer_paths(
        &self,
        outgoing_transfer: &str,
    ) -> Result<Vec<OutgoingTransferPath>> {
        let query =
            sqlx::query("SELECT * FROM outgoing_transfer_paths WHERE outgoing_transfer = ?")
                .bind(outgoing_transfer)
                .map(
                    |row: sqlx::sqlite::SqliteRow| -> Result<OutgoingTransferPath> {
                        Ok(OutgoingTransferPath {
                            id: row.try_get::<i64, _>("id")?,
                            outgoing_transfer: row.try_get::<String, _>("outgoing_transfer")?,
                            path: row.try_get::<String, _>("path")?,
                        })
                    },
                );

        let res = query.fetch_all(&self.conn).await?;

        Ok(res
            .into_iter()
            .map(|r| r.unwrap())
            .collect::<Vec<OutgoingTransferPath>>())
    }

    pub async fn get_outgoing_traversed_paths(
        &self,
        id: i64,
    ) -> Result<Vec<OutgoingTraversedPath>> {
        let query = sqlx::query("SELECT * FROM outgoing_traversed_paths WHERE id = ?")
            .bind(id)
            .map(
                |row: sqlx::sqlite::SqliteRow| -> Result<OutgoingTraversedPath> {
                    Ok(OutgoingTraversedPath {
                        id: row.try_get::<i64, _>("id")?,
                        outgoing_path: row.try_get::<i64, _>("outgoing_path")?,
                        path: row.try_get::<String, _>("path")?,
                        size: row.try_get::<i64, _>("size")?,
                        state: State::from_str(row.try_get::<&str, _>("state")?)?,
                    })
                },
            );

        let res = query.fetch_all(&self.conn).await?;

        Ok(res
            .into_iter()
            .map(|r| r.unwrap())
            .collect::<Vec<OutgoingTraversedPath>>())
    }

    pub async fn get_incoming_transfer(&self, id: &str) -> Result<IncomingTransfer> {
        let query = sqlx::query("SELECT * FROM incoming_transfers WHERE id = ?")
            .bind(id)
            .map(|row: sqlx::sqlite::SqliteRow| {
                Ok(IncomingTransfer {
                    id: row.try_get::<String, _>("id")?,
                    peer: row.try_get::<String, _>("peer")?,
                    state: State::from_str(row.try_get::<&str, _>("state")?)?,
                })
            });

        query.fetch_one(&self.conn).await?
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
                            state: State::from_str(row.try_get::<&str, _>("state")?)?,
                        })
                    },
                );

        let res = query.fetch_all(&self.conn).await?;

        Ok(res
            .into_iter()
            .map(|r| r.unwrap())
            .collect::<Vec<IncomingTransferPath>>())
    }

    pub async fn insert_outgoing_transfer(
        &mut self,
        id: &str,
        peer: &str,
    ) -> Result<OutgoingTransfer> {
        let query =
            sqlx::query("INSERT INTO outgoing_transfers (id, peer, state) VALUES (?, ?, ?)")
                .bind(id)
                .bind(peer)
                .bind(State::Active.to_string());

        query.execute(&self.conn).await?;

        Ok(OutgoingTransfer {
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

            query.execute(&self.conn).await.unwrap();
        }

        Ok(())
    }

    pub async fn insert_incoming_transfer(
        &mut self,
        id: &str,
        peer: &str,
    ) -> Result<IncomingTransfer> {
        let query =
            sqlx::query("INSERT INTO incoming_transfers (id, peer, state) VALUES (?, ?, ?)")
                .bind(id.to_string())
                .bind(peer)
                .bind(State::Active.to_string());

        query.execute(&self.conn).await?;

        Ok(IncomingTransfer {
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

            query.execute(&self.conn).await.unwrap();
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
