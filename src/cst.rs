//! Concrete syntax tree types for format-preserving TOML roundtrip.
//!
//! Every parsed value carries its original source text alongside its semantic
//! counterpart.  Arrays and inline tables are represented recursively so that
//! individual elements and entries can be modified while the surrounding
//! formatting is reproduced exactly.

use crate::value::Value;

// ── ValueNode ─────────────────────────────────────────────────────────────────

/// A value in the concrete syntax tree — scalar, array, or inline table.
///
/// `ValueNode` mirrors a [`Value`] but stores format information:
/// original source text for scalars, and structured child nodes for compound
/// values.  When `raw` is `None` in a [`Scalar`](ValueNode::Scalar), the
/// serializer regenerates that value in canonical form.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueNode {
    /// A primitive scalar value (string, integer, float, boolean, or datetime).
    Scalar {
        /// Original source text, e.g. `0xFF`, `'hello'`, `"""multi"""`.
        /// `None` means the serializer should regenerate from `value`.
        raw: Option<String>,
        /// The parsed semantic value — used when `raw` is `None`.
        value: Value,
    },
    /// An array `[…]` with per-element formatting.
    Array(ArrayNode),
    /// An inline table `{…}` with per-entry formatting.
    InlineTable(InlineTableNode),
}

impl ValueNode {
    /// Create a scalar node whose value must be regenerated on next serialize.
    pub fn new_dirty(value: Value) -> Self {
        ValueNode::Scalar { raw: None, value }
    }
}

// ── ArrayNode ─────────────────────────────────────────────────────────────────

/// An array `[…]` with full formatting metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayNode {
    /// The opening `[`.
    pub open: String,
    /// The elements in order.
    pub elements: Vec<ArrayElement>,
    /// Whitespace before `]` (if any) followed by `]`.
    pub close: String,
}

/// One element inside an array.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayElement {
    /// Whitespace/comments before the element value (including leading newlines).
    pub leading: String,
    /// The element value.
    pub node: ValueNode,
    /// Whitespace after the value and before the comma or closing `]`.
    pub trailing: String,
    /// The comma character `","` if present, `None` for the last element
    /// when no trailing comma exists.
    pub comma: Option<String>,
}

// ── InlineTableNode ───────────────────────────────────────────────────────────

/// An inline table `{…}` with full formatting metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineTableNode {
    /// The opening `{`.
    pub open: String,
    /// The key-value entries in order.
    pub entries: Vec<InlineEntry>,
    /// Whitespace before `}` (if any) followed by `}`.
    pub close: String,
}

/// One key-value entry inside an inline table.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineEntry {
    /// Whitespace/comments before the key (TOML 1.1: newlines allowed).
    pub leading: String,
    /// Original key source text, e.g. `"foo bar"` or `a.b`.
    pub raw_key: String,
    /// Whitespace between key and `=`.
    pub pre_eq: String,
    /// Whitespace between `=` and value.
    pub post_eq: String,
    /// The entry value.
    pub node: ValueNode,
    /// Whitespace after the value and before the comma or closing `}`.
    pub trailing: String,
    /// The comma `","` if present, `None` for the last entry
    /// when no trailing comma exists.
    pub comma: Option<String>,
}

// ── EntryNode ─────────────────────────────────────────────────────────────────

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
    /// The value node — preserves original source text recursively.
    pub node: ValueNode,
    /// Text from after the value to the end of the line: optional inline
    /// comment (including `#`) followed by the newline character(s).
    pub trailing: String,
}

// ── SectionNode ───────────────────────────────────────────────────────────────

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

// ── DocumentItem ──────────────────────────────────────────────────────────────

/// One item in the flat, source-ordered list that drives format-preserving
/// serialization.
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

// ── Key helpers ───────────────────────────────────────────────────────────────

/// Decode a raw key segment: trim surrounding whitespace, then strip quotes.
///
/// Whitespace can appear around dotted-key separators (e.g. `a . b`), so
/// each segment must be trimmed before checking for quotes.
pub(crate) fn decode_key_segment(raw: &str) -> String {
    let s = raw.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Split a raw key (which may be dotted or quoted) into decoded segments.
///
/// Handles the common cases: bare keys, basic-quoted keys, literal-quoted keys,
/// and dotted combinations thereof.  Dots inside quoted segments are preserved.
pub(crate) fn raw_key_to_segments(raw: &str) -> Vec<String> {
    let mut segs: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_basic = false;
    let mut in_literal = false;

    for ch in raw.chars() {
        match ch {
            '"' if !in_literal => {
                in_basic = !in_basic;
                current.push(ch);
            }
            '\'' if !in_basic => {
                in_literal = !in_literal;
                current.push(ch);
            }
            '.' if !in_basic && !in_literal => {
                segs.push(decode_key_segment(&current));
                current.clear();
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        segs.push(decode_key_segment(&current));
    }
    segs
}
