use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Connection pool error: {0}")]
    PoolError(String),

    #[error("Query failed: {0}")]
    QueryError(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

impl StorageError {
    pub fn is_connection_error(&self) -> bool {
        match self {
            Self::DatabaseError(e) => {
                matches!(
                    e,
                    sqlx::Error::PoolTimedOut
                        | sqlx::Error::PoolClosed
                        | sqlx::Error::Io(_)
                )
            }
            Self::PoolError(_) => true,
            _ => false,
        }
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}
