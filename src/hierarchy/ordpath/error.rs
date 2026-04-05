// Vendored from ordpath 0.5.0 — https://github.com/yohdeadfall/ordpath/
// License: MIT OR Apache-2.0 — Author: Yoh Deadfall <yoh.deadfall@hotmail.com>

use std::error;
use std::fmt;
use std::io::Error as IoError;
use std::num::ParseIntError;

/// A list of possible types of errors that can cause parsing an ORDPATH to fail.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Error due to the computed capacity exceeding the maximum length of [`OrdPath`].
    ///
    /// [`OrdPath`]: crate::hierarchy::ordpath::OrdPath
    CapacityOverflow,
    /// A parameter was incorrect.
    InvalidInput,
}

impl ErrorKind {
    fn as_str(&self) -> &str {
        use ErrorKind::*;
        match *self {
            CapacityOverflow => "data capacity exceeds the ord path's maximum",
            InvalidInput => "invalid input",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str(self.as_str())
    }
}

/// The error type for operations on an ORDPATH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
}

impl Error {
    pub(crate) const fn new(kind: ErrorKind) -> Error {
        Error { kind }
    }

    /// Returns the corresponding [`ErrorKind`] for this error.
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(fmt)
    }
}

impl error::Error for Error {
    #[allow(deprecated, deprecated_in_future)]
    fn description(&self) -> &str {
        self.kind.as_str()
    }
}

impl From<ParseIntError> for Error {
    fn from(_: ParseIntError) -> Error {
        Error::new(ErrorKind::InvalidInput)
    }
}

impl From<IoError> for Error {
    fn from(_: IoError) -> Error {
        Error::new(ErrorKind::InvalidInput)
    }
}
