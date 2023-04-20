use thiserror::Error;


#[derive(Error, Debug)]
pub enum DbError {
    #[error("failed to dump database into file: {0}")]
    DumpError(String),

    #[error("key not found")]
    NotFound,

    #[error("failed DB operation:")]
    GenericError(String),
}

pub type Result<T> = std::result::Result<T, DbError>;
