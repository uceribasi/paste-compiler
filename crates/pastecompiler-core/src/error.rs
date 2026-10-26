//! Error type shared by the whole core library.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Parse(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Recipe(String),
    #[error("{0}")]
    Template(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Http(String),
    #[error("{0}")]
    Ai(String),
    #[error("{0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Parse(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e.to_string())
    }
}

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
