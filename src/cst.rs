//! Concrete syntax tree types for format-preserving TOML roundtrip.
//!
//! A parsed [`Document`](crate::Document) stores a flat [`Vec<DocumentItem>`]
//! that mirrors the source file in its original order.  The serializer walks
//! this list and emits the stored raw source text wherever available, so
//! comments, string quoting style, number radix/underscores, inline vs.
//! block table style, blank lines, and trailing comments are all reproduced
//! exactly.
//!
//! Values modified through [`Document::set_value`] have their `raw_value`
//! cleared; the serializer then regenerates those values in canonical form
//! while leaving everything else untouched.

/// Formatting metadata for a single key-value entry in a parsed document.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryNode {
    /// Blank lines and comment lines that appear immediately before the key.
    pub leading: String,
    /// The key exactly as written in the source, e.g. `"foo bar"` or `a.b.c`.
    pub raw_key: String,
    /// Whitespace between the key and the `=` sign.
    pub pre_eq: String,
    /// Whitespace between the `=` sign and the value.
    pub post_eq: String,
    /// Original value source text, e.g. `0xFF`, `'literal'`, `"""multi"""`.
    /// `None` tells the serializer to regenerate the value in canonical form.
    pub raw_value: Option<String>,
    /// Text from after the value to the end of the line: optional inline
    /// comment (including `#`) followed by the newline character(s).
    pub trailing: String,
}

/// A `[section]` or `[[array-of-tables]]` header with its surrounding trivia.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionNode {
    /// Blank lines and comment lines before the opening `[`.
    pub leading: String,
    /// The complete header text as written, e.g. `[server]` or `[[products]]`.
    pub raw: String,
    /// Text from after the closing `]` to the end of the line.
    pub trailing: String,
    /// Decoded key path segments for DOM lookup, e.g. `["server"]`.
    pub path: Vec<String>,
    /// `true` for `[[array-of-tables]]` headers, `false` for `[table]` headers.
    pub is_array: bool,
}

/// One item in the flat, source-ordered list that drives format-preserving
/// serialization.
///
/// A [`Document`](crate::Document) built by parsing always contains a
/// populated `items` list.  A [`Document`](crate::Document) constructed
/// programmatically via [`Document::from_table`](crate::Document::from_table)
/// has an empty list and falls back to the canonical DOM serializer.
#[derive(Debug, Clone, PartialEq)]
pub enum DocumentItem {
    /// A key-value pair with full formatting metadata.
    Entry {
        /// Formatting and raw source text.
        node: EntryNode,
        /// Full DOM path (section prefix + local key segments).
        ///
        /// For array-of-tables entries the path includes a stringified array
        /// index as one segment, e.g. `["products", "0", "name"]` for the
        /// `name` key inside the first `[[products]]` block.
        path: Vec<String>,
    },
    /// A `[section]` or `[[array-of-tables]]` header.
    Section(SectionNode),
    /// Trailing whitespace and/or comments after the last entry in the file.
    Eof(String),
}
