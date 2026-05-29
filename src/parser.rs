//! TOML 1.1 recursive-descent parser.
//!
//! The [`Parser`] struct is the low-level entry point.  Most callers should
//! use the higher-level [`Document::parse`] / [`Document::parse_file`] instead.

use std::collections::HashMap;

use crate::cst::{
    ArrayElement, ArrayNode, DocumentItem, EntryNode, InlineEntry, InlineTableNode, SectionNode,
    ValueNode,
};
use crate::datetime::{LocalDate, LocalDateTime, LocalTime, OffsetDateTime};
use crate::document::Document;
use crate::error::{TomlError, TomlErrorKind};
use crate::value::{Array, Table, Value};

// ── ParseContext ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableStatus {
    ExplicitlyDefined,
    ImplicitlyCreated,
    Inline,
    ArrayElement,
}

struct ParseContext {
    known: HashMap<String, TableStatus>,
}

impl ParseContext {
    fn new() -> Self {
        Self {
            known: HashMap::new(),
        }
    }

    fn is_known(&self, path: &str) -> bool {
        self.known.contains_key(path)
    }

    fn get_status(&self, path: &str) -> Option<TableStatus> {
        self.known.get(path).copied()
    }

    fn mark(
        &mut self,
        path: &str,
        status: TableStatus,
        line: u32,
        col: u32,
    ) -> Result<(), TomlError> {
        if let Some(existing) = self.known.get(path) {
            match (existing, &status) {
                (TableStatus::ImplicitlyCreated, TableStatus::ExplicitlyDefined) => {
                    self.known.insert(path.to_string(), status);
                    return Ok(());
                }
                (TableStatus::ArrayElement, TableStatus::ArrayElement) => {
                    return Ok(());
                }
                (TableStatus::Inline, _) => {
                    return Err(TomlError::parse(
                        format!("cannot extend inline table '{}' with a table header", path),
                        line,
                        col,
                    ));
                }
                (TableStatus::ExplicitlyDefined, TableStatus::ExplicitlyDefined) => {
                    return Err(TomlError {
                        kind: TomlErrorKind::DuplicateKey,
                        message: format!("table '{}' is defined more than once", path),
                        location: Some(crate::error::SourceLocation {
                            line,
                            column: col,
                            source_file: None,
                        }),
                    });
                }
                _ => {
                    return Err(TomlError {
                        kind: TomlErrorKind::DuplicateKey,
                        message: format!("table '{}' conflicts with existing definition", path),
                        location: Some(crate::error::SourceLocation {
                            line,
                            column: col,
                            source_file: None,
                        }),
                    });
                }
            }
        }
        self.known.insert(path.to_string(), status);
        Ok(())
    }
}

// ── Source helper ─────────────────────────────────────────────────────────────

struct Source<'src> {
    src: &'src str,
    pos: usize,
    line: u32,
    col: u32,
}

impl<'src> Source<'src> {
    fn new(src: &'src str) -> Self {
        Self { src, pos: 0, line: 1, col: 1 }
    }

    fn remaining(&self) -> &'src str {
        &self.src[self.pos..]
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn current_byte(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn peek_byte(&self, offset: usize) -> Option<u8> {
        self.src.as_bytes().get(self.pos + offset).copied()
    }

    fn current_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn advance(&mut self) {
        if let Some(ch) = self.current_char() {
            self.pos += ch.len_utf8();
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
    }

    fn advance_bytes(&mut self, n: usize) {
        for _ in 0..n {
            self.advance();
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.current_byte(), Some(b' ') | Some(b'\t')) {
            self.advance();
        }
    }

    fn skip_ws_and_newlines(&mut self) {
        loop {
            match self.current_byte() {
                Some(b' ') | Some(b'\t') | Some(b'\n') => self.advance(),
                Some(b'\r') if self.peek_byte(1) == Some(b'\n') => {
                    self.advance();
                    self.advance();
                }
                Some(b'#') => self.skip_comment(),
                _ => break,
            }
        }
    }

    fn skip_comment(&mut self) {
        while !self.is_eof() {
            let b = self.current_byte();
            if b == Some(b'\n') {
                break;
            }
            if b == Some(b'\r') && self.peek_byte(1) == Some(b'\n') {
                break;
            }
            self.advance();
        }
    }

    fn skip_ws_comment(&mut self) {
        self.skip_ws();
        if self.current_byte() == Some(b'#') {
            self.skip_comment();
        }
    }

    fn skip_ws_nl_comments(&mut self) {
        loop {
            match self.current_byte() {
                Some(b' ') | Some(b'\t') | Some(b'\n') => self.advance(),
                Some(b'\r') if self.peek_byte(1) == Some(b'\n') => {
                    self.advance();
                    self.advance();
                }
                Some(b'#') => self.skip_comment(),
                _ => break,
            }
        }
    }

    fn err_here(&self, msg: impl Into<String>) -> TomlError {
        TomlError::parse(msg, self.line, self.col)
    }

    /// Source text consumed since `start`.
    fn slice_from(&self, start: usize) -> &'src str {
        &self.src[start..self.pos]
    }
}

// ── KeyvalInfo ────────────────────────────────────────────────────────────────

struct KeyvalInfo {
    keys: Vec<String>,
    is_inline_table: bool,
    raw_key: String,
    pre_eq: String,
    post_eq: String,
    value_node: ValueNode,
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Low-level TOML 1.1 parser.
pub struct Parser<'src> {
    src: Source<'src>,
    ctx: ParseContext,
}

impl<'src> Parser<'src> {
    /// Create a new `Parser` for the given TOML source string.
    pub fn new(src: &'src str) -> Self {
        Self { src: Source::new(src), ctx: ParseContext::new() }
    }

    /// Run the parser and return the resulting [`Document`].
    pub fn parse(mut self) -> Result<Document, TomlError> {
        let mut root = Table::new();
        let mut items: Vec<DocumentItem> = Vec::new();
        self.parse_document(&mut root, &mut items)?;
        Ok(Document::from_parts(root, items))
    }

    fn parse_document(
        &mut self,
        root: &mut Table,
        items: &mut Vec<DocumentItem>,
    ) -> Result<(), TomlError> {
        let mut current_dom_path: Vec<String> = vec![];
        let mut current_item_prefix: Vec<String> = vec![];
        let mut aot_counts: HashMap<String, usize> = HashMap::new();

        loop {
            let leading_start = self.src.pos;
            self.src.skip_ws_and_newlines();
            let leading = self.src.slice_from(leading_start).to_string();

            if self.src.is_eof() {
                items.push(DocumentItem::Eof(leading));
                break;
            }

            match self.src.current_byte() {
                Some(b'[') => {
                    if self.src.peek_byte(1) == Some(b'[') {
                        // [[array-of-tables]]
                        let header_start = self.src.pos;
                        self.src.advance();
                        self.src.advance();
                        self.src.skip_ws();
                        let path = self.parse_key()?;
                        self.src.skip_ws();
                        self.expect_byte(b']', "expected ']]'")?;
                        self.expect_byte(b']', "expected ']]'")?;
                        let raw = self.src.slice_from(header_start).to_string();

                        let trailing_start = self.src.pos;
                        self.src.skip_ws_comment();
                        self.expect_newline_or_eof()?;
                        let trailing = self.src.slice_from(trailing_start).to_string();

                        let path_str = path.join(".");
                        self.ctx.mark(
                            &path_str,
                            TableStatus::ArrayElement,
                            self.src.line,
                            self.src.col,
                        )?;
                        let arr = get_or_create_array_of_tables(
                            root,
                            &path,
                            &mut self.ctx,
                            self.src.line,
                            self.src.col,
                        )?;
                        arr.0.push(Value::Table(Table::new()));

                        let idx = {
                            let c = aot_counts.entry(path_str.clone()).or_insert(0);
                            let i = *c;
                            *c += 1;
                            i
                        };

                        current_dom_path = path.clone();
                        current_item_prefix = {
                            let mut p = path.clone();
                            p.push(idx.to_string());
                            p
                        };

                        items.push(DocumentItem::Section(SectionNode {
                            leading,
                            raw,
                            trailing,
                            path,
                            is_array: true,
                        }));
                    } else {
                        // [table]
                        let header_start = self.src.pos;
                        self.src.advance();
                        self.src.skip_ws();
                        let path = self.parse_key()?;
                        self.src.skip_ws();
                        self.expect_byte(b']', "expected ']'")?;
                        let raw = self.src.slice_from(header_start).to_string();

                        let trailing_start = self.src.pos;
                        self.src.skip_ws_comment();
                        self.expect_newline_or_eof()?;
                        let trailing = self.src.slice_from(trailing_start).to_string();

                        let path_str = path.join(".");
                        let line = self.src.line;
                        let col = self.src.col;
                        self.ctx.mark(&path_str, TableStatus::ExplicitlyDefined, line, col)?;
                        ensure_path_exists(root, &path, &mut self.ctx, line, col)?;
                        current_dom_path = path.clone();
                        current_item_prefix = path.clone();

                        items.push(DocumentItem::Section(SectionNode {
                            leading,
                            raw,
                            trailing,
                            path,
                            is_array: false,
                        }));
                    }
                }
                _ => {
                    // key = value
                    let line = self.src.line;
                    let col = self.src.col;
                    let target = navigate_to_table_mut(
                        root,
                        &current_dom_path,
                        &mut self.ctx,
                        line,
                        col,
                    )?;
                    let info = self.parse_keyval(target)?;

                    if info.is_inline_table {
                        let mut full = if current_dom_path.is_empty() {
                            String::new()
                        } else {
                            current_dom_path.join(".")
                        };
                        if !full.is_empty() {
                            full.push('.');
                        }
                        full.push_str(&info.keys.join("."));
                        let _ = self.ctx.mark(&full, TableStatus::Inline, line, col);
                    }

                    let trailing_start = self.src.pos;
                    self.src.skip_ws_comment();
                    self.expect_newline_or_eof()?;
                    let trailing = self.src.slice_from(trailing_start).to_string();

                    let mut full_path = current_item_prefix.clone();
                    full_path.extend(info.keys.iter().cloned());

                    items.push(DocumentItem::Entry {
                        node: EntryNode {
                            leading,
                            raw_key: info.raw_key,
                            pre_eq: info.pre_eq,
                            post_eq: info.post_eq,
                            node: info.value_node,
                            trailing,
                        },
                        path: full_path,
                    });
                }
            }
        }
        Ok(())
    }

    fn parse_keyval(&mut self, target: &mut Table) -> Result<KeyvalInfo, TomlError> {
        let line = self.src.line;
        let col = self.src.col;

        let key_start = self.src.pos;
        let keys = self.parse_key()?;
        // parse_key() consumes trailing whitespace when peeking for '.' in dotted keys.
        // Split raw_key (no trailing ws) from the ws that belongs in pre_eq.
        let raw_key_with_trailing = self.src.slice_from(key_start);
        let trim_len = raw_key_with_trailing.trim_end().len();
        let raw_key = raw_key_with_trailing[..trim_len].to_string();
        let already_consumed_ws = raw_key_with_trailing[trim_len..].to_string();

        // pre_eq = whitespace already consumed by parse_key + any further ws before '='
        let extra_ws_start = self.src.pos;
        self.src.skip_ws();
        let pre_eq = format!("{}{}", already_consumed_ws, self.src.slice_from(extra_ws_start));

        self.expect_byte(b'=', "expected '='")?;

        let post_eq_start = self.src.pos;
        self.src.skip_ws();
        let post_eq = self.src.slice_from(post_eq_start).to_string();

        let (value, value_node) = self.parse_val()?;
        let is_inline_table = matches!(value, Value::Table(_));

        if keys.len() == 1 {
            if target.contains_key(&keys[0]) {
                return Err(TomlError {
                    kind: TomlErrorKind::DuplicateKey,
                    message: format!("duplicate key '{}'", keys[0]),
                    location: Some(crate::error::SourceLocation {
                        line,
                        column: col,
                        source_file: None,
                    }),
                });
            }
            target.inner.insert(keys[0].clone(), value);
        } else {
            let mut current = target;
            for (i, key) in keys.iter().enumerate() {
                if i == keys.len() - 1 {
                    if current.contains_key(key) {
                        return Err(TomlError {
                            kind: TomlErrorKind::DuplicateKey,
                            message: format!("duplicate key '{}'", key),
                            location: Some(crate::error::SourceLocation {
                                line,
                                column: col,
                                source_file: None,
                            }),
                        });
                    }
                    current.inner.insert(key.clone(), value);
                    break;
                } else {
                    let entry = current
                        .inner
                        .entry(key.clone())
                        .or_insert_with(|| Value::Table(Table::new()));
                    match entry {
                        Value::Table(t) => current = t,
                        _ => {
                            return Err(TomlError::parse(
                                format!("key '{}' is not a table", key),
                                line,
                                col,
                            ));
                        }
                    }
                }
            }
        }

        Ok(KeyvalInfo { keys, is_inline_table, raw_key, pre_eq, post_eq, value_node })
    }

    fn parse_key(&mut self) -> Result<Vec<String>, TomlError> {
        let mut keys = vec![self.parse_simple_key()?];
        loop {
            self.src.skip_ws();
            if self.src.current_byte() == Some(b'.') {
                self.src.advance();
                self.src.skip_ws();
                keys.push(self.parse_simple_key()?);
            } else {
                break;
            }
        }
        Ok(keys)
    }

    fn parse_simple_key(&mut self) -> Result<String, TomlError> {
        match self.src.current_byte() {
            Some(b'"') => self.parse_basic_string(),
            Some(b'\'') => self.parse_literal_string(),
            Some(b) if is_bare_key_char(b) => {
                let start = self.src.pos;
                while self.src.current_byte().map_or(false, is_bare_key_char) {
                    self.src.advance();
                }
                Ok(self.src.src[start..self.src.pos].to_string())
            }
            _ => Err(self.src.err_here("expected key")),
        }
    }

    /// Parse a value and return both the semantic [`Value`] and its [`ValueNode`].
    fn parse_val(&mut self) -> Result<(Value, ValueNode), TomlError> {
        let val_start = self.src.pos;

        match self.src.current_byte() {
            Some(b'"') => {
                let s = if self.src.peek_byte(1) == Some(b'"')
                    && self.src.peek_byte(2) == Some(b'"')
                {
                    self.parse_ml_basic_string()?
                } else {
                    self.parse_basic_string()?
                };
                let raw = self.src.slice_from(val_start).to_string();
                let v = Value::String(s);
                Ok((v.clone(), ValueNode::Scalar { raw: Some(raw), value: v }))
            }
            Some(b'\'') => {
                let s = if self.src.peek_byte(1) == Some(b'\'')
                    && self.src.peek_byte(2) == Some(b'\'')
                {
                    self.parse_ml_literal_string()?
                } else {
                    self.parse_literal_string()?
                };
                let raw = self.src.slice_from(val_start).to_string();
                let v = Value::String(s);
                Ok((v.clone(), ValueNode::Scalar { raw: Some(raw), value: v }))
            }
            Some(b't') => {
                if self.src.remaining().starts_with("true") {
                    self.src.advance_bytes(4);
                    let raw = self.src.slice_from(val_start).to_string();
                    Ok((Value::Boolean(true), ValueNode::Scalar { raw: Some(raw), value: Value::Boolean(true) }))
                } else {
                    Err(self.src.err_here("expected 'true'"))
                }
            }
            Some(b'f') => {
                if self.src.remaining().starts_with("false") {
                    self.src.advance_bytes(5);
                    let raw = self.src.slice_from(val_start).to_string();
                    Ok((Value::Boolean(false), ValueNode::Scalar { raw: Some(raw), value: Value::Boolean(false) }))
                } else {
                    Err(self.src.err_here("expected 'false'"))
                }
            }
            Some(b'[') => {
                let (arr, arr_node) = self.parse_array()?;
                Ok((Value::Array(arr), ValueNode::Array(arr_node)))
            }
            Some(b'{') => {
                let (tbl, tbl_node) = self.parse_inline_table()?;
                Ok((Value::Table(tbl), ValueNode::InlineTable(tbl_node)))
            }
            Some(b'i') => {
                if self.src.remaining().starts_with("inf") {
                    self.src.advance_bytes(3);
                    let raw = self.src.slice_from(val_start).to_string();
                    Ok((Value::Float(f64::INFINITY), ValueNode::Scalar { raw: Some(raw), value: Value::Float(f64::INFINITY) }))
                } else {
                    Err(self.src.err_here("expected 'inf'"))
                }
            }
            Some(b'n') => {
                if self.src.remaining().starts_with("nan") {
                    self.src.advance_bytes(3);
                    let raw = self.src.slice_from(val_start).to_string();
                    Ok((Value::Float(f64::NAN), ValueNode::Scalar { raw: Some(raw), value: Value::Float(f64::NAN) }))
                } else {
                    Err(self.src.err_here("expected 'nan'"))
                }
            }
            Some(b'+') | Some(b'-') => {
                let sign = self.src.current_byte().unwrap() as char;
                self.src.advance();
                if self.src.remaining().starts_with("inf") {
                    self.src.advance_bytes(3);
                    let raw = self.src.slice_from(val_start).to_string();
                    let v = Value::Float(if sign == '-' { f64::NEG_INFINITY } else { f64::INFINITY });
                    return Ok((v.clone(), ValueNode::Scalar { raw: Some(raw), value: v }));
                }
                if self.src.remaining().starts_with("nan") {
                    self.src.advance_bytes(3);
                    let raw = self.src.slice_from(val_start).to_string();
                    return Ok((Value::Float(f64::NAN), ValueNode::Scalar { raw: Some(raw), value: Value::Float(f64::NAN) }));
                }
                let start = self.src.pos - 1;
                let v = self.parse_number_from(start, sign == '-')?;
                let raw = self.src.slice_from(val_start).to_string();
                Ok((v.clone(), ValueNode::Scalar { raw: Some(raw), value: v }))
            }
            Some(b) if b.is_ascii_digit() => {
                let start = self.src.pos;
                let v = self.parse_number_from(start, false)?;
                let raw = self.src.slice_from(val_start).to_string();
                Ok((v.clone(), ValueNode::Scalar { raw: Some(raw), value: v }))
            }
            _ => Err(self.src.err_here("unexpected character")),
        }
    }

    fn parse_number_from(&mut self, start: usize, negated: bool) -> Result<Value, TomlError> {
        let line = self.src.line;
        let col = self.src.col;
        let _ = negated;
        let tok_start = start;
        let after_sign_pos = self.src.pos;

        if self.src.remaining().starts_with("0x") {
            self.src.advance();
            self.src.advance();
            let digit_start = self.src.pos;
            while self.src.current_byte().map_or(false, |b| b.is_ascii_hexdigit() || b == b'_') {
                self.src.advance();
            }
            let digits: String = self.src.src[digit_start..self.src.pos]
                .chars()
                .filter(|&c| c != '_')
                .collect();
            if digits.is_empty() {
                return Err(TomlError::parse("empty hex integer", line, col));
            }
            let val = u64::from_str_radix(&digits, 16)
                .map_err(|_| TomlError::integer_overflow("integer overflow (hex)", line, col))?;
            let negative = self.src.src[tok_start..self.src.pos].starts_with('-');
            return parse_int_result(val, negative, "hex", line, col);
        }

        if self.src.remaining().starts_with("0o") {
            self.src.advance();
            self.src.advance();
            let digit_start = self.src.pos;
            while self.src.current_byte().map_or(false, |b| matches!(b, b'0'..=b'7') || b == b'_') {
                self.src.advance();
            }
            let digits: String = self.src.src[digit_start..self.src.pos]
                .chars()
                .filter(|&c| c != '_')
                .collect();
            if digits.is_empty() {
                return Err(TomlError::parse("empty octal integer", line, col));
            }
            let val = u64::from_str_radix(&digits, 8)
                .map_err(|_| TomlError::integer_overflow("integer overflow (octal)", line, col))?;
            let negative = self.src.src[tok_start..self.src.pos].starts_with('-');
            return parse_int_result(val, negative, "octal", line, col);
        }

        if self.src.remaining().starts_with("0b") {
            self.src.advance();
            self.src.advance();
            let digit_start = self.src.pos;
            while self.src.current_byte().map_or(false, |b| matches!(b, b'0' | b'1') || b == b'_') {
                self.src.advance();
            }
            let digits: String = self.src.src[digit_start..self.src.pos]
                .chars()
                .filter(|&c| c != '_')
                .collect();
            if digits.is_empty() {
                return Err(TomlError::parse("empty binary integer", line, col));
            }
            let val = u64::from_str_radix(&digits, 2)
                .map_err(|_| TomlError::integer_overflow("integer overflow (binary)", line, col))?;
            let negative = self.src.src[tok_start..self.src.pos].starts_with('-');
            return parse_int_result(val, negative, "binary", line, col);
        }

        // Decimal integer, float, or datetime
        while let Some(b) = self.src.current_byte() {
            match b {
                b'0'..=b'9' | b'.' | b'e' | b'E' | b'_' | b':' | b'-' | b'+' | b'T' | b't'
                | b'Z' | b'z' | b' ' => {
                    if b == b' ' {
                        let so_far = &self.src.src[after_sign_pos..self.src.pos];
                        if looks_like_date(so_far) {
                            if let Some(next) = self.src.peek_byte(1) {
                                if next.is_ascii_digit() {
                                    self.src.advance();
                                    continue;
                                }
                            }
                        }
                        break;
                    }
                    self.src.advance();
                }
                _ => break,
            }
        }

        let token_str = &self.src.src[tok_start..self.src.pos];

        if let Some(v) = try_parse_datetime(token_str, line, col)? {
            return Ok(v);
        }

        let signed = token_str.replace('_', "");
        if (signed.contains('.') || signed.contains('e') || signed.contains('E'))
            && !signed.starts_with("0x")
            && !signed.starts_with("0o")
            && !signed.starts_with("0b")
        {
            let f: f64 = signed
                .parse()
                .map_err(|_| TomlError::parse(format!("invalid float: '{}'", signed), line, col))?;
            return Ok(Value::Float(f));
        }

        let signed_clean = token_str.replace('_', "");
        let digit_part = signed_clean.trim_start_matches(['+', '-']);
        if !digit_part.chars().all(|c| c.is_ascii_digit()) {
            return Err(TomlError::parse(
                format!("invalid integer: '{}'", token_str),
                line,
                col,
            ));
        }
        let val: i64 = signed_clean.parse().map_err(|_| {
            TomlError::integer_overflow(
                format!("integer overflow (decimal): '{}'", signed_clean),
                line,
                col,
            )
        })?;
        Ok(Value::Integer(val))
    }

    fn parse_basic_string(&mut self) -> Result<String, TomlError> {
        let line = self.src.line;
        let col = self.src.col;
        self.expect_byte(b'"', "expected '\"'")?;
        let mut result = String::new();
        loop {
            match self.src.current_byte() {
                None => return Err(TomlError::parse("unterminated string", line, col)),
                Some(b'"') => { self.src.advance(); break; }
                Some(b'\\') => {
                    self.src.advance();
                    result.push_str(&self.parse_escape_sequence()?);
                }
                Some(b'\n') | Some(b'\r') => {
                    return Err(TomlError::parse("newline in basic string", line, col))
                }
                _ => {
                    if let Some(ch) = self.src.current_char() {
                        result.push(ch);
                        self.src.advance();
                    }
                }
            }
        }
        Ok(result)
    }

    fn parse_ml_basic_string(&mut self) -> Result<String, TomlError> {
        let line = self.src.line;
        let col = self.src.col;
        self.expect_byte(b'"', "expected '\"'")?;
        self.expect_byte(b'"', "expected '\"'")?;
        self.expect_byte(b'"', "expected '\"'")?;
        if self.src.current_byte() == Some(b'\n') {
            self.src.advance();
        } else if self.src.current_byte() == Some(b'\r') && self.src.peek_byte(1) == Some(b'\n') {
            self.src.advance();
            self.src.advance();
        }
        let mut result = String::new();
        loop {
            match self.src.current_byte() {
                None => return Err(TomlError::parse("unterminated multiline string", line, col)),
                Some(b'"') => {
                    if self.src.peek_byte(1) == Some(b'"') && self.src.peek_byte(2) == Some(b'"') {
                        self.src.advance();
                        self.src.advance();
                        self.src.advance();
                        let mut extra = 0;
                        while self.src.current_byte() == Some(b'"') && extra < 2 {
                            result.push('"');
                            self.src.advance();
                            extra += 1;
                        }
                        break;
                    } else {
                        result.push('"');
                        self.src.advance();
                    }
                }
                Some(b'\\') => {
                    self.src.advance();
                    if self.src.current_byte() == Some(b'\n')
                        || (self.src.current_byte() == Some(b'\r')
                            && self.src.peek_byte(1) == Some(b'\n'))
                    {
                        if self.src.current_byte() == Some(b'\r') {
                            self.src.advance();
                        }
                        self.src.advance();
                        while matches!(
                            self.src.current_byte(),
                            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
                        ) {
                            self.src.advance();
                        }
                        continue;
                    }
                    result.push_str(&self.parse_escape_sequence()?);
                }
                _ => {
                    if let Some(ch) = self.src.current_char() {
                        result.push(ch);
                        self.src.advance();
                    }
                }
            }
        }
        Ok(result)
    }

    fn parse_literal_string(&mut self) -> Result<String, TomlError> {
        let line = self.src.line;
        let col = self.src.col;
        self.expect_byte(b'\'', "expected \"'\"")?;
        let mut result = String::new();
        loop {
            match self.src.current_byte() {
                None => return Err(TomlError::parse("unterminated literal string", line, col)),
                Some(b'\'') => { self.src.advance(); break; }
                Some(b'\n') | Some(b'\r') => {
                    return Err(TomlError::parse("newline in literal string", line, col))
                }
                _ => {
                    if let Some(ch) = self.src.current_char() {
                        result.push(ch);
                        self.src.advance();
                    }
                }
            }
        }
        Ok(result)
    }

    fn parse_ml_literal_string(&mut self) -> Result<String, TomlError> {
        let line = self.src.line;
        let col = self.src.col;
        self.expect_byte(b'\'', "expected \"'\"")?;
        self.expect_byte(b'\'', "expected \"'\"")?;
        self.expect_byte(b'\'', "expected \"'\"")?;
        if self.src.current_byte() == Some(b'\n') {
            self.src.advance();
        } else if self.src.current_byte() == Some(b'\r') && self.src.peek_byte(1) == Some(b'\n') {
            self.src.advance();
            self.src.advance();
        }
        let mut result = String::new();
        loop {
            match self.src.current_byte() {
                None => return Err(TomlError::parse("unterminated multiline literal string", line, col)),
                Some(b'\'') => {
                    if self.src.peek_byte(1) == Some(b'\'') && self.src.peek_byte(2) == Some(b'\'')
                    {
                        self.src.advance();
                        self.src.advance();
                        self.src.advance();
                        let mut extra = 0;
                        while self.src.current_byte() == Some(b'\'') && extra < 2 {
                            result.push('\'');
                            self.src.advance();
                            extra += 1;
                        }
                        break;
                    } else {
                        result.push('\'');
                        self.src.advance();
                    }
                }
                _ => {
                    if let Some(ch) = self.src.current_char() {
                        result.push(ch);
                        self.src.advance();
                    }
                }
            }
        }
        Ok(result)
    }

    fn parse_escape_sequence(&mut self) -> Result<String, TomlError> {
        let line = self.src.line;
        let col = self.src.col;
        match self.src.current_byte() {
            Some(b'b') => { self.src.advance(); Ok("\x08".to_string()) }
            Some(b't') => { self.src.advance(); Ok("\t".to_string()) }
            Some(b'n') => { self.src.advance(); Ok("\n".to_string()) }
            Some(b'f') => { self.src.advance(); Ok("\x0C".to_string()) }
            Some(b'r') => { self.src.advance(); Ok("\r".to_string()) }
            Some(b'"') => { self.src.advance(); Ok("\"".to_string()) }
            Some(b'\\') => { self.src.advance(); Ok("\\".to_string()) }
            Some(b'e') => { self.src.advance(); Ok("\x1B".to_string()) }
            Some(b'x') => {
                self.src.advance();
                let h1 = self.read_hex_digit()?;
                let h2 = self.read_hex_digit()?;
                let code = (h1 << 4) | h2;
                let ch = char::from_u32(code as u32).ok_or_else(|| TomlError {
                    kind: TomlErrorKind::InvalidEscape(format!("\\x{:02x}", code)),
                    message: format!("invalid \\x escape: {:02x}", code),
                    location: Some(crate::error::SourceLocation { line, column: col, source_file: None }),
                })?;
                Ok(ch.to_string())
            }
            Some(b'u') => {
                self.src.advance();
                let code = self.read_hex_digits(4)?;
                let ch = char::from_u32(code).ok_or_else(|| TomlError {
                    kind: TomlErrorKind::InvalidEscape(format!("\\u{:04X}", code)),
                    message: format!("invalid Unicode escape: \\u{:04X}", code),
                    location: Some(crate::error::SourceLocation { line, column: col, source_file: None }),
                })?;
                Ok(ch.to_string())
            }
            Some(b'U') => {
                self.src.advance();
                let code = self.read_hex_digits(8)?;
                let ch = char::from_u32(code).ok_or_else(|| TomlError {
                    kind: TomlErrorKind::InvalidEscape(format!("\\U{:08X}", code)),
                    message: format!("invalid Unicode escape: \\U{:08X}", code),
                    location: Some(crate::error::SourceLocation { line, column: col, source_file: None }),
                })?;
                Ok(ch.to_string())
            }
            Some(other) => Err(TomlError {
                kind: TomlErrorKind::InvalidEscape(format!("\\{}", other as char)),
                message: format!("invalid escape sequence: \\{}", other as char),
                location: Some(crate::error::SourceLocation { line, column: col, source_file: None }),
            }),
            None => Err(TomlError::parse("unexpected EOF in escape sequence", line, col)),
        }
    }

    fn read_hex_digit(&mut self) -> Result<u8, TomlError> {
        let b = self.src.current_byte().ok_or_else(|| self.src.err_here("expected hex digit"))?;
        let val = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return Err(self.src.err_here(format!("expected hex digit, got '{}'", b as char))),
        };
        self.src.advance();
        Ok(val)
    }

    fn read_hex_digits(&mut self, count: usize) -> Result<u32, TomlError> {
        let mut val: u32 = 0;
        for _ in 0..count {
            val = (val << 4) | self.read_hex_digit()? as u32;
        }
        Ok(val)
    }

    /// Parse an array `[…]`, returning both the semantic [`Array`] and the
    /// format-preserving [`ArrayNode`].
    fn parse_array(&mut self) -> Result<(Array, ArrayNode), TomlError> {
        let line = self.src.line;
        let col = self.src.col;

        let open_start = self.src.pos;
        self.expect_byte(b'[', "expected '['")?;
        let open = self.src.slice_from(open_start).to_string();

        let mut arr = Array::new();
        let mut elements: Vec<ArrayElement> = Vec::new();
        let close: String;

        loop {
            // Capture leading trivia (or pre-close whitespace)
            let leading_start = self.src.pos;
            self.src.skip_ws_nl_comments();
            let leading = self.src.slice_from(leading_start).to_string();

            if self.src.current_byte() == Some(b']') {
                close = format!("{}]", leading);
                self.src.advance();
                break;
            }
            if self.src.is_eof() {
                return Err(TomlError::parse("unterminated array", line, col));
            }

            let (val, val_node) = self.parse_val()?;
            arr.0.push(val);

            // Trailing whitespace between value and comma/close
            let trailing_start = self.src.pos;
            self.src.skip_ws_nl_comments();
            let trailing = self.src.slice_from(trailing_start).to_string();

            match self.src.current_byte() {
                Some(b',') => {
                    self.src.advance();
                    elements.push(ArrayElement {
                        leading,
                        node: val_node,
                        trailing,
                        comma: Some(",".to_string()),
                    });
                }
                Some(b']') | None => {
                    elements.push(ArrayElement {
                        leading,
                        node: val_node,
                        trailing,
                        comma: None,
                    });
                    // `]` (or EOF) will be handled in the next iteration
                }
                _ => {
                    return Err(TomlError::parse(
                        "expected ',' or ']' in array",
                        self.src.line,
                        self.src.col,
                    ));
                }
            }
        }

        Ok((arr, ArrayNode { open, elements, close }))
    }

    /// Parse an inline table `{…}`, returning both the semantic [`Table`] and
    /// the format-preserving [`InlineTableNode`].
    fn parse_inline_table(&mut self) -> Result<(Table, InlineTableNode), TomlError> {
        let line = self.src.line;
        let col = self.src.col;

        let open_start = self.src.pos;
        self.expect_byte(b'{', "expected '{'")?;
        let open = self.src.slice_from(open_start).to_string();

        let mut tbl = Table::new();
        let mut entries: Vec<InlineEntry> = Vec::new();
        let close: String;

        loop {
            // Leading trivia before key (or pre-close whitespace)
            let leading_start = self.src.pos;
            self.src.skip_ws_nl_comments();
            let leading = self.src.slice_from(leading_start).to_string();

            if self.src.current_byte() == Some(b'}') {
                close = format!("{}}}", leading);
                self.src.advance();
                break;
            }
            if self.src.is_eof() {
                return Err(TomlError::parse("unterminated inline table", line, col));
            }

            let info = self.parse_keyval(&mut tbl)?;

            // Trailing whitespace after value
            let trailing_start = self.src.pos;
            self.src.skip_ws_nl_comments();
            let trailing = self.src.slice_from(trailing_start).to_string();

            match self.src.current_byte() {
                Some(b',') => {
                    self.src.advance();
                    entries.push(InlineEntry {
                        leading,
                        raw_key: info.raw_key,
                        pre_eq: info.pre_eq,
                        post_eq: info.post_eq,
                        node: info.value_node,
                        trailing,
                        comma: Some(",".to_string()),
                    });
                }
                Some(b'}') | None => {
                    entries.push(InlineEntry {
                        leading,
                        raw_key: info.raw_key,
                        pre_eq: info.pre_eq,
                        post_eq: info.post_eq,
                        node: info.value_node,
                        trailing,
                        comma: None,
                    });
                    // `}` will be consumed in the next iteration
                }
                _ => {
                    return Err(TomlError::parse(
                        "expected ',' or '}' in inline table",
                        self.src.line,
                        self.src.col,
                    ));
                }
            }
        }

        Ok((tbl, InlineTableNode { open, entries, close }))
    }

    fn expect_byte(&mut self, expected: u8, msg: &str) -> Result<(), TomlError> {
        if self.src.current_byte() == Some(expected) {
            self.src.advance();
            Ok(())
        } else {
            Err(self.src.err_here(msg))
        }
    }

    fn expect_newline_or_eof(&mut self) -> Result<(), TomlError> {
        match self.src.current_byte() {
            None => Ok(()),
            Some(b'\n') => { self.src.advance(); Ok(()) }
            Some(b'\r') if self.src.peek_byte(1) == Some(b'\n') => {
                self.src.advance();
                self.src.advance();
                Ok(())
            }
            _ => Err(self.src.err_here("expected newline or EOF")),
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Parse a TOML 1.1 source string and return a [`Document`].
pub fn parse(src: &str) -> Result<Document, TomlError> {
    Parser::new(src).parse()
}

// ── Helper functions ──────────────────────────────────────────────────────────

fn is_bare_key_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn looks_like_date(s: &str) -> bool {
    if s.len() < 10 { return false; }
    let b = s.as_bytes();
    b[4] == b'-' && b[7] == b'-'
}

/// Shared helper to convert a raw u64 integer to a signed Value,
/// handling sign and overflow.
fn parse_int_result(
    val: u64,
    negative: bool,
    label: &str,
    line: u32,
    col: u32,
) -> Result<Value, TomlError> {
    if negative {
        if val > 0x8000_0000_0000_0000u64 {
            return Err(TomlError::integer_overflow(
                format!("integer overflow ({} negative)", label),
                line,
                col,
            ));
        }
        if val == 0x8000_0000_0000_0000u64 {
            return Ok(Value::Integer(i64::MIN));
        }
        Ok(Value::Integer(-(val as i64)))
    } else {
        if val > i64::MAX as u64 {
            return Err(TomlError::integer_overflow(
                format!("integer overflow ({})", label),
                line,
                col,
            ));
        }
        Ok(Value::Integer(val as i64))
    }
}

fn try_parse_datetime(s: &str, line: u32, col: u32) -> Result<Option<Value>, TomlError> {
    let bytes = s.as_bytes();

    let is_time = s.len() >= 5
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b':'
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && (s.len() < 10 || bytes[4] != b'-');

    let is_date = s.len() >= 10
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && bytes[7] == b'-'
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit();

    if is_time && !is_date {
        let t = parse_time_str(s, line, col)?;
        return Ok(Some(Value::LocalTime(t)));
    }
    if !is_date { return Ok(None); }

    let date = parse_date_str(&s[..10], line, col)?;
    if s.len() == 10 { return Ok(Some(Value::LocalDate(date))); }

    let delim = bytes[10];
    if delim != b'T' && delim != b't' && delim != b' ' {
        return Ok(Some(Value::LocalDate(date)));
    }

    let time_and_offset = &s[11..];
    let (time_str, offset_str) = split_time_offset(time_and_offset);
    let time = parse_time_str(time_str, line, col)?;

    if let Some(off) = offset_str {
        let offset_minutes = parse_offset(off, line, col)?;
        Ok(Some(Value::OffsetDateTime(OffsetDateTime { date, time, offset_minutes })))
    } else {
        Ok(Some(Value::LocalDateTime(LocalDateTime { date, time })))
    }
}

fn parse_date_str(s: &str, line: u32, col: u32) -> Result<LocalDate, TomlError> {
    let bytes = s.as_bytes();
    if bytes.len() < 10 { return Err(TomlError::parse("invalid date", line, col)); }
    let year = parse_digits(&s[0..4], line, col)? as i32;
    let month = parse_digits(&s[5..7], line, col)? as u8;
    let day = parse_digits(&s[8..10], line, col)? as u8;
    Ok(LocalDate { year, month, day })
}

fn parse_time_str(s: &str, line: u32, col: u32) -> Result<LocalTime, TomlError> {
    let bytes = s.as_bytes();
    if bytes.len() < 5 { return Err(TomlError::parse("invalid time", line, col)); }
    let hour = parse_digits(&s[0..2], line, col)? as u8;
    if bytes[2] != b':' { return Err(TomlError::parse("invalid time (expected ':')", line, col)); }
    let minute = parse_digits(&s[3..5], line, col)? as u8;

    if bytes.len() <= 5 || bytes[5] != b':' {
        return Ok(LocalTime { hour, minute, second: 0, nanosecond: 0 });
    }
    if bytes.len() < 8 {
        return Err(TomlError::parse("invalid time: seconds field incomplete after ':'", line, col));
    }
    let second = parse_digits(&s[6..8], line, col)? as u8;
    let nanosecond = if bytes.len() > 8 && bytes[8] == b'.' {
        parse_fractional_seconds(&s[9..], line, col)?
    } else {
        0
    };
    Ok(LocalTime { hour, minute, second, nanosecond })
}

fn parse_fractional_seconds(s: &str, _line: u32, _col: u32) -> Result<u32, TomlError> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() { return Ok(0); }
    let padded = if digits.len() >= 9 { digits[..9].to_string() } else { format!("{:0<9}", digits) };
    Ok(padded.parse().unwrap_or(0))
}

fn split_time_offset(s: &str) -> (&str, Option<&str>) {
    for (i, &b) in s.as_bytes().iter().enumerate() {
        match b {
            b'Z' | b'z' => return (&s[..i], Some(&s[i..])),
            b'+' | b'-' if i > 0 => return (&s[..i], Some(&s[i..])),
            _ => {}
        }
    }
    (s, None)
}

fn parse_offset(s: &str, line: u32, col: u32) -> Result<i32, TomlError> {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return Err(TomlError::parse("empty offset", line, col)); }
    match bytes[0] {
        b'Z' | b'z' => return Ok(OffsetDateTime::UTC_OFFSET),
        b'+' | b'-' => {}
        _ => return Err(TomlError::parse("invalid offset", line, col)),
    }
    let sign = if bytes[0] == b'-' { -1i32 } else { 1i32 };
    if s.len() < 6 { return Err(TomlError::parse("invalid offset format", line, col)); }
    let hours = parse_digits(&s[1..3], line, col)? as i32;
    if bytes[3] != b':' { return Err(TomlError::parse("invalid offset (expected ':')", line, col)); }
    let minutes = parse_digits(&s[4..6], line, col)? as i32;
    Ok(sign * (hours * 60 + minutes))
}

fn parse_digits(s: &str, line: u32, col: u32) -> Result<u64, TomlError> {
    s.parse::<u64>()
        .map_err(|_| TomlError::parse(format!("invalid digits: '{}'", s), line, col))
}

fn navigate_to_table_mut<'a>(
    root: &'a mut Table,
    path: &[String],
    ctx: &mut ParseContext,
    line: u32,
    col: u32,
) -> Result<&'a mut Table, TomlError> {
    if path.is_empty() { return Ok(root); }
    let mut current = root;
    for (i, key) in path.iter().enumerate() {
        let partial_path = path[..=i].join(".");
        let entry = current.inner.get_mut(key.as_str()).ok_or_else(|| {
            TomlError::parse(format!("table '{}' not found", partial_path), line, col)
        })?;
        match entry {
            Value::Table(t) => current = t,
            Value::Array(arr) => {
                if let Some(Value::Table(t)) = arr.0.last_mut() {
                    current = t;
                } else {
                    return Err(TomlError::parse(
                        format!("cannot navigate into array '{}' (not an array of tables)", key),
                        line, col,
                    ));
                }
            }
            _ => return Err(TomlError::parse(format!("key '{}' is not a table", key), line, col)),
        }
        if ctx.get_status(&partial_path) == Some(TableStatus::Inline) {
            return Err(TomlError::parse(
                format!("cannot extend inline table '{}'", partial_path),
                line, col,
            ));
        }
    }
    Ok(current)
}

fn ensure_path_exists(
    root: &mut Table,
    path: &[String],
    ctx: &mut ParseContext,
    line: u32,
    col: u32,
) -> Result<(), TomlError> {
    let mut current = root;
    for (i, key) in path.iter().enumerate() {
        let partial_path = path[..=i].join(".");
        if let Some(TableStatus::Inline) = ctx.get_status(&partial_path) {
            return Err(TomlError::parse(
                format!("cannot extend inline table '{}'", partial_path), line, col,
            ));
        }
        let entry = current.inner.entry(key.clone()).or_insert_with(|| {
            if !ctx.is_known(&partial_path) {
                ctx.known.insert(partial_path.clone(), TableStatus::ImplicitlyCreated);
            }
            Value::Table(Table::new())
        });
        match entry {
            Value::Table(t) => current = t,
            Value::Array(arr) => {
                if let Some(Value::Table(t)) = arr.0.last_mut() {
                    current = t;
                } else {
                    return Err(TomlError::parse(
                        format!("cannot use '{}' as table (it's an array)", key), line, col,
                    ));
                }
            }
            _ => return Err(TomlError {
                kind: TomlErrorKind::DuplicateKey,
                message: format!("key '{}' is not a table", key),
                location: Some(crate::error::SourceLocation { line, column: col, source_file: None }),
            }),
        }
    }
    Ok(())
}

fn get_or_create_array_of_tables<'a>(
    root: &'a mut Table,
    path: &[String],
    ctx: &mut ParseContext,
    line: u32,
    col: u32,
) -> Result<&'a mut Array, TomlError> {
    if path.is_empty() {
        return Err(TomlError::parse("empty array-of-tables path", line, col));
    }
    let mut current = root;
    for (i, key) in path[..path.len() - 1].iter().enumerate() {
        let partial = path[..=i].join(".");
        if let Some(TableStatus::Inline) = ctx.get_status(&partial) {
            return Err(TomlError::parse(
                format!("cannot extend inline table '{}'", partial), line, col,
            ));
        }
        let entry = current.inner.entry(key.clone()).or_insert_with(|| {
            if !ctx.is_known(&partial) {
                ctx.known.insert(partial.clone(), TableStatus::ImplicitlyCreated);
            }
            Value::Table(Table::new())
        });
        match entry {
            Value::Table(t) => current = t,
            Value::Array(arr) => {
                if let Some(Value::Table(t)) = arr.0.last_mut() {
                    current = t;
                } else {
                    return Err(TomlError::parse(
                        format!("cannot navigate array '{}' (not array of tables)", key), line, col,
                    ));
                }
            }
            _ => return Err(TomlError::parse(
                format!("key '{}' is not a table", key), line, col,
            )),
        }
    }
    let last_key = &path[path.len() - 1];
    let last_partial = path.join(".");
    if let Some(TableStatus::Inline) = ctx.get_status(&last_partial) {
        return Err(TomlError::parse(
            format!("cannot extend inline table '{}'", last_partial), line, col,
        ));
    }
    let entry = current.inner.entry(last_key.clone()).or_insert_with(|| {
        if !ctx.is_known(&last_partial) {
            ctx.known.insert(last_partial.clone(), TableStatus::ImplicitlyCreated);
        }
        Value::Array(Array::new())
    });
    match entry {
        Value::Array(arr) => Ok(arr),
        _ => Err(TomlError::parse(format!("key '{}' is not an array", last_key), line, col)),
    }
}
