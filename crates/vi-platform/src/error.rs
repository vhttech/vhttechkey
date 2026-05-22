use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error("D-Bus error: {0}")]
    DBus(String),

    #[error("Wayland protocol error: {0}")]
    Wayland(String),

    #[error("X11 error: {0}")]
    X11(String),

    #[error("Connection lost")]
    ConnectionLost,

    #[error("Operation not supported by this backend")]
    NotSupported,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialize(String),

    #[error("invalid surrounding text: {0}")]
    InvalidSurroundingText(String),

    #[error("surrounding text is not available from this backend")]
    SurroundingTextUnavailable,

    #[error("cursor offset is not on a UTF-8 character boundary")]
    BadCursorOffset,
}
