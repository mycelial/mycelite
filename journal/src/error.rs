//! Journal Data Format Serialize/Deserialize Error

use serde::{de, ser};
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Message(String),
    IoError(std::io::Error),
    Incomplete,
    Unexpected,
    Unsupported,
    OutOfMemory(std::collections::TryReserveError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::Message(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::Message(msg.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}
