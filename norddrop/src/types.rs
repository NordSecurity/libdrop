use std::fmt;

use slog::Level;

pub enum TransferDescriptor {
    Path {
        path: String,
    },
    Fd {
        filename: String,
        content_uri: String,
        fd: Option<i32>,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum LibdropError {
    /// Operation resulted to unknown error.
    Unknown = 1,

    /// Failed to parse C string, meaning the string provided is not valid UTF8
    /// or is a null pointer
    InvalidString = 2,

    /// One of the arguments provided is invalid
    BadInput = 3,

    /// Failed to create transfer based on arguments provided
    TransferCreate = 5,

    /// The libdrop instance is not started yet
    NotStarted = 6,

    /// Address already in use
    AddrInUse = 7,

    /// Failed to start the libdrop instance
    InstanceStart = 8,

    /// Failed to stop the libdrop instance
    InstanceStop = 9,

    /// Invalid private key provided
    InvalidPrivkey = 10,

    /// Database error
    DbError = 11,
}

impl fmt::Display for LibdropError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for LibdropError {}

#[derive(Copy, Clone)]
/// Posible log levels.
pub enum LogLevel {
    Critical = 1,
    Error = 2,
    Warning = 3,
    Info = 4,
    Debug = 5,
    Trace = 6,
}

macro_rules! map_enum {
    ($from:tt <=> $to:tt, $($f:tt = $t:tt),+ $(,)?) => {
        impl From<$from> for $to {
            fn from(f: $from) -> $to {
                match f {
                    $($from::$f => $to::$t),+
                }
            }
        }

        impl From<$to> for $from {
            fn from(t: $to) -> $from {
                match t {
                    $($to::$t => $from::$f),+
                }
            }
        }
    };
}

map_enum! {
    Level <=> LogLevel,
    Critical = Critical,
    Error = Error,
    Warning = Warning,
    Info = Info,
    Debug = Debug,
    Trace = Trace,
}
