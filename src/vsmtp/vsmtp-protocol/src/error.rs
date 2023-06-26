/*
 * vSMTP mail transfer agent
 * Copyright (C) 2022 viridIT SAS
 *
 * This program is free software: you can redistribute it and/or modify it under
 * the terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program. If not, see https://www.gnu.org/licenses/.
 *
*/

// macro to generate the error kind enum and std::io::ErrorKind conversion
macro_rules! def {
    (
        $(#[$attr:meta])*
        pub enum $name:ident {
        $(
            $variant:ident
        ),*
        $(,)?
    }) => {
        $(#[$attr])*
        pub enum $name {
            $(
                #[doc = "See to [`std::io::ErrorKind`]."]
                $variant
            ),*
        }

        impl From<std::io::ErrorKind> for $name {
            #[inline]
            fn from(value: std::io::ErrorKind) -> Self {
                match value {
                    $(std::io::ErrorKind::$variant => Self::$variant,)*
                    _ => unimplemented!()
                }
            }
        }

        impl $name {
            /// Convert a [`ErrorKind`] to a [`std::io::ErrorKind`].
            #[must_use]
            #[inline]
            pub const fn to_std(self) -> std::io::ErrorKind {
                match self {
                    $(Self::$variant => std::io::ErrorKind::$variant,)*
                }
            }
        }
    };
}

// list of unstable:
// HostUnreachable,
// NetworkUnreachable,
// NetworkDown,
// NotADirectory,
// IsADirectory,
// DirectoryNotEmpty,
// ReadOnlyFilesystem,
// FilesystemLoop,
// StaleNetworkFileHandle,
// StorageFull,
// NotSeekable,
// FilesystemQuotaExceeded,
// FileTooLarge,
// ResourceBusy,
// ExecutableFileBusy,
// Deadlock,
// CrossesDevices,
// TooManyLinks,
// InvalidFilename,
// ArgumentListTooLong,

def! {
    /// Category of errors. see [`std::io::ErrorKind`]
    #[non_exhaustive]
    #[derive(
        Debug,
        Clone,
        Copy,
        strum::Display,
        strum::EnumString,
        strum::EnumIter,
        serde_with::SerializeDisplay,
        serde_with::DeserializeFromStr,
    )]
    pub enum ErrorKind {
        NotFound,
        PermissionDenied,
        ConnectionRefused,
        ConnectionReset,
        ConnectionAborted,
        NotConnected,
        AddrInUse,
        AddrNotAvailable,
        BrokenPipe,
        AlreadyExists,
        WouldBlock,
        InvalidInput,
        InvalidData,
        TimedOut,
        WriteZero,
        Interrupted,
        Unsupported,
        UnexpectedEof,
        OutOfMemory,
        Other,
    }
}

/// Helper type for [`E`] that implements [`serde::Serialize`], using [`std::fmt::Display`].
///
/// The [`serde::Deserialize`] store the value as a string.
#[derive(Debug, serde::Deserialize)]
enum Opaque<E: std::fmt::Display> {
    /// deserialize marked as skipped, but really *should* be unreachable
    #[serde(skip_deserializing)]
    Clear(E),
    Opaque(String),
}

impl<E: std::fmt::Display> std::fmt::Display for Opaque<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[allow(clippy::pattern_type_mismatch)] // false positive
        match self {
            Self::Clear(c) => write!(f, "{c}"),
            Self::Opaque(o) => write!(f, "{o}"),
        }
    }
}

impl<E: std::fmt::Display> serde::Serialize for Opaque<E> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[allow(clippy::pattern_type_mismatch)] // false positive
        match self {
            Self::Clear(e) => serializer.serialize_str(&e.to_string()),
            Self::Opaque(s) => serializer.serialize_str(s),
        }
    }
}

///
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Error {
    kind: ErrorKind,
    raw_os_error: Option<i32>,
    inner: Option<Opaque<Box<dyn std::error::Error + Send + Sync>>>,
    // Note: store description / source / backtrace too ?
}

impl std::fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "smtp protocol error: {}", self.kind)?;
        if let Some(raw) = self.raw_os_error {
            write!(f, " ({raw})")?;
        }
        if let Some(ref inner) = self.inner {
            write!(f, ": {inner}")?;
        }
        Ok(())
    }
}

impl From<std::io::Error> for Error {
    #[inline]
    fn from(value: std::io::Error) -> Self {
        Self {
            kind: value.kind().into(),
            raw_os_error: value.raw_os_error(),
            inner: value.into_inner().map(Opaque::Clear),
        }
    }
}

impl Error {
    pub(crate) fn buffer_too_long(expected: usize, got: usize) -> Self {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            ParseArgsError::BufferTooLong { expected, got },
        )
        .into()
    }

    pub(crate) fn no_crlf() -> Self {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "No CRLF found".to_owned()).into()
    }

    /// Produce an error with a timeout message.
    #[must_use]
    #[inline]
    pub fn timeout(duration: std::time::Duration, message: &str) -> Self {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("after {}: {message}", humantime::format_duration(duration)),
        )
        .into()
    }

    /// Get the kind of error.
    #[inline]
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Return the underlying error if any.
    #[inline]
    #[must_use]
    pub fn get_ref(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        #[allow(clippy::pattern_type_mismatch)] // false positive
        match &self.inner {
            Some(Opaque::Clear(e)) => Some(&**e),
            _ => None,
        }
    }

    /// Return the underlying error if any.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Option<Box<dyn std::error::Error + Send + Sync + 'static>> {
        #[allow(clippy::pattern_type_mismatch)] // false positive
        match self.inner {
            Some(Opaque::Clear(e)) => Some(e),
            _ => None,
        }
    }
}

impl From<std::str::Utf8Error> for Error {
    #[inline]
    fn from(value: std::str::Utf8Error) -> Self {
        std::io::Error::new(std::io::ErrorKind::InvalidData, value).into()
    }
}

/// Error while parsing the arguments of a command.
#[allow(clippy::module_name_repetitions)]
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ParseArgsError {
    /// Non-UTF8 buffer.
    #[error("{0}")]
    InvalidUtf8(#[from] alloc::string::FromUtf8Error),
    /// Non-UTF8 buffer.
    #[error("{0}")]
    InvalidUtf8ref(#[from] std::str::Utf8Error),
    /// Invalid IP address.
    #[error("{0}")]
    BadTypeAddr(#[from] std::net::AddrParseError),
    /// The buffer is too big (between each "\r\n").
    #[error("buffer is not supposed to be longer than {expected} bytes but got {got}")]
    BufferTooLong {
        /// buffer size limit
        expected: usize,
        /// actual size of the buffer we got
        got: usize,
    },
    /// mail address is invalid (for rcpt, mail from ...)
    #[error("")]
    InvalidMailAddress {
        /// ill-formatted mail address
        mail: String,
    },
    /// specified address it not available.
    /// In command parsing, it can be fired if a given email is in utf8
    /// and no smtputf8 option is provided
    #[error("")]
    EmailUnavailable,
    /// Other
    // FIXME: improve that
    #[error("")]
    InvalidArgs,
}
