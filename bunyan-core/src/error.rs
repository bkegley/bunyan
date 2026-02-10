use std::fmt;

#[derive(Debug)]
pub enum BunyanError {
    Database(rusqlite::Error),
    Serialization(serde_json::Error),
    Git(String),
    Process(String),
    NotFound(String),
    Docker(String),
}

impl fmt::Display for BunyanError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BunyanError::Database(e) => write!(f, "Database error: {}", e),
            BunyanError::Serialization(e) => write!(f, "Serialization error: {}", e),
            BunyanError::Git(msg) => write!(f, "Git error: {}", msg),
            BunyanError::Process(msg) => write!(f, "Process error: {}", msg),
            BunyanError::NotFound(msg) => write!(f, "Not found: {}", msg),
            BunyanError::Docker(msg) => write!(f, "Docker error: {}", msg),
        }
    }
}

impl std::error::Error for BunyanError {}

impl From<rusqlite::Error> for BunyanError {
    fn from(err: rusqlite::Error) -> Self {
        BunyanError::Database(err)
    }
}

impl From<serde_json::Error> for BunyanError {
    fn from(err: serde_json::Error) -> Self {
        BunyanError::Serialization(err)
    }
}

impl From<bollard::errors::Error> for BunyanError {
    fn from(err: bollard::errors::Error) -> Self {
        BunyanError::Docker(err.to_string())
    }
}

impl From<BunyanError> for String {
    fn from(err: BunyanError) -> Self {
        err.to_string()
    }
}

pub type Result<T> = std::result::Result<T, BunyanError>;
