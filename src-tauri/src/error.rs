use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    NotImplemented,
    InvalidInput,
    NotFound,
    PermissionMissing,
    /// The microphone test found no input device.
    NoMicrophone,
    /// Another app holds the microphone.
    MicrophoneBusy,
    /// Microphone access is denied in the system settings.
    PermissionDenied,
    Io,
    Internal,
}

/// The error every command returns. Serialises as `{ kind, message }`.
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct CommandError {
    pub kind: ErrorKind,
    pub message: String,
}

impl CommandError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn not_implemented(command: &str) -> Self {
        Self::new(
            ErrorKind::NotImplemented,
            format!("{command} is not implemented yet"),
        )
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }
}

pub type CommandResult<T> = Result<T, CommandError>;

impl From<std::io::Error> for CommandError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, err.to_string())
    }
}

impl From<tauri::Error> for CommandError {
    fn from(err: tauri::Error) -> Self {
        Self::new(ErrorKind::Internal, err.to_string())
    }
}

impl From<anyhow::Error> for CommandError {
    fn from(err: anyhow::Error) -> Self {
        Self::new(ErrorKind::Internal, format!("{err:#}"))
    }
}

impl From<tw_os::OsError> for CommandError {
    fn from(err: tw_os::OsError) -> Self {
        let kind = match err {
            tw_os::OsError::NotImplemented => ErrorKind::NotImplemented,
            tw_os::OsError::PermissionMissing(_) => ErrorKind::PermissionMissing,
            tw_os::OsError::Io(_) => ErrorKind::Io,
            tw_os::OsError::Unsupported | tw_os::OsError::Platform(_) => ErrorKind::Internal,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<tw_audio::AudioError> for CommandError {
    fn from(err: tw_audio::AudioError) -> Self {
        let kind = match err {
            tw_audio::AudioError::NotImplemented => ErrorKind::NotImplemented,
            tw_audio::AudioError::NoDevice => ErrorKind::NoMicrophone,
            tw_audio::AudioError::DeviceBusy => ErrorKind::MicrophoneBusy,
            tw_audio::AudioError::PermissionDenied => ErrorKind::PermissionDenied,
            tw_audio::AudioError::Io(_) => ErrorKind::Io,
            tw_audio::AudioError::Stream(_) => ErrorKind::Internal,
        };
        let message = match kind {
            ErrorKind::NoMicrophone => "No microphone found".to_owned(),
            ErrorKind::MicrophoneBusy => "Another app is using the microphone".to_owned(),
            ErrorKind::PermissionDenied => "Microphone access is turned off".to_owned(),
            _ => err.to_string(),
        };
        Self::new(kind, message)
    }
}
