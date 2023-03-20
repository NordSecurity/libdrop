use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(untagged)]
pub enum Error {
    #[error("Invalid address")]
    BadAddr,
    #[error("Generic error")]
    Generic,
    #[error("Error parsing JSON string")]
    JsonParse,
    #[error("Could not create transfer")]
    TransferCreate,
    #[error("Moose event path cannot be empty")]
    EmptyEventPath,
    #[error("Instance not started")]
    InstanceNotStarted,
    #[error("Address already in use")]
    AddrInUse,
}

impl From<&Error> for i32 {
    fn from(err: &Error) -> Self {
        match err {
            Error::BadAddr => 0,
            Error::Generic => 1,
            Error::JsonParse => 3,
            Error::TransferCreate => 5,
            Error::EmptyEventPath => 6,
            Error::InstanceNotStarted => 7,
            Error::AddrInUse => 8,
        }
    }
}
