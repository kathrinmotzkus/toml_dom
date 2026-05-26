//! The top-level [`Document`] type that owns a parsed TOML document.

use std::io::Read;
use std::path::Path;

use crate::error::TomlError;
use crate::serializer::SerializeOptions;
use crate::value::{FromValue, Table, Value};

pub use crate::serializer::serialize;

/// A complete, parsed TOML document.
///
/// A `Document` owns the root [`Table`] and exposes convenience methods for
/// parsing, value access, and serialization.
///
/// # Example
///
/// ```rust
/// use toml_dom::Document;
///
/// let doc = Document::parse("[db]\nport = 5432\n").unwrap();
/// let port: &i64 = doc.get::<i64>("port")
///     .or_else(|_| {
///         doc.path("db.port")
///             .and_then(|v| if let toml_dom::Value::Integer(n) = v { Some(n) } else { None })
///             .ok_or_else(|| toml_dom::TomlError::key_not_found("db.port"))
///     })
///     .unwrap();
/// assert_eq!(*port, 5432);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    root: Table,
}

impl Document {
    /// The TOML specification version implemented by this library,
    /// as a `(major, minor, patch)` tuple.
    pub const TOML_VERSION: (u32, u32, u32) = (1, 1, 0);

    /// Create a `Document` from an already-built root [`Table`].
    pub(crate) fn from_root(root: Table) -> Self {
        Self { root }
    }

    /// Create a `Document` from a pre-built root [`Table`].
    pub fn from_table(root: Table) -> Self {
        Self { root }
    }

    // ── Parsing ───────────────────────────────────────────────────────────────

    /// Parse a TOML document from a string slice.
    ///
    /// Returns `Err(TomlError)` on any syntax error, duplicate key, or
    /// integer overflow detected during parsing.
    pub fn parse(text: &str) -> Result<Self, TomlError> {
        crate::parser::parse(text)
    }

    /// Parse a TOML document by reading all bytes from `reader` into a string
    /// and then parsing it.
    ///
    /// Returns `Err(TomlError { kind: Io(…), … })` on read failure, or a
    /// parse error if the content is invalid TOML.
    pub fn parse_reader(mut reader: impl Read) -> Result<Self, TomlError> {
        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        Self::parse(&text)
    }

    /// Read the file at `path` and parse its contents as TOML.
    ///
    /// The file path is attached to any returned error via
    /// [`TomlError::with_file`], so error messages include the file name.
    /// Returns `Err(TomlError { kind: Io(…), … })` when the file cannot be
    /// read, or a parse error for invalid TOML content.
    pub fn parse_file(path: impl AsRef<Path>) -> Result<Self, TomlError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| {
            TomlError::from(e).with_file(path.display().to_string())
        })?;
        Self::parse(&text)
            .map_err(|e| e.with_file(path.display().to_string()))
    }

    // ── Access ────────────────────────────────────────────────────────────────

    /// Return a shared reference to the document's root table.
    pub fn root(&self) -> &Table {
        &self.root
    }

    /// Return a mutable reference to the document's root table.
    pub fn root_mut(&mut self) -> &mut Table {
        &mut self.root
    }

    /// Look up a top-level key and return a typed reference.
    ///
    /// Shorthand for `self.root().get_as::<T>(key)`.
    /// Returns `Err(TomlError { kind: KeyNotFound, … })` when the key does
    /// not exist, or `Err(TomlError { kind: TypeError { … }, … })` for a
    /// type mismatch.
    pub fn get<T: FromValue>(&self, key: &str) -> Result<&T, TomlError> {
        self.root.get_as::<T>(key)
    }

    /// Look up a value by dot-separated path starting from the document root.
    ///
    /// For example `doc.path("server.host")` is equivalent to
    /// `doc.root().get_path("server.host")`.
    /// Returns `None` if any path component is missing or not a table.
    pub fn path(&self, dotted: &str) -> Option<&Value> {
        self.root.get_path(dotted)
    }

    /// Look up a value mutably by dot-separated path starting from the
    /// document root.
    ///
    /// Returns `None` under the same conditions as [`Document::path`].
    pub fn path_mut(&mut self, dotted: &str) -> Option<&mut Value> {
        self.root.get_path_mut(dotted)
    }

    // ── Serialization ─────────────────────────────────────────────────────────

    /// Serialize the document to a TOML-formatted string using default
    /// [`SerializeOptions`].
    pub fn serialize(&self) -> String {
        crate::serializer::serialize(self, &SerializeOptions::default())
    }

    /// Serialize the document to a TOML-formatted string using the supplied
    /// [`SerializeOptions`].
    pub fn serialize_with(&self, opts: &SerializeOptions) -> String {
        crate::serializer::serialize(self, opts)
    }

    /// Serialize the document and write the result to a file, using default
    /// [`SerializeOptions`].
    ///
    /// Returns `Err(TomlError { kind: Io(…), … })` if the file cannot be written.
    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<(), TomlError> {
        self.write_file_with(path, &SerializeOptions::default())
    }

    /// Serialize the document with `opts` and write the result to a file.
    ///
    /// Returns `Err(TomlError { kind: Io(…), … })` if the file cannot be written.
    pub fn write_file_with(
        &self,
        path: impl AsRef<Path>,
        opts: &SerializeOptions,
    ) -> Result<(), TomlError> {
        let text = self.serialize_with(opts);
        std::fs::write(path, text)?;
        Ok(())
    }
}
