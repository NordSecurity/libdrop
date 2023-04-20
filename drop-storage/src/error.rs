#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Storage error: {0}")]
    InternalError(String),
    #[error("DB Error: {0}")]
    DBError(#[from] sqlx::Error),
    #[error("No rows found")]
    RowNotFound,
}
