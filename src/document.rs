//! The top-level [`Document`] type that owns a parsed TOML document.

use std::io::Read;
use std::path::Path;

use crate::cst::{raw_key_to_segments, DocumentItem, ValueNode};
use crate::error::TomlError;
use crate::serializer::SerializeOptions;
use crate::value::{FromValue, Table, Value};

pub use crate::serializer::serialize;

/// A complete, parsed TOML document.
///
/// Documents produced by [`Document::parse`] preserve the full source
/// formatting internally.  Calling [`Document::serialize`] on such a
/// document reproduces the original text exactly — including comments,
/// string quoting style, number radix and underscores, blank lines, and
/// inline vs. block table style — except for values changed via
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
    pub(crate) items: Vec<DocumentItem>,
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl Document {
    /// The TOML specification version implemented by this library.
    pub const TOML_VERSION: (u32, u32, u32) = (1, 1, 0);

    pub(crate) fn from_parts(root: Table, items: Vec<DocumentItem>) -> Self {
        Self { root, items }
    }

    /// Create a `Document` from an already-built root [`Table`].
    ///
    /// The resulting document has no formatting metadata; it always serializes
    /// to canonical TOML.
    pub fn from_table(root: Table) -> Self {
        Self { root, items: vec![] }
    }

    // ── Parsing ───────────────────────────────────────────────────────────────

    /// Parse a TOML document from a string slice.
    pub fn parse(text: &str) -> Result<Self, TomlError> {
        crate::parser::parse(text)
    }

    /// Parse a TOML document from a `Read` source.
    pub fn parse_reader(mut reader: impl Read) -> Result<Self, TomlError> {
        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        Self::parse(&text)
    }

    /// Read and parse a TOML file.
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
    /// [`Document::set_value`] to modify values while keeping formatting intact.
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

    /// Look up a value mutably by dot-separated path starting from the root.
    pub fn path_mut(&mut self, dotted: &str) -> Option<&mut Value> {
        self.root.get_path_mut(dotted)
    }

    // ── Format-preserving mutation ────────────────────────────────────────────

    /// Replace the value at `path` while preserving all surrounding formatting.
    ///
    /// The method searches the items list for an entry whose path starts with
    /// the given path.  For top-level entries (scalars, arrays, inline tables)
    /// the match is exact.  For entries inside inline tables or arrays the
    /// search descends into the [`ValueNode`] tree.
    ///
    /// Returns `true` when the value was found and updated, `false` otherwise.
    pub fn set_value(&mut self, path: &[&str], value: Value) -> bool {
        if path.is_empty() { return false; }
        let path_owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();

        for item in &mut self.items {
            if let DocumentItem::Entry { node, path: item_path } = item {
                // Exact match — top-level entry
                if *item_path == path_owned {
                    node.node = ValueNode::new_dirty(value.clone());
                    let _ = self.root.insert_path_segments(path, value);
                    return true;
                }
                // Prefix match — path leads into this entry's ValueNode tree
                if path_owned.starts_with(item_path.as_slice()) {
                    let remainder = &path_owned[item_path.len()..];
                    if navigate_value_node_mut(&mut node.node, remainder, value.clone()) {
                        let _ = self.root.insert_path_segments(path, value);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Return the source-order items list.
    pub fn items(&self) -> &[DocumentItem] {
        &self.items
    }

    // ── Serialization ─────────────────────────────────────────────────────────

    /// Serialize to TOML text using default [`SerializeOptions`].
    pub fn serialize(&self) -> String {
        crate::serializer::serialize(self, &SerializeOptions::default())
    }

    /// Serialize to TOML text using the supplied [`SerializeOptions`].
    pub fn serialize_with(&self, opts: &SerializeOptions) -> String {
        crate::serializer::serialize(self, opts)
    }

    /// Serialize and write to a file.
    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<(), TomlError> {
        self.write_file_with(path, &SerializeOptions::default())
    }

    /// Serialize with `opts` and write to a file.
    pub fn write_file_with(
        &self,
        path: impl AsRef<Path>,
        opts: &SerializeOptions,
    ) -> Result<(), TomlError> {
        std::fs::write(path, self.serialize_with(opts))?;
        Ok(())
    }
}

// ── ValueNode navigation ──────────────────────────────────────────────────────

/// Descend into a [`ValueNode`] tree by `path` and mark the target as dirty.
///
/// Returns `true` when the path was resolved and the node was updated.
fn navigate_value_node_mut(node: &mut ValueNode, path: &[String], value: Value) -> bool {
    if path.is_empty() {
        *node = ValueNode::new_dirty(value);
        return true;
    }
    match node {
        ValueNode::InlineTable(tbl) => {
            for entry in &mut tbl.entries {
                let key_segs = raw_key_to_segments(&entry.raw_key);
                if path.starts_with(key_segs.as_slice()) {
                    let sub_path = &path[key_segs.len()..];
                    return navigate_value_node_mut(&mut entry.node, sub_path, value);
                }
            }
            false
        }
        ValueNode::Array(arr) => {
            if let Ok(idx) = path[0].parse::<usize>() {
                if let Some(elem) = arr.elements.get_mut(idx) {
                    return navigate_value_node_mut(&mut elem.node, &path[1..], value);
                }
            }
            false
        }
        ValueNode::Scalar { .. } => false,
    }
}
