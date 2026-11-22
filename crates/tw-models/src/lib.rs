//! Model files: resumable, verified HTTPS downloads, the on-disk inventory
//! under `<app data>/models/{whisper,llm}/`, and what this computer can run.
//!
//! This crate is the only place the app talks to the network.

mod download;
mod hardware;
mod inventory;

pub use download::{DownloadRequest, Downloader, RequestOutcome, RequestRecord};
pub use hardware::hardware;
pub use inventory::Inventory;
pub use tw_core::models::{DownloadState, ModelProgress};
pub use tw_core::{ModelInfo, ModelKind};

#[derive(Debug, thiserror::Error)]
pub enum ModelsError {
    #[error("Downloads are turned off")]
    Offline,
    #[error("Couldn't reach the download server. Check your internet connection.")]
    Network(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("{}", http_message(*status))]
    Http { status: u16 },
    #[error("Not enough disk space: {} needed, {} free", gigabytes(*needed), gigabytes(*available))]
    NotEnoughSpace { needed: u64, available: u64 },
    #[error("The disk is full")]
    DiskFull,
    #[error("The download of {file} was damaged, so it was deleted. Try again.")]
    ChecksumMismatch { file: String },
    #[error("The server has a different {file} than expected, so it wasn't downloaded")]
    UnexpectedFile { file: String },
    #[error("Download cancelled")]
    Cancelled,
    #[error("The file is larger than expected, so it wasn't downloaded")]
    TooLarge { limit: u64 },
    #[error("This isn't a {expected} file")]
    WrongFormat { expected: &'static str },
    #[error("{file} is already in your models")]
    AlreadyInstalled { file: String },
    #[error("{file} isn't in your models")]
    NotFound { file: String },
    #[error("{0:?} isn't a valid model file name")]
    InvalidFileName(String),
    #[error(transparent)]
    Io(std::io::Error),
}

pub type Result<T, E = ModelsError> = std::result::Result<T, E>;

impl From<std::io::Error> for ModelsError {
    fn from(error: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match error.kind() {
            ErrorKind::StorageFull | ErrorKind::QuotaExceeded => Self::DiskFull,
            _ => Self::Io(error),
        }
    }
}

fn http_message(status: u16) -> String {
    match status {
        404 | 410 => format!("The model file is no longer at its address ({status})"),
        401 | 403 => format!("The download server refused the request ({status})"),
        429 => format!("The download server is busy ({status}). Try again in a few minutes."),
        500.. => format!("The download server is having trouble ({status}). Try again later."),
        _ => format!("The download server answered with an error ({status})"),
    }
}

fn gigabytes(bytes: u64) -> String {
    #[expect(clippy::cast_precision_loss, reason = "one decimal place for display")]
    let gb = bytes as f64 / 1e9;
    format!("{gb:.1} GB")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_plain() {
        assert_eq!(
            ModelsError::NotEnoughSpace {
                needed: 1_624_555_275,
                available: 300_000_000
            }
            .to_string(),
            "Not enough disk space: 1.6 GB needed, 0.3 GB free"
        );
        assert_eq!(
            ModelsError::Http { status: 404 }.to_string(),
            "The model file is no longer at its address (404)"
        );
        assert_eq!(
            ModelsError::Http { status: 503 }.to_string(),
            "The download server is having trouble (503). Try again later."
        );
    }

    #[test]
    fn full_disk_is_its_own_error() {
        let full = std::io::Error::from(std::io::ErrorKind::StorageFull);
        assert!(matches!(ModelsError::from(full), ModelsError::DiskFull));
        let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert!(matches!(ModelsError::from(other), ModelsError::Io(_)));
    }
}
