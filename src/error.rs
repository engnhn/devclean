use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum DevcleanError {
    Io { path: PathBuf, error: io::Error },
    Message(String),
}

impl DevcleanError {
    pub fn io(path: impl Into<PathBuf>, error: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            error,
        }
    }

    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}

impl fmt::Display for DevcleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "could not process {}: {error}", path.display()),
            Self::Message(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DevcleanError {}

pub type Result<T> = std::result::Result<T, DevcleanError>;
