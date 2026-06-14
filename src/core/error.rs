use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("invalid argument: {0}")]
    InvalidArgs(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("external command error: {0}")]
    External(String),

    #[error("{0}")]
    Unknown(String),
}

impl AppError {
    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::InvalidArgs(_) => crate::core::exit::EXIT_INVALID_ARGS,
            AppError::Io(_) => crate::core::exit::EXIT_IO_ERROR,
            AppError::Db(_) => crate::core::exit::EXIT_DB_ERROR,
            AppError::External(_) => crate::core::exit::EXIT_EXTERNAL,
            AppError::Unknown(_) => crate::core::exit::EXIT_UNKNOWN,
        }
    }
}

pub fn classify_exit_code(err: &anyhow::Error) -> i32 {
    for cause in err.chain() {
        if let Some(ae) = cause.downcast_ref::<AppError>() {
            return ae.exit_code();
        }
        if cause.downcast_ref::<std::io::Error>().is_some() {
            return crate::core::exit::EXIT_IO_ERROR;
        }
        if cause.downcast_ref::<rusqlite::Error>().is_some() {
            return crate::core::exit::EXIT_DB_ERROR;
        }
        if cause.downcast_ref::<clap::Error>().is_some() {
            return crate::core::exit::EXIT_INVALID_ARGS;
        }
        if cause.downcast_ref::<csv::Error>().is_some() {
            return crate::core::exit::EXIT_IO_ERROR;
        }
    }
    crate::core::exit::EXIT_UNKNOWN
}
