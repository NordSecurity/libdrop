use std::str::FromStr;
pub mod error;
use slog::Logger;

// use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use crate::error::Error;

type Result<T> = std::result::Result<T, Error>;
// SQLite storage wrapper
pub struct Storage {
    _logger: Logger,
    // #[allow(dead_code)]
    // conn: SqlitePool,
}

impl Storage {
    pub async fn new(logger: Logger, path: &str) -> Result<Self> {
        Ok(Self { _logger: logger })
        // let options = SqliteConnectOptions::from_str(path)?;
        // let options = options.create_if_missing(true);

        // let conn = SqlitePool::connect_with(options)
        //     .await
        //     .map_err(|e| error::Error::InternalError(e.to_string()))?;

        // sqlx::migrate!("./migrations")
        //     .run(&conn)
        //     .await
        //     .map_err(|e| error::Error::InternalError(e.to_string()))?;

        // Ok(Self {
        //     _logger: logger,
        //     conn,
        // })
    }
}
