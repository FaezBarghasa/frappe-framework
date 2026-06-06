pub mod h3_server;
pub mod middleware;
pub mod routes;

#[derive(thiserror::Error, Debug)]
pub enum CaffeineError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("QUIC error: {0}")]
    Quic(String),
    #[error("Metadata error: {0}")]
    Metadata(String),
}

pub type Result<T> = std::result::Result<T, CaffeineError>;
