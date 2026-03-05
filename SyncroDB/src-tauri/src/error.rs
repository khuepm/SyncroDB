use thiserror::Error;

#[derive(Error, Debug)]
pub enum SyncroDbError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Encryption error: {0}")]
    Encryption(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Invalid identifier: {0}")]
    InvalidIdentifier(String),
    
    #[error("Schema analysis error: {0}")]
    SchemaAnalysis(String),
    
    #[error("Migration error: {0}")]
    Migration(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, SyncroDbError>;

// Convert to String for Tauri commands
impl From<SyncroDbError> for String {
    fn from(err: SyncroDbError) -> String {
        err.to_string()
    }
}
