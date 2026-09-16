//! Error type shared by all providers.

/// Errors raised by generators. Each variant mirrors the Python exception Faker raises.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Locale has no compiled-in data.
    #[error("locale `{0}` is not available")]
    UnknownLocale(String),
    /// Template or `format()` referenced a formatter that does not exist (Python `AttributeError`).
    #[error("Unknown formatter {0:?}")]
    UnknownFormatter(String),
    /// Bad argument value (Python `ValueError`).
    #[error("{0}")]
    InvalidArgument(String),
    /// Choosing from an empty sequence (Python `IndexError`).
    #[error("Cannot choose from an empty sequence")]
    EmptySequence,
    /// Value outside the representable range (Python `OverflowError`).
    #[error("{0}")]
    Overflow(String),
    /// A date string such as `+3d` could not be parsed (Faker's `ParseError`, a `ValueError`).
    #[error("{0}")]
    Parse(String),
    /// The locale lacks data the method needs (Python `AttributeError`).
    #[error("{0}")]
    Unsupported(String),
    /// Faker raises a bare `Exception` for this.
    #[error("State Abbreviation not found in list")]
    StateNotFound,
}

/// Result alias for this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidArgument(message.into())
}
