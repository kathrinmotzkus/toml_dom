//! Error types for the TOML 1.1 library.
//!
//! All fallible operations in this crate return [`TomlError`].
//! The [`TomlErrorKind`] enum distinguishes parse errors, type mismatches,
//! missing keys, and I/O problems, while [`SourceLocation`] carries the
//! line/column (and optional file name) of where the problem occurred.

use std::fmt;

/// The position inside a TOML source where an error was detected.
///
/// Both `line` and `column` are 1-based.
/// `source_file` is `None` when parsing from an in-memory string.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
    /// Optional file path that was being parsed when the error occurred.
    pub source_file: Option<String>,
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref file) = self.source_file {
            write!(f, "{}:{}:{}", file, self.line, self.column)
        } else {
            write!(f, "{}:{}", self.line, self.column)
        }
    }
}

/// Discriminant that classifies every error this library can produce.
///
/// Returned inside [`TomlError::kind`]; match on it when you need to handle
/// specific failure modes programmatically.
#[derive(Debug, Clone)]
pub enum TomlErrorKind {
    /// A syntax rule of the TOML 1.1 grammar was violated during parsing.
    ParseError,
    /// The same key was defined more than once, or a table header conflicts
    /// with a previously defined key.
    DuplicateKey,
    /// The value at a key has a different type than the caller expected.
    TypeError {
        /// The type name the caller requested.
        expected: &'static str,
        /// The type name that was actually stored.
        found: &'static str,
    },
    /// A key that was looked up does not exist in the table.
    /// The contained `String` is the missing key name.
    KeyNotFound(String),
    /// A numeric literal is outside the range of `i64`.
    IntegerOverflow,
    /// A string literal contained an unrecognised escape sequence.
    /// The contained `String` describes the offending sequence.
    InvalidEscape(String),
    /// The input bytes are not valid UTF-8.
    InvalidUtf8,
    /// An underlying I/O operation (file read or write) failed.
    /// The contained `String` is the OS error message.
    Io(String),
    /// The document could not be serialized to TOML text.
    /// The contained `String` describes the reason.
    SerializeError(String),
}

/// A structured error returned by all fallible operations in this crate.
///
/// Use [`TomlError::formatted`] for a human-readable string, or match on
/// [`TomlError::kind`] to handle specific cases.
#[derive(Debug, Clone)]
pub struct TomlError {
    /// The category of error that occurred.
    pub kind: TomlErrorKind,
    /// A human-readable description of the problem.
    pub message: String,
    /// Where in the source the error was detected, if known.
    pub location: Option<SourceLocation>,
}

impl TomlError {
    /// Create a [`TomlErrorKind::ParseError`] with the given message and
    /// source position (`line` and `col` are 1-based).
    pub fn parse(msg: impl Into<String>, line: u32, col: u32) -> Self {
        Self {
            kind: TomlErrorKind::ParseError,
            message: msg.into(),
            location: Some(SourceLocation {
                line,
                column: col,
                source_file: None,
            }),
        }
    }

    /// Create a [`TomlErrorKind::TypeError`] that reports the key name together
    /// with the expected and actual type names.
    pub fn type_error(expected: &'static str, found: &'static str, key: &str) -> Self {
        Self {
            kind: TomlErrorKind::TypeError { expected, found },
            message: format!(
                "type error for key '{}': expected {}, found {}",
                key, expected, found
            ),
            location: None,
        }
    }

    /// Create a [`TomlErrorKind::KeyNotFound`] error for the given key name.
    pub fn key_not_found(key: impl Into<String>) -> Self {
        let k = key.into();
        Self {
            kind: TomlErrorKind::KeyNotFound(k.clone()),
            message: format!("key not found: '{}'", k),
            location: None,
        }
    }

    /// Create a [`TomlErrorKind::IntegerOverflow`] error at the given source
    /// position (`line` and `col` are 1-based).
    pub fn integer_overflow(msg: impl Into<String>, line: u32, col: u32) -> Self {
        Self {
            kind: TomlErrorKind::IntegerOverflow,
            message: msg.into(),
            location: Some(SourceLocation {
                line,
                column: col,
                source_file: None,
            }),
        }
    }

    /// Return a human-readable error string.
    ///
    /// If a [`SourceLocation`] is present the result has the form
    /// `"<file>:<line>:<col>: <message>"` (or `"<line>:<col>: <message>"` when
    /// no file name is set); otherwise just `"<message>"`.
    pub fn formatted(&self) -> String {
        if let Some(ref loc) = self.location {
            format!("{}: {}", loc, self.message)
        } else {
            self.message.clone()
        }
    }

    /// Attach a file path to the error's source location and return `self`.
    ///
    /// Has no effect if the error has no associated [`SourceLocation`].
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        if let Some(ref mut loc) = self.location {
            loc.source_file = Some(file.into());
        }
        self
    }
}

impl fmt::Display for TomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.formatted())
    }
}

impl std::error::Error for TomlError {}

impl From<std::io::Error> for TomlError {
    fn from(e: std::io::Error) -> Self {
        Self {
            kind: TomlErrorKind::Io(e.to_string()),
            message: e.to_string(),
            location: None,
        }
    }
}
