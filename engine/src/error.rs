use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioLibreError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Audio stream error: {0}")]
    StreamError(String),
    #[error("Mixer error: {0}")]
    MixerError(String),
    #[error("IPC error: {0}")]
    IpcError(String),
    #[error("Platform error: {0}")]
    PlatformError(String),
}

pub type Result<T> = std::result::Result<T, AudioLibreError>;
