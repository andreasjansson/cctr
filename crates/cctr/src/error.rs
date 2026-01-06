use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to read corpus file '{path}'")]
    ReadCorpus {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse corpus file '{path}': {message}")]
    ParseCorpus { path: PathBuf, message: String },

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Walkdir error: {0}")]
    WalkDir(#[from] walkdir::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
