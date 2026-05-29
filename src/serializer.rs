//! TOML serializer: converts a [`Document`] back to TOML text.
//!
//! Two serialization paths exist:
//!
//! * **Format-preserving** — used whenever the document was produced by
//!   parsing.  The serializer walks the `items` list and emits raw source
//!   text for each entry.  Only entries whose `raw_value` was cleared (via
//!   [`Document::set_value`]) are regenerated in canonical form.  All
//!   comments, blank lines, string styles, and number representations are
//!   reproduced verbatim.
//!
//! * **Canonical DOM** — used for documents constructed programmatically via
//!   [`Document::from_table`].  This is the original v0.1 behaviour.

use std::collections::HashSet;

use crate::cst::DocumentItem;
use crate::document::Document;
use crate::value::{Array, Table, Value};

/// Configuration for the canonical DOM serializer.
///
/// These options only affect output when the document has no items list
/// (i.e. was constructed programmatically).  For parsed documents the
/// original formatting is used regardless of these settings — except
/// `trailing_newline`, which is always respected.
///
/// # Defaults
///
/// | Field              | Default         |
/// |--------------------|-----------------|
/// | `sort_keys`        | `false`         |
/// | `prefer_inline`    | `false`         |
/// | `indent`           | `"    "` (4 sp) |
/// | `trailing_newline` | `true`          |
#[derive(Debug, Clone)]
pub struct SerializeOptions {
    /// When `true`, all table keys are emitted in ascending alphabetical order.
    pub sort_keys: bool,
    /// When `true`, every table is serialized as an inline table `{ … }`.
    pub prefer_inline: bool,
    /// The string prepended per indentation level when writing nested tables.
    pub indent: String,
    /// When `true` (the default), a final newline character is appended to
    /// the output if it does not already end with one.
    pub trailing_newline: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            sort_keys: false,
            prefer_inline: false,
            indent: "    ".to_owned(),
            trailing_newline: true,
        }
    }
}

/// Serialize `doc` to a TOML string using the given [`SerializeOptions`].
///
/// When the document was produced by parsing **and** neither `sort_keys` nor
/// `prefer_inline` is set, the format-preserving path is used: comments,
/// string quoting style, number radix/underscores, blank lines, and inline
/// vs. block table style are all reproduced verbatim.  The `trailing_newline`
/// option is ignored on this path — the original line ending is preserved.
///
/// In all other cases (programmatic document, or `sort_keys`/`prefer_inline`
/// requested) the canonical DOM serializer is used and `trailing_newline` is
/// respected.
pub fn serialize(doc: &Document, opts: &SerializeOptions) -> String {
    let use_dom = doc.items.is_empty() || opts.sort_keys || opts.prefer_inline;
    if use_dom {
        let ser = DomSerializer { opts };
        let mut out = ser.serialize(doc);
        if opts.trailing_newline && !out.ends_with('\n') {
            out.push('\n');
        }
        return out;
    }

    // Format-preserving path — don't override original line endings
    serialize_from_items(doc)
}

// ── Format-preserving serializer ──────────────────────────────────────────────

fn serialize_from_items(doc: &Document) -> String {
    let mut out = String::new();

    // Collect all item entry paths and which ones are inline tables.
    let mut item_paths: HashSet<Vec<String>> = HashSet::new();
    // Paths whose value is stored as an inline table raw_value (starts with '{').
    // These must not be recursed into when looking for new DOM values.
    let mut inline_table_paths: HashSet<Vec<String>> = HashSet::new();

    for item in &doc.items {
        if let DocumentItem::Entry { node, path } = item {
            if node.raw_value.as_deref().map_or(false, |r| r.trim_start().starts_with('{')) {
                inline_table_paths.insert(path.clone());
            }
        }
    }

    for item in &doc.items {
        match item {
            DocumentItem::Eof(s) => {
                out.push_str(s);
            }
            DocumentItem::Section(s) => {
                out.push_str(&s.leading);
                out.push_str(&s.raw);
                out.push_str(&s.trailing);
            }
            DocumentItem::Entry { node, path } => {
                // Skip if the entry was deleted from the DOM
                if lookup_path(doc.root(), path).is_none() {
                    continue;
                }
                item_paths.insert(path.clone());

                out.push_str(&node.leading);
                out.push_str(&node.raw_key);
                out.push_str(&node.pre_eq);
                out.push('=');
                out.push_str(&node.post_eq);

                if let Some(raw) = &node.raw_value {
                    out.push_str(raw);
                } else {
                    // Value was changed: regenerate in canonical form
                    let val = lookup_path(doc.root(), path).unwrap();
                    write_value_canonical(&mut out, val, 0);
                }

                out.push_str(&node.trailing);
            }
        }
    }

    // Append any DOM values added after parsing (not covered by items).
    // Build covered set: all prefixes of item paths.
    let mut covered: HashSet<Vec<String>> = HashSet::new();
    for p in &item_paths {
        for len in 1..=p.len() {
            covered.insert(p[..len].to_vec());
        }
    }
    append_new_dom_values(&mut out, doc.root(), &covered, &inline_table_paths, &[]);

    out
}

/// Walk the DOM and output any paths not already covered by items.
fn append_new_dom_values(
    out: &mut String,
    table: &Table,
    covered: &HashSet<Vec<String>>,
    inline_roots: &HashSet<Vec<String>>,
    prefix: &[String],
) {
    for (key, val) in table.iter() {
        let mut path = prefix.to_vec();
        path.push(key.to_string());

        match val {
            Value::Table(t) => {
                if covered.contains(&path) {
                    if inline_roots.contains(&path) {
                        // Inline table: contents are in raw_value, do not recurse
                        continue;
                    }
                    // Block table covered by items — recurse for new sub-entries
                    append_new_dom_values(out, t, covered, inline_roots, &path);
                } else {
                    // New table — output as [section]
                    out.push('\n');
                    out.push('[');
                    out.push_str(&path_to_header(&path));
                    out.push_str("]\n");
                    append_new_dom_values(out, t, covered, inline_roots, &path);
                }
            }
            Value::Array(arr) if is_array_of_tables(arr) => {
                for (i, elem) in arr.iter().enumerate() {
                    let mut elem_path = path.clone();
                    elem_path.push(i.to_string());
                    if covered.contains(&elem_path) {
                        continue;
                    }
                    if let Value::Table(t) = elem {
                        out.push('\n');
                        out.push_str("[[");
                        out.push_str(&path_to_header(&path));
                        out.push_str("]]\n");
                        append_new_dom_values(out, t, covered, inline_roots, &elem_path);
                    }
                }
            }
            _ => {
                if !covered.contains(&path) {
                    let key_str = if needs_quoting(key) {
                        format!("\"{}\"", key.replace('\\', "\\\\").replace('"', "\\\""))
                    } else {
                        key.to_string()
                    };
                    out.push_str(&key_str);
                    out.push_str(" = ");
                    write_value_canonical(out, val, 0);
                    out.push('\n');
                }
            }
        }
    }
}

fn path_to_header(path: &[String]) -> String {
    path.iter()
        .map(|k| {
            if needs_quoting(k) {
                format!("\"{}\"", k.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                k.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Traverse a DOM path that may contain stringified array indices.
pub(crate) fn lookup_path<'a>(root: &'a Table, path: &[String]) -> Option<&'a Value> {
    if path.is_empty() {
        return None;
    }
    let first = root.get(&path[0])?;
    if path.len() == 1 {
        return Some(first);
    }
    let mut current = first;
    for seg in &path[1..] {
        if let Ok(idx) = seg.parse::<usize>() {
            match current {
                Value::Array(arr) => current = arr.get(idx)?,
                _ => return None,
            }
        } else {
            match current {
                Value::Table(t) => current = t.get(seg)?,
                _ => return None,
            }
        }
    }
    Some(current)
}

// ── Canonical value writer (for new/changed values) ───────────────────────────

fn write_value_canonical(out: &mut String, val: &Value, depth: usize) {
    match val {
        Value::String(s) => write_string_canonical(out, s),
        Value::Integer(n) => out.push_str(&n.to_string()),
        Value::Float(f) => write_float_canonical(out, *f),
        Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::OffsetDateTime(dt) => out.push_str(&dt.to_string()),
        Value::LocalDateTime(dt) => out.push_str(&dt.to_string()),
        Value::LocalDate(d) => out.push_str(&d.to_string()),
        Value::LocalTime(t) => out.push_str(&t.to_string()),
        Value::Array(arr) => write_array_canonical(out, arr, depth),
        Value::Table(tbl) => write_inline_table_canonical(out, tbl),
    }
}

fn write_float_canonical(out: &mut String, f: f64) {
    if f.is_nan() {
        out.push_str("nan");
    } else if f.is_infinite() {
        if f.is_sign_positive() {
            out.push_str("inf");
        } else {
            out.push_str("-inf");
        }
    } else {
        let s = format!("{}", f);
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            out.push_str(&s);
            out.push_str(".0");
        } else {
            out.push_str(&s);
        }
    }
}

fn write_array_canonical(out: &mut String, arr: &Array, depth: usize) {
    if arr.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (i, val) in arr.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_value_canonical(out, val, depth);
    }
    out.push(']');
}

fn write_inline_table_canonical(out: &mut String, tbl: &Table) {
    out.push('{');
    for (i, (key, val)) in tbl.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if needs_quoting(key) {
            out.push('"');
            for ch in key.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c => out.push(c),
                }
            }
            out.push('"');
        } else {
            out.push_str(key);
        }
        out.push_str(" = ");
        write_value_canonical(out, val, 0);
    }
    out.push('}');
}

fn write_string_canonical(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\x08' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\x0C' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ── Canonical DOM serializer (programmatic documents) ─────────────────────────

struct DomSerializer<'opts> {
    opts: &'opts SerializeOptions,
}

impl<'opts> DomSerializer<'opts> {
    fn serialize(&self, doc: &Document) -> String {
        let mut out = String::new();
        self.write_table_contents(&mut out, doc.root(), "", 0, true);
        out
    }

    fn write_table_contents(
        &self,
        out: &mut String,
        table: &Table,
        prefix: &str,
        depth: usize,
        _is_root: bool,
    ) {
        let keys: Vec<&str> = if self.opts.sort_keys {
            let mut ks: Vec<&str> = table.keys().collect();
            ks.sort_unstable();
            ks
        } else {
            table.keys().collect()
        };

        let mut deferred: Vec<&str> = Vec::new();

        for &key in &keys {
            let val = &table[key];
            match val {
                Value::Table(t) => {
                    if self.prefer_inline_table(t) {
                        self.write_key_value(out, key, val, depth);
                    } else {
                        deferred.push(key);
                    }
                }
                Value::Array(arr) if is_array_of_tables(arr) => {
                    deferred.push(key);
                }
                _ => {
                    self.write_key_value(out, key, val, depth);
                }
            }
        }

        for &key in &deferred {
            let val = &table[key];
            let subpath = if prefix.is_empty() {
                key_to_string(key)
            } else {
                format!("{}.{}", prefix, key_to_string(key))
            };
            match val {
                Value::Table(t) => {
                    out.push('\n');
                    out.push('[');
                    out.push_str(&subpath);
                    out.push_str("]\n");
                    self.write_table_contents(out, t, &subpath, depth + 1, false);
                }
                Value::Array(arr) if is_array_of_tables(arr) => {
                    for elem in &arr.0 {
                        if let Value::Table(t) = elem {
                            out.push('\n');
                            out.push_str("[[");
                            out.push_str(&subpath);
                            out.push_str("]]\n");
                            self.write_table_contents(out, t, &subpath, depth + 1, false);
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    fn write_key_value(&self, out: &mut String, key: &str, val: &Value, depth: usize) {
        self.write_key(out, key);
        out.push_str(" = ");
        self.write_value(out, val, depth);
        out.push('\n');
    }

    fn write_value(&self, out: &mut String, val: &Value, depth: usize) {
        match val {
            Value::String(s) => write_string_canonical(out, s),
            Value::Integer(n) => out.push_str(&n.to_string()),
            Value::Float(f) => write_float_canonical(out, *f),
            Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::OffsetDateTime(dt) => out.push_str(&dt.to_string()),
            Value::LocalDateTime(dt) => out.push_str(&dt.to_string()),
            Value::LocalDate(d) => out.push_str(&d.to_string()),
            Value::LocalTime(t) => out.push_str(&t.to_string()),
            Value::Array(arr) => self.write_array(out, arr, depth),
            Value::Table(tbl) => write_inline_table_canonical(out, tbl),
        }
    }

    fn write_array(&self, out: &mut String, arr: &Array, depth: usize) {
        if arr.is_empty() {
            out.push_str("[]");
            return;
        }
        out.push('[');
        for (i, val) in arr.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write_value(out, val, depth);
        }
        out.push(']');
    }

    fn write_key(&self, out: &mut String, key: &str) {
        if needs_quoting(key) {
            out.push('"');
            for ch in key.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c => out.push(c),
                }
            }
            out.push('"');
        } else {
            out.push_str(key);
        }
    }

    fn prefer_inline_table(&self, tbl: &Table) -> bool {
        if self.opts.prefer_inline {
            return true;
        }
        tbl.len() <= 4 && !has_sub_tables(tbl)
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn needs_quoting(key: &str) -> bool {
    if key.is_empty() {
        return true;
    }
    !key.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn has_sub_tables(tbl: &Table) -> bool {
    tbl.inner
        .values()
        .any(|v| matches!(v, Value::Table(_) | Value::Array(_)))
}

fn is_array_of_tables(arr: &Array) -> bool {
    arr.iter().all(|v| matches!(v, Value::Table(_))) && !arr.is_empty()
}

fn key_to_string(key: &str) -> String {
    if needs_quoting(key) {
        let mut s = String::new();
        s.push('"');
        for ch in key.chars() {
            match ch {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                c => s.push(c),
            }
        }
        s.push('"');
        s
    } else {
        key.to_string()
    }
}
