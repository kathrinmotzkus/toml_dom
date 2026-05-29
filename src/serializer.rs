//! TOML serializer: converts a [`Document`] back to TOML text.
//!
//! Two serialization paths:
//!
//! * **Format-preserving** — used for parsed documents without `sort_keys`/
//!   `prefer_inline`.  Walks the `items` list and emits raw source text at
//!   every level (scalars, array elements, inline table entries).  Only nodes
//!   whose `raw` was cleared by [`Document::set_value`] are regenerated.
//!
//! * **Canonical DOM** — used for programmatic documents or when `sort_keys`/
//!   `prefer_inline` is set.  Original v0.1 behaviour.

use std::collections::HashSet;

use crate::cst::{ArrayNode, DocumentItem, InlineTableNode, ValueNode};
use crate::document::Document;
use crate::value::{Array, Table, Value};

/// Configuration for the canonical DOM serializer.
#[derive(Debug, Clone)]
pub struct SerializeOptions {
    /// Sort all table keys alphabetically (forces canonical path).
    pub sort_keys: bool,
    /// Emit every table as an inline table `{ … }` (forces canonical path).
    pub prefer_inline: bool,
    /// Indentation string for nested tables (canonical path only).
    pub indent: String,
    /// Append a final `\n` if missing (canonical path only).
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
    serialize_from_items(doc)
}

// ── Format-preserving serializer ──────────────────────────────────────────────

fn serialize_from_items(doc: &Document) -> String {
    let mut out = String::new();
    let mut item_paths: HashSet<Vec<String>> = HashSet::new();

    // Paths whose top-level value is an inline table (ValueNode::InlineTable).
    // We must NOT recurse into these in append_new_dom_values.
    let mut inline_table_paths: HashSet<Vec<String>> = HashSet::new();
    for item in &doc.items {
        if let DocumentItem::Entry { node, path } = item {
            if matches!(node.node, ValueNode::InlineTable(_)) {
                inline_table_paths.insert(path.clone());
            }
        }
    }

    for item in &doc.items {
        match item {
            DocumentItem::Eof(s) => out.push_str(s),
            DocumentItem::Section(s) => {
                out.push_str(&s.leading);
                out.push_str(&s.raw);
                out.push_str(&s.trailing);
            }
            DocumentItem::Entry { node, path } => {
                if lookup_path(doc.root(), path).is_none() {
                    continue;
                }
                item_paths.insert(path.clone());

                out.push_str(&node.leading);
                out.push_str(&node.raw_key);
                out.push_str(&node.pre_eq);
                out.push('=');
                out.push_str(&node.post_eq);

                let current_val = lookup_path(doc.root(), path).unwrap();
                write_value_node(&mut out, &node.node, current_val);

                out.push_str(&node.trailing);
            }
        }
    }

    // Covered set: all prefixes of item entry paths + all section paths.
    let mut covered: HashSet<Vec<String>> = HashSet::new();
    for p in &item_paths {
        for len in 1..=p.len() {
            covered.insert(p[..len].to_vec());
        }
    }
    for item in &doc.items {
        if let DocumentItem::Section(s) = item {
            covered.insert(s.path.clone());
        }
    }
    append_new_dom_values(&mut out, doc.root(), &covered, &inline_table_paths, &[]);
    out
}

// ── ValueNode writer ──────────────────────────────────────────────────────────

/// Emit a [`ValueNode`], using `current_val` as the semantic fallback when
/// a scalar node's `raw` is `None`.
fn write_value_node(out: &mut String, node: &ValueNode, current_val: &Value) {
    match node {
        ValueNode::Scalar { raw: Some(raw), .. } => out.push_str(raw),
        ValueNode::Scalar { raw: None, value } => write_value_canonical(out, value, 0),
        ValueNode::Array(arr_node) => write_array_node(out, arr_node, current_val),
        ValueNode::InlineTable(tbl_node) => write_inline_table_node(out, tbl_node, current_val),
    }
}

fn write_array_node(out: &mut String, node: &ArrayNode, current_val: &Value) {
    out.push_str(&node.open);
    let elems = if let Value::Array(arr) = current_val { Some(arr) } else { None };
    for (i, elem) in node.elements.iter().enumerate() {
        out.push_str(&elem.leading);
        let fallback = elems.and_then(|a| a.get(i)).unwrap_or(&Value::Boolean(false));
        write_value_node(out, &elem.node, fallback);
        out.push_str(&elem.trailing);
        if let Some(comma) = &elem.comma {
            out.push_str(comma);
        }
    }
    out.push_str(&node.close);
}

fn write_inline_table_node(out: &mut String, node: &InlineTableNode, current_val: &Value) {
    out.push_str(&node.open);
    let tbl = if let Value::Table(t) = current_val { Some(t) } else { None };
    for entry in &node.entries {
        out.push_str(&entry.leading);
        out.push_str(&entry.raw_key);
        out.push_str(&entry.pre_eq);
        out.push('=');
        out.push_str(&entry.post_eq);
        // Look up the current DOM value for this entry key
        let key_segs = crate::cst::raw_key_to_segments(&entry.raw_key);
        let fallback_val = tbl
            .and_then(|t| lookup_segments(t, &key_segs))
            .unwrap_or(&Value::Boolean(false));
        write_value_node(out, &entry.node, fallback_val);
        out.push_str(&entry.trailing);
        if let Some(comma) = &entry.comma {
            out.push_str(comma);
        }
    }
    out.push_str(&node.close);
}

/// Navigate a table by decoded key segments (no array index support needed here).
fn lookup_segments<'a>(table: &'a Table, segs: &[String]) -> Option<&'a Value> {
    if segs.is_empty() { return None; }
    let first = table.get(&segs[0])?;
    if segs.len() == 1 { return Some(first); }
    let mut current = first;
    for seg in &segs[1..] {
        match current {
            Value::Table(t) => current = t.get(seg)?,
            _ => return None,
        }
    }
    Some(current)
}

// ── Append new DOM values ─────────────────────────────────────────────────────

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
                        continue;
                    }
                    append_new_dom_values(out, t, covered, inline_roots, &path);
                } else {
                    out.push('\n');
                    out.push('[');
                    out.push_str(&path_to_header(&path));
                    out.push_str("]\n");
                    append_new_dom_values(out, t, covered, inline_roots, &path);
                }
            }
            Value::Array(arr) if is_array_of_tables(arr) => {
                if covered.contains(&path) {
                    continue;
                }
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
/// Checks value type first so that digit-string table keys are handled correctly.
pub(crate) fn lookup_path<'a>(root: &'a Table, path: &[String]) -> Option<&'a Value> {
    if path.is_empty() { return None; }
    let first = root.get(&path[0])?;
    if path.len() == 1 { return Some(first); }
    let mut current = first;
    for seg in &path[1..] {
        match current {
            Value::Array(arr) => {
                let idx = seg.parse::<usize>().ok()?;
                current = arr.get(idx)?;
            }
            Value::Table(t) => current = t.get(seg)?,
            _ => return None,
        }
    }
    Some(current)
}

// ── Canonical value writer ────────────────────────────────────────────────────

pub(crate) fn write_value_canonical(out: &mut String, val: &Value, depth: usize) {
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
        out.push_str(if f.is_sign_positive() { "inf" } else { "-inf" });
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
    if arr.is_empty() { out.push_str("[]"); return; }
    out.push('[');
    for (i, val) in arr.iter().enumerate() {
        if i > 0 { out.push_str(", "); }
        write_value_canonical(out, val, depth);
    }
    out.push(']');
}

fn write_inline_table_canonical(out: &mut String, tbl: &Table) {
    out.push('{');
    for (i, (key, val)) in tbl.iter().enumerate() {
        if i > 0 { out.push_str(", "); }
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

// ── Canonical DOM serializer ──────────────────────────────────────────────────

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
                    if self.prefer_inline_table(t) { self.write_key_value(out, key, val, depth); }
                    else { deferred.push(key); }
                }
                Value::Array(arr) if is_array_of_tables(arr) => deferred.push(key),
                _ => self.write_key_value(out, key, val, depth),
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
            Value::Array(arr) => write_array_canonical(out, arr, depth),
            Value::Table(tbl) => write_inline_table_canonical(out, tbl),
        }
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
        self.opts.prefer_inline || (tbl.len() <= 4 && !has_sub_tables(tbl))
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn needs_quoting(key: &str) -> bool {
    key.is_empty()
        || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn has_sub_tables(tbl: &Table) -> bool {
    tbl.inner.values().any(|v| matches!(v, Value::Table(_) | Value::Array(_)))
}

fn is_array_of_tables(arr: &Array) -> bool {
    !arr.is_empty() && arr.iter().all(|v| matches!(v, Value::Table(_)))
}

fn key_to_string(key: &str) -> String {
    if needs_quoting(key) {
        let mut s = String::from('"');
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
