//! TOML serializer: converts a [`Document`] back to TOML text.
//!
//! The main entry point is [`Document::serialize`] / [`Document::serialize_with`].
//! This module also exposes [`SerializeOptions`] for controlling output style.

use crate::document::Document;
use crate::value::{Array, Table, Value};

/// Configuration for the TOML serializer.
///
/// Obtain an instance via [`SerializeOptions::default`] and modify the public
/// fields before passing it to [`Document::serialize_with`] or
/// [`Document::write_file_with`].
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
    /// When `false` (the default), insertion order is preserved.
    pub sort_keys: bool,
    /// When `true`, every table is serialized as an inline table `{ … }`.
    /// When `false` (the default), only small leaf tables without sub-tables
    /// are written inline; larger tables use the `[header]` section style.
    pub prefer_inline: bool,
    /// The string prepended per indentation level when writing nested tables.
    /// Defaults to four spaces.
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
/// This is the underlying function called by [`Document::serialize`] and
/// [`Document::serialize_with`].
pub fn serialize(doc: &Document, opts: &SerializeOptions) -> String {
    let ser = Serializer { opts };
    ser.serialize(doc)
}

struct Serializer<'opts> {
    opts: &'opts SerializeOptions,
}

impl<'opts> Serializer<'opts> {
    fn serialize(&self, doc: &Document) -> String {
        let mut out = String::new();
        self.write_table_contents(&mut out, doc.root(), "", 0, true);
        if self.opts.trailing_newline && !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }

    /// Write the contents of a table.
    /// `is_root`: if true, this is the document root.
    /// `prefix`: dot-separated path prefix for sub-table headers.
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

        // First pass: write scalar values and inline tables/arrays
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

        // Second pass: write sub-tables and arrays-of-tables
        for &key in &deferred {
            let val = &table[key];
            let subpath = if prefix.is_empty() {
                key_to_string(key)
            } else {
                format!("{}.{}", prefix, key_to_string(key))
            };
            match val {
                Value::Table(t) => {
                    // Write [header]
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
            Value::String(s) => self.write_string(out, s),
            Value::Integer(n) => out.push_str(&n.to_string()),
            Value::Float(f) => self.write_float(out, *f),
            Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::OffsetDateTime(dt) => out.push_str(&dt.to_string()),
            Value::LocalDateTime(dt) => out.push_str(&dt.to_string()),
            Value::LocalDate(d) => out.push_str(&d.to_string()),
            Value::LocalTime(t) => out.push_str(&t.to_string()),
            Value::Array(arr) => self.write_array(out, arr, depth),
            Value::Table(tbl) => self.write_inline_table(out, tbl),
        }
    }

    fn write_float(&self, out: &mut String, f: f64) {
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
            // Ensure there's a '.' or 'e' in the output
            if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                out.push_str(&s);
                out.push_str(".0");
            } else {
                out.push_str(&s);
            }
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

    fn write_inline_table(&self, out: &mut String, tbl: &Table) {
        out.push('{');
        let keys: Vec<&str> = if self.opts.sort_keys {
            let mut ks: Vec<&str> = tbl.keys().collect();
            ks.sort_unstable();
            ks
        } else {
            tbl.keys().collect()
        };
        for (i, &key) in keys.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.write_key(out, key);
            out.push_str(" = ");
            self.write_value(out, &tbl[key], 0);
        }
        out.push('}');
    }

    fn write_string(&self, out: &mut String, s: &str) {
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

    fn write_key(&self, out: &mut String, key: &str) {
        if needs_quoting(key) {
            // Use basic string for keys with special chars
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

    /// Returns true if the table should be written inline.
    fn prefer_inline_table(&self, tbl: &Table) -> bool {
        if self.opts.prefer_inline {
            return true;
        }
        // Inline if small (≤ 4 entries) and no sub-tables
        tbl.len() <= 4 && !has_sub_tables(tbl)
    }
}

fn needs_quoting(key: &str) -> bool {
    if key.is_empty() {
        return true;
    }
    !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn has_sub_tables(tbl: &Table) -> bool {
    tbl.inner.values().any(|v| matches!(v, Value::Table(_) | Value::Array(_)))
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
