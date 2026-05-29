//! The top-level [`Document`] type that owns a parsed TOML document.

use std::io::Read;
use std::path::Path;

use crate::cst::DocumentItem;
use crate::error::TomlError;
use crate::serializer::SerializeOptions;
use crate::value::{FromValue, Table, Value};

pub use crate::serializer::serialize;

/// A complete, parsed TOML document.
///
/// A `Document` owns the root [`Table`] and exposes convenience methods for
/// parsing, value access, and serialization.
///
/// Documents produced by [`Document::parse`] preserve the full source
/// formatting internally.  Calling [`Document::serialize`] on such a document
/// reproduces the original text exactly — including comments, string quoting
/// style, number radix and underscores, blank lines, and inline vs. block
/// table style — except for values explicitly changed via
/// [`Document::set_value`].
///
/// Documents created with [`Document::from_table`] contain no formatting
/// metadata and always serialize to canonical TOML.
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
#[derive(Debug, Clone)]
pub struct Document {
    root: Table,
    /// Source-order items for format-preserving serialization.
    /// Empty for programmatically constructed documents.
    pub(crate) items: Vec<DocumentItem>,
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl Document {
    /// The TOML specification version implemented by this library,
    /// as a `(major, minor, patch)` tuple.
    pub const TOML_VERSION: (u32, u32, u32) = (1, 1, 0);

    /// Internal constructor used by the parser: supplies both DOM and items.
    pub(crate) fn from_parts(root: Table, items: Vec<DocumentItem>) -> Self {
        Self { root, items }
    }

    /// Create a `Document` from an already-built root [`Table`].
    ///
    /// The resulting document has no formatting metadata; it will always
    /// serialize to canonical TOML.
    pub fn from_table(root: Table) -> Self {
        Self {
            root,
            items: vec![],
        }
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
    pub fn parse_reader(mut reader: impl Read) -> Result<Self, TomlError> {
        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        Self::parse(&text)
    }

    /// Read the file at `path` and parse its contents as TOML.
    pub fn parse_file(path: impl AsRef<Path>) -> Result<Self, TomlError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| TomlError::from(e).with_file(path.display().to_string()))?;
        Self::parse(&text).map_err(|e| e.with_file(path.display().to_string()))
    }

    // ── Access ────────────────────────────────────────────────────────────────

    /// Return a shared reference to the document's root table.
    pub fn root(&self) -> &Table {
        &self.root
    }

    /// Return a mutable reference to the document's root table.
    ///
    /// Mutations through this reference update the DOM but do **not**
    /// automatically update the format-preserving items list.  Use
    /// [`Document::set_value`] to modify values while keeping formatting
    /// intact.
    pub fn root_mut(&mut self) -> &mut Table {
        &mut self.root
    }

    /// Look up a top-level key and return a typed reference.
    pub fn get<T: FromValue>(&self, key: &str) -> Result<&T, TomlError> {
        self.root.get_as::<T>(key)
    }

    /// Look up a value by dot-separated path starting from the document root.
    pub fn path(&self, dotted: &str) -> Option<&Value> {
        self.root.get_path(dotted)
    }

    /// Look up a value mutably by dot-separated path starting from the
    /// document root.
    pub fn path_mut(&mut self, dotted: &str) -> Option<&mut Value> {
        self.root.get_path_mut(dotted)
    }

    // ── Format-preserving mutation ────────────────────────────────────────────

    /// Replace the value at `path` while preserving the formatting of all
    /// other entries.
    ///
    /// The matching entry's `raw_value` is cleared so the serializer will
    /// regenerate that one value in canonical form.  All surrounding
    /// whitespace, comments, and other entries remain untouched.
    ///
    /// Returns `true` when the path was found and updated, `false` when no
    /// matching entry exists in the items list (the DOM is not modified in
    /// that case either).
    pub fn set_value(&mut self, path: &[&str], value: Value) -> bool {
        let path_owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();

        // Update items list
        let mut found = false;
        for item in &mut self.items {
            if let DocumentItem::Entry { node, path: p } = item {
                if *p == path_owned {
                    node.raw_value = None;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return false;
        }

        // Update DOM
        let _ = self.root.insert_path_segments(path, value);
        true
    }

    /// Return the source-order items list.
    ///
    /// This is the flat sequence of entries, section headers, and trailing
    /// whitespace that drives format-preserving serialization.  Empty for
    /// documents that were not produced by parsing.
    pub fn items(&self) -> &[DocumentItem] {
        &self.items
    }

    // ── Serialization ─────────────────────────────────────────────────────────

    /// Serialize the document to a TOML-formatted string using default
    /// [`SerializeOptions`].
    ///
    /// If the document was produced by [`Document::parse`], the output
    /// preserves all original formatting except for values changed via
    /// [`Document::set_value`].
    pub fn serialize(&self) -> String {
        crate::serializer::serialize(self, &SerializeOptions::default())
    }

    /// Serialize the document to a TOML-formatted string using the supplied
    /// [`SerializeOptions`].
    pub fn serialize_with(&self, opts: &SerializeOptions) -> String {
        crate::serializer::serialize(self, opts)
    }

    /// Serialize the document and write the result to a file.
    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<(), TomlError> {
        self.write_file_with(path, &SerializeOptions::default())
    }

    /// Serialize with `opts` and write the result to a file.
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
