#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Storage error: {0}")]
    InternalError(String),
    #[error("DB Error: {0}")]
    DBError(#[from] rusqlite::Error),
    #[error("URI parsing: {0}")]
    UriParsing(#[from] url::ParseError),
    #[error("Invalid URI scheme: {0}")]
    InvalidUri(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
