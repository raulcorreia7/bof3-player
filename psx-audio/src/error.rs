// SPDX-License-Identifier: MIT
//! The one error type of the library.

use alloc::string::String;
use core::fmt;

/// Result alias used across the crate.
pub type Result<T> = core::result::Result<T, Error>;

/// Why an operation failed.
///
/// Every variant carries a human-readable message that names the offending
/// file, item or field. Match on the variant to decide what to do; show the
/// message to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A read or write on the underlying storage failed.
    Io(String),
    /// The bytes are not the format they claim to be, or are truncated.
    Format(String),
    /// A caller-supplied argument is out of range or names nothing.
    Invalid(String),
    /// The input is valid but uses a feature this library does not implement.
    Unsupported(String),
    /// A call was made in the wrong state (for example rendering after `stop`).
    State(String),
}

impl Error {
    /// A malformed or truncated input.
    pub fn format(message: impl Into<String>) -> Self {
        Self::Format(message.into())
    }

    /// A caller-supplied argument that is out of range or names nothing.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    /// A failed read or write on the underlying storage.
    pub fn io(message: impl Into<String>) -> Self {
        Self::Io(message.into())
    }

    /// Prefix the message with where the error happened.
    #[must_use]
    pub fn context(self, what: &str) -> Self {
        let wrap = |m: String| alloc::format!("{what}: {m}");
        match self {
            Self::Io(m) => Self::Io(wrap(m)),
            Self::Format(m) => Self::Format(wrap(m)),
            Self::Invalid(m) => Self::Invalid(wrap(m)),
            Self::Unsupported(m) => Self::Unsupported(wrap(m)),
            Self::State(m) => Self::State(wrap(m)),
        }
    }

    /// The message without the category.
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Io(m) | Self::Format(m) | Self::Invalid(m) | Self::Unsupported(m) | Self::State(m) => m,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Io(_) => "i/o error",
            Self::Format(_) => "bad format",
            Self::Invalid(_) => "invalid argument",
            Self::Unsupported(_) => "unsupported",
            Self::State(_) => "wrong state",
        };
        write!(f, "{label}: {}", self.message())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(alloc::format!("{error}"))
    }
}

impl From<psx_formats::Error> for Error {
    fn from(error: psx_formats::Error) -> Self {
        use psx_formats::Error as Source;
        match error {
            Source::VagOutOfRange { .. } | Source::SepSequenceOutOfRange { .. } => Self::Invalid(alloc::format!("{error}")),
            _ => Self::Format(alloc::format!("{error}")),
        }
    }
}
