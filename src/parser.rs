//! TOML 1.1 recursive-descent parser.
//!
//! The [`Parser`] struct is the low-level entry point.  Most callers should
//! use the higher-level [`Document::parse`] / [`Document::parse_file`] instead.

use std::collections::HashMap;

use crate::datetime::{LocalDate, LocalDateTime, LocalTime, OffsetDateTime};
use crate::document::Document;
use crate::error::{TomlError, TomlErrorKind};
use crate::value::{Array, Table, Value};

// ── ParseContext ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableStatus {
    /// Explicitly introduced with `[header]`.
    ExplicitlyDefined,
    /// Implicitly created via dotted keys or parent tables.
    ImplicitlyCreated,
    /// Inline table `{…}` – must not be extended afterwards.
    Inline,
    /// Element of `[[array of tables]]`.
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
                // Implicitly created tables can be upgraded to explicitly defined
                (TableStatus::ImplicitlyCreated, TableStatus::ExplicitlyDefined) => {
                    self.known.insert(path.to_string(), status);
                    return Ok(());
                }
                // Array elements can always add new elements (handled elsewhere)
                (TableStatus::ArrayElement, TableStatus::ArrayElement) => {
                    return Ok(());
                }
                // Inline tables can never be re-opened
                (TableStatus::Inline, _) => {
                    return Err(TomlError::parse(
                        format!(
                            "cannot extend inline table '{}' with a table header",
                            path
                        ),
                        line,
                        col,
                    ));
                }
                // Explicitly defined tables cannot be redefined
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
        Self {
            src,
            pos: 0,
            line: 1,
            col: 1,
        }
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
            let len = ch.len_utf8();
            self.pos += len;
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

    /// Skip whitespace (space and tab), but not newlines.
    fn skip_ws(&mut self) {
        while matches!(self.current_byte(), Some(b' ') | Some(b'\t')) {
            self.advance();
        }
    }

    /// Skip whitespace and newlines.
    fn skip_ws_and_newlines(&mut self) {
        loop {
            match self.current_byte() {
                Some(b' ') | Some(b'\t') | Some(b'\n') => {
                    self.advance();
                }
                Some(b'\r') if self.peek_byte(1) == Some(b'\n') => {
                    self.advance();
                    self.advance();
                }
                Some(b'#') => {
                    self.skip_comment();
                }
                _ => break,
            }
        }
    }

    /// Skip a comment (from # to end of line).
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

    /// Skip whitespace and optional comment at end of line.
    fn skip_ws_comment(&mut self) {
        self.skip_ws();
        if self.current_byte() == Some(b'#') {
            self.skip_comment();
        }
    }

    fn err_here(&self, msg: impl Into<String>) -> TomlError {
        TomlError::parse(msg, self.line, self.col)
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Low-level TOML 1.1 parser.
///
/// Holds all mutable state needed during a single parse run.
/// Prefer [`Document::parse`] unless you need direct access to the parser.
pub struct Parser<'src> {
    src: Source<'src>,
    ctx: ParseContext,
}

impl<'src> Parser<'src> {
    /// Create a new `Parser` for the given TOML source string.
    pub fn new(src: &'src str) -> Self {
        Self {
            src: Source::new(src),
            ctx: ParseContext::new(),
        }
    }

    /// Run the parser and return the resulting [`Document`].
    ///
    /// Returns `Err(TomlError)` on any syntax error, duplicate key, or
    /// integer overflow.
    pub fn parse(mut self) -> Result<Document, TomlError> {
        let mut root = Table::new();
        self.parse_document(&mut root)?;
        Ok(Document::from_root(root))
    }

    fn parse_document(&mut self, root: &mut Table) -> Result<(), TomlError> {
        let mut current_path: Vec<String> = vec![];

        loop {
            self.src.skip_ws_and_newlines();
            if self.src.is_eof() {
                break;
            }

            match self.src.current_byte() {
                // Table header [key] or array header [[key]]
                Some(b'[') => {
                    if self.src.peek_byte(1) == Some(b'[') {
                        // Array of tables [[key]]
                        self.src.advance(); // first [
                        self.src.advance(); // second [
                        self.src.skip_ws();
                        let path = self.parse_key()?;
                        self.src.skip_ws();
                        self.expect_byte(b']', "expected ']]'")?;
                        self.expect_byte(b']', "expected ']]'")?;
                        self.src.skip_ws_comment();
                        self.expect_newline_or_eof()?;

                        let path_str = path.join(".");
                        // Mark this path as an ArrayElement (or add to existing)
                        self.ctx.mark(&path_str, TableStatus::ArrayElement, self.src.line, self.src.col)?;

                        // Navigate to (or create) the array at that path
                        let arr = get_or_create_array_of_tables(root, &path, &mut self.ctx, self.src.line, self.src.col)?;
                        arr.0.push(Value::Table(Table::new()));
                        let last_idx = arr.0.len() - 1;
                        current_path = path.clone();

                        // We need to navigate to the last element of the array
                        // and parse key-value pairs into it.
                        // We'll do this by re-navigating on each key-value parse.
                        let _ = last_idx; // used below
                    } else {
                        // Standard table [key]
                        self.src.advance(); // [
                        self.src.skip_ws();
                        let path = self.parse_key()?;
                        self.src.skip_ws();
                        self.expect_byte(b']', "expected ']'")?;
                        self.src.skip_ws_comment();
                        self.expect_newline_or_eof()?;

                        let path_str = path.join(".");
                        let line = self.src.line;
                        let col = self.src.col;
                        self.ctx.mark(&path_str, TableStatus::ExplicitlyDefined, line, col)?;

                        // Ensure all intermediate tables exist and are valid
                        ensure_path_exists(root, &path, &mut self.ctx, line, col)?;
                        current_path = path;
                    }
                }
                _ => {
                    // Key-value pair
                    let line = self.src.line;
                    let col = self.src.col;
                    let target = navigate_to_table_mut(root, &current_path, &mut self.ctx, line, col)?;
                    let (keys, is_inline_table) = self.parse_keyval(target)?;
                    // Register inline tables in ctx so they can't be extended by [headers]
                    if is_inline_table {
                        let mut full_path = if current_path.is_empty() {
                            String::new()
                        } else {
                            current_path.join(".")
                        };
                        if !full_path.is_empty() {
                            full_path.push('.');
                        }
                        full_path.push_str(&keys.join("."));
                        let _ = self.ctx.mark(&full_path, TableStatus::Inline, line, col);
                    }
                    self.src.skip_ws_comment();
                    self.expect_newline_or_eof()?;
                }
            }
        }
        Ok(())
    }

    /// Parse a key-value pair and insert it into `target`.
    /// Returns the key path and whether the value was an inline table.
    fn parse_keyval(&mut self, target: &mut Table) -> Result<(Vec<String>, bool), TomlError> {
        let line = self.src.line;
        let col = self.src.col;
        let keys = self.parse_key()?;
        self.src.skip_ws();
        self.expect_byte(b'=', "expected '='")?;
        self.src.skip_ws();
        let value = self.parse_val()?;
        let is_inline_table = matches!(value, Value::Table(_));

        // Insert with dotted key support
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
            // Dotted key: a.b.c = value
            // Navigate intermediate keys, creating implicit tables
            let mut current = target;
            for (i, key) in keys.iter().enumerate() {
                if i == keys.len() - 1 {
                    // Last key: insert value
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
                    // Intermediate key: navigate or create implicit table
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
        Ok((keys, is_inline_table))
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

    fn parse_val(&mut self) -> Result<Value, TomlError> {
        match self.src.current_byte() {
            Some(b'"') => {
                if self.src.peek_byte(1) == Some(b'"') && self.src.peek_byte(2) == Some(b'"') {
                    let s = self.parse_ml_basic_string()?;
                    Ok(Value::String(s))
                } else {
                    let s = self.parse_basic_string()?;
                    Ok(Value::String(s))
                }
            }
            Some(b'\'') => {
                if self.src.peek_byte(1) == Some(b'\'') && self.src.peek_byte(2) == Some(b'\'') {
                    let s = self.parse_ml_literal_string()?;
                    Ok(Value::String(s))
                } else {
                    let s = self.parse_literal_string()?;
                    Ok(Value::String(s))
                }
            }
            Some(b't') => {
                if self.src.remaining().starts_with("true") {
                    self.src.advance_bytes(4);
                    Ok(Value::Boolean(true))
                } else {
                    Err(self.src.err_here("expected 'true'"))
                }
            }
            Some(b'f') => {
                if self.src.remaining().starts_with("false") {
                    self.src.advance_bytes(5);
                    Ok(Value::Boolean(false))
                } else {
                    Err(self.src.err_here("expected 'false'"))
                }
            }
            Some(b'[') => {
                let arr = self.parse_array()?;
                Ok(Value::Array(arr))
            }
            Some(b'{') => {
                let tbl = self.parse_inline_table()?;
                Ok(Value::Table(tbl))
            }
            Some(b'i') => {
                // inf
                if self.src.remaining().starts_with("inf") {
                    self.src.advance_bytes(3);
                    Ok(Value::Float(f64::INFINITY))
                } else {
                    Err(self.src.err_here("expected 'inf'"))
                }
            }
            Some(b'n') => {
                // nan
                if self.src.remaining().starts_with("nan") {
                    self.src.advance_bytes(3);
                    Ok(Value::Float(f64::NAN))
                } else {
                    Err(self.src.err_here("expected 'nan'"))
                }
            }
            Some(b'+') | Some(b'-') => {
                // Could be number or +inf/-inf/+nan/-nan
                let sign = self.src.current_byte().unwrap() as char;
                self.src.advance();
                if self.src.remaining().starts_with("inf") {
                    self.src.advance_bytes(3);
                    return Ok(Value::Float(if sign == '-' {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    }));
                }
                if self.src.remaining().starts_with("nan") {
                    self.src.advance_bytes(3);
                    return Ok(Value::Float(f64::NAN));
                }
                // Back to number parsing with sign
                let start = self.src.pos - 1; // include sign
                self.parse_number_from(start, sign == '-')
            }
            Some(b) if b.is_ascii_digit() => {
                let start = self.src.pos;
                self.parse_number_from(start, false)
            }
            _ => Err(self.src.err_here("unexpected character")),
        }
    }

    fn parse_number_from(&mut self, start: usize, negated: bool) -> Result<Value, TomlError> {
        // We may have already consumed the sign; start is the position before the sign (or at digit)
        // Re-read from start
        let _remaining = &self.src.src[start..];
        let line = self.src.line;
        let col = self.src.col;

        // Collect all characters that can form a number or datetime
        let end = self.src.pos;
        let _ = end;
        let _ = negated;

        // We need to scan forward to find the end of the token
        // Collect from start
        let tok_start = start;
        // Move src.pos back to start to re-scan
        // Actually we need to be smarter here.
        // Let's scan forward from current pos to find the full token.
        // The sign was already consumed if present.
        let after_sign_pos = self.src.pos;

        // Check for radix prefixes: 0x, 0o, 0b
        if self.src.remaining().starts_with("0x") {
            self.src.advance(); // 0
            self.src.advance(); // x
            let digit_start = self.src.pos;
            while self.src.current_byte().map_or(false, |b| b.is_ascii_hexdigit() || b == b'_') {
                self.src.advance();
            }
            let digits = self.src.src[digit_start..self.src.pos]
                .chars()
                .filter(|&c| c != '_')
                .collect::<String>();
            if digits.is_empty() {
                return Err(TomlError::parse("empty hex integer", line, col));
            }
            let val = u64::from_str_radix(&digits, 16).map_err(|_| {
                TomlError::integer_overflow("integer overflow (hex)", line, col)
            })?;
            // Check sign
            let full = &self.src.src[tok_start..self.src.pos];
            let negative = full.starts_with('-');
            if negative {
                // val <= 2^63  (i64::MIN = -2^63, abs = 0x8000000000000000)
                if val > 0x8000_0000_0000_0000u64 {
                    return Err(TomlError::integer_overflow("integer overflow (hex negative)", line, col));
                }
                // Special case: 0x8000000000000000 == i64::MIN magnitude (no overflow)
                if val == 0x8000_0000_0000_0000u64 {
                    return Ok(Value::Integer(i64::MIN));
                }
                return Ok(Value::Integer(-(val as i64)));
            } else {
                if val > i64::MAX as u64 {
                    return Err(TomlError::integer_overflow("integer overflow (hex)", line, col));
                }
                return Ok(Value::Integer(val as i64));
            }
        }

        if self.src.remaining().starts_with("0o") {
            self.src.advance(); // 0
            self.src.advance(); // o
            let digit_start = self.src.pos;
            while self.src.current_byte().map_or(false, |b| matches!(b, b'0'..=b'7') || b == b'_') {
                self.src.advance();
            }
            let digits = self.src.src[digit_start..self.src.pos]
                .chars()
                .filter(|&c| c != '_')
                .collect::<String>();
            if digits.is_empty() {
                return Err(TomlError::parse("empty octal integer", line, col));
            }
            let val = u64::from_str_radix(&digits, 8).map_err(|_| {
                TomlError::integer_overflow("integer overflow (octal)", line, col)
            })?;
            let full = &self.src.src[tok_start..self.src.pos];
            let negative = full.starts_with('-');
            if negative {
                if val > 0x8000_0000_0000_0000u64 {
                    return Err(TomlError::integer_overflow("integer overflow (octal negative)", line, col));
                }
                if val == 0x8000_0000_0000_0000u64 {
                    return Ok(Value::Integer(i64::MIN));
                }
                return Ok(Value::Integer(-(val as i64)));
            } else {
                if val > i64::MAX as u64 {
                    return Err(TomlError::integer_overflow("integer overflow (octal)", line, col));
                }
                return Ok(Value::Integer(val as i64));
            }
        }

        if self.src.remaining().starts_with("0b") {
            self.src.advance(); // 0
            self.src.advance(); // b
            let digit_start = self.src.pos;
            while self.src.current_byte().map_or(false, |b| matches!(b, b'0' | b'1') || b == b'_') {
                self.src.advance();
            }
            let digits = self.src.src[digit_start..self.src.pos]
                .chars()
                .filter(|&c| c != '_')
                .collect::<String>();
            if digits.is_empty() {
                return Err(TomlError::parse("empty binary integer", line, col));
            }
            let val = u64::from_str_radix(&digits, 2).map_err(|_| {
                TomlError::integer_overflow("integer overflow (binary)", line, col)
            })?;
            let full = &self.src.src[tok_start..self.src.pos];
            let negative = full.starts_with('-');
            if negative {
                if val > 0x8000_0000_0000_0000u64 {
                    return Err(TomlError::integer_overflow("integer overflow (binary negative)", line, col));
                }
                if val == 0x8000_0000_0000_0000u64 {
                    return Ok(Value::Integer(i64::MIN));
                }
                return Ok(Value::Integer(-(val as i64)));
            } else {
                if val > i64::MAX as u64 {
                    return Err(TomlError::integer_overflow("integer overflow (binary)", line, col));
                }
                return Ok(Value::Integer(val as i64));
            }
        }

        // Decimal integer or float or datetime
        // Scan digits
        let num_start = self.src.pos;

        // Scan digits, dots, colons, dashes, underscores, e, E, +, -, T, t, Z, z
        while let Some(b) = self.src.current_byte() {
            match b {
                b'0'..=b'9' | b'.' | b'e' | b'E' | b'_' | b':' | b'-' | b'+' | b'T' | b't' | b'Z' | b'z' | b' ' => {
                    // Space can be datetime delimiter
                    if b == b' ' {
                        // Only allow one space as delimiter between date and time
                        // Check if it looks like a datetime (if we have a date pattern so far)
                        let so_far = &self.src.src[after_sign_pos..self.src.pos];
                        if looks_like_date(so_far) {
                            // Check next byte
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

        // Try datetime first
        if let Some(v) = try_parse_datetime(token_str, line, col)? {
            return Ok(v);
        }

        // Try float
        let clean_no_sign = &self.src.src[after_sign_pos..self.src.pos];
        let clean = clean_no_sign.replace('_', "");
        let signed = token_str.replace('_', "");

        if signed.contains('.') || (signed.contains('e') || signed.contains('E'))
            && !signed.starts_with("0x")
            && !signed.starts_with("0o")
            && !signed.starts_with("0b")
        {
            let f: f64 = signed.parse().map_err(|_| {
                TomlError::parse(format!("invalid float: '{}'", signed), line, col)
            })?;
            return Ok(Value::Float(f));
        }

        // Integer decimal
        let _ = clean;
        let signed_clean = token_str.replace('_', "");

        // Must only contain digits (and optional leading sign)
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

        // Verify with checked arithmetic for overflow
        let num_start_str = &self.src.src[num_start..self.src.pos];
        let _ = num_start_str;

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
                Some(b'"') => {
                    self.src.advance();
                    break;
                }
                Some(b'\\') => {
                    self.src.advance();
                    let escaped = self.parse_escape_sequence()?;
                    result.push_str(&escaped);
                }
                Some(b'\n') | Some(b'\r') => {
                    return Err(TomlError::parse("newline in basic string", line, col));
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
        // Consume opening """
        self.expect_byte(b'"', "expected '\"'")?;
        self.expect_byte(b'"', "expected '\"'")?;
        self.expect_byte(b'"', "expected '\"'")?;
        // Skip optional immediate newline
        if self.src.current_byte() == Some(b'\n') {
            self.src.advance();
        } else if self.src.current_byte() == Some(b'\r')
            && self.src.peek_byte(1) == Some(b'\n')
        {
            self.src.advance();
            self.src.advance();
        }
        let mut result = String::new();
        loop {
            match self.src.current_byte() {
                None => return Err(TomlError::parse("unterminated multiline string", line, col)),
                Some(b'"') => {
                    // Check for closing """
                    if self.src.peek_byte(1) == Some(b'"') && self.src.peek_byte(2) == Some(b'"') {
                        // But it could be """"" (5 quotes) = 2 literal + closing 3
                        self.src.advance();
                        self.src.advance();
                        self.src.advance();
                        // Check for extra quotes (up to 2 allowed)
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
                    // Line ending backslash: skip whitespace/newlines
                    if self.src.current_byte() == Some(b'\n')
                        || (self.src.current_byte() == Some(b'\r')
                            && self.src.peek_byte(1) == Some(b'\n'))
                    {
                        if self.src.current_byte() == Some(b'\r') {
                            self.src.advance();
                        }
                        self.src.advance(); // \n
                        while matches!(
                            self.src.current_byte(),
                            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
                        ) {
                            self.src.advance();
                        }
                        continue;
                    }
                    let escaped = self.parse_escape_sequence()?;
                    result.push_str(&escaped);
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
                Some(b'\'') => {
                    self.src.advance();
                    break;
                }
                Some(b'\n') | Some(b'\r') => {
                    return Err(TomlError::parse("newline in literal string", line, col));
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
        // Consume opening '''
        self.expect_byte(b'\'', "expected \"'\"")?;
        self.expect_byte(b'\'', "expected \"'\"")?;
        self.expect_byte(b'\'', "expected \"'\"")?;
        // Skip optional immediate newline
        if self.src.current_byte() == Some(b'\n') {
            self.src.advance();
        } else if self.src.current_byte() == Some(b'\r')
            && self.src.peek_byte(1) == Some(b'\n')
        {
            self.src.advance();
            self.src.advance();
        }
        let mut result = String::new();
        loop {
            match self.src.current_byte() {
                None => return Err(TomlError::parse("unterminated multiline literal string", line, col)),
                Some(b'\'') => {
                    if self.src.peek_byte(1) == Some(b'\'') && self.src.peek_byte(2) == Some(b'\'') {
                        self.src.advance();
                        self.src.advance();
                        self.src.advance();
                        // Extra quotes
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
            Some(b'e') => {
                // TOML 1.1: \e = ESC = U+001B
                self.src.advance();
                Ok("\x1B".to_string())
            }
            Some(b'x') => {
                // TOML 1.1: \xHH
                self.src.advance();
                let h1 = self.read_hex_digit()?;
                let h2 = self.read_hex_digit()?;
                let code = (h1 << 4) | h2;
                let ch = char::from_u32(code as u32).ok_or_else(|| {
                    TomlError {
                        kind: TomlErrorKind::InvalidEscape(format!("\\x{:02x}", code)),
                        message: format!("invalid \\x escape: {:02x}", code),
                        location: Some(crate::error::SourceLocation {
                            line,
                            column: col,
                            source_file: None,
                        }),
                    }
                })?;
                Ok(ch.to_string())
            }
            Some(b'u') => {
                self.src.advance();
                let code = self.read_hex_digits(4)?;
                let ch = char::from_u32(code).ok_or_else(|| TomlError {
                    kind: TomlErrorKind::InvalidEscape(format!("\\u{:04X}", code)),
                    message: format!("invalid Unicode escape: \\u{:04X}", code),
                    location: Some(crate::error::SourceLocation {
                        line,
                        column: col,
                        source_file: None,
                    }),
                })?;
                Ok(ch.to_string())
            }
            Some(b'U') => {
                self.src.advance();
                let code = self.read_hex_digits(8)?;
                let ch = char::from_u32(code).ok_or_else(|| TomlError {
                    kind: TomlErrorKind::InvalidEscape(format!("\\U{:08X}", code)),
                    message: format!("invalid Unicode escape: \\U{:08X}", code),
                    location: Some(crate::error::SourceLocation {
                        line,
                        column: col,
                        source_file: None,
                    }),
                })?;
                Ok(ch.to_string())
            }
            Some(other) => Err(TomlError {
                kind: TomlErrorKind::InvalidEscape(format!("\\{}", other as char)),
                message: format!("invalid escape sequence: \\{}", other as char),
                location: Some(crate::error::SourceLocation {
                    line,
                    column: col,
                    source_file: None,
                }),
            }),
            None => Err(TomlError::parse("unexpected EOF in escape sequence", line, col)),
        }
    }

    fn read_hex_digit(&mut self) -> Result<u8, TomlError> {
        let b = self.src.current_byte().ok_or_else(|| {
            self.src.err_here("expected hex digit")
        })?;
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

    fn parse_array(&mut self) -> Result<Array, TomlError> {
        let line = self.src.line;
        let col = self.src.col;
        self.expect_byte(b'[', "expected '['")?;
        let mut arr = Array::new();
        loop {
            // Skip whitespace, newlines, and comments
            self.skip_ws_nl_comments();
            if self.src.current_byte() == Some(b']') {
                self.src.advance();
                break;
            }
            if self.src.is_eof() {
                return Err(TomlError::parse("unterminated array", line, col));
            }
            let val = self.parse_val()?;
            arr.0.push(val);
            self.skip_ws_nl_comments();
            match self.src.current_byte() {
                Some(b',') => {
                    self.src.advance();
                    // Trailing comma: after comma, skip to ]
                }
                Some(b']') => {
                    self.src.advance();
                    break;
                }
                _ => {
                    return Err(TomlError::parse("expected ',' or ']' in array", self.src.line, self.src.col));
                }
            }
        }
        Ok(arr)
    }

    fn parse_inline_table(&mut self) -> Result<Table, TomlError> {
        let line = self.src.line;
        let col = self.src.col;
        self.expect_byte(b'{', "expected '{'")?;
        let mut tbl = Table::new();
        // TOML 1.1: newlines are allowed inside inline tables
        self.skip_ws_nl_comments();
        if self.src.current_byte() == Some(b'}') {
            self.src.advance();
            return Ok(tbl);
        }
        if self.src.is_eof() {
            return Err(TomlError::parse("unterminated inline table", line, col));
        }
        loop {
            self.skip_ws_nl_comments();
            if self.src.is_eof() {
                return Err(TomlError::parse("unterminated inline table", line, col));
            }
            if self.src.current_byte() == Some(b'}') {
                self.src.advance();
                break;
            }
            let _ = self.parse_keyval(&mut tbl)?; // ignore return value for inline tables
            self.skip_ws_nl_comments();
            match self.src.current_byte() {
                Some(b',') => {
                    self.src.advance();
                    // TOML 1.1: trailing comma allowed
                    self.skip_ws_nl_comments();
                    if self.src.current_byte() == Some(b'}') {
                        self.src.advance();
                        break;
                    }
                }
                Some(b'}') => {
                    self.src.advance();
                    break;
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
        Ok(tbl)
    }

    fn skip_ws_nl_comments(&mut self) {
        loop {
            match self.src.current_byte() {
                Some(b' ') | Some(b'\t') | Some(b'\n') => {
                    self.src.advance();
                }
                Some(b'\r') if self.src.peek_byte(1) == Some(b'\n') => {
                    self.src.advance();
                    self.src.advance();
                }
                Some(b'#') => {
                    self.src.skip_comment();
                }
                _ => break,
            }
        }
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
            Some(b'\n') => {
                self.src.advance();
                Ok(())
            }
            Some(b'\r') if self.src.peek_byte(1) == Some(b'\n') => {
                self.src.advance();
                self.src.advance();
                Ok(())
            }
            _ => Err(self.src.err_here("expected newline or EOF")),
        }
    }
}

// ── Helper functions ──────────────────────────────────────────────────────────

fn is_bare_key_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn looks_like_date(s: &str) -> bool {
    // Check if s matches YYYY-MM-DD
    if s.len() < 10 {
        return false;
    }
    let bytes = s.as_bytes();
    bytes[4] == b'-' && bytes[7] == b'-'
}

/// Try to parse a datetime token. Returns Ok(None) if not a datetime.
fn try_parse_datetime(s: &str, line: u32, col: u32) -> Result<Option<Value>, TomlError> {
    let bytes = s.as_bytes();

    // Check HH:MM pattern (local time) FIRST — before the length >= 10 check
    // A local time starts with HH:MM which is only 5 chars
    let is_time = s.len() >= 5
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b':'
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && (s.len() < 10 || bytes[4] != b'-'); // distinguish from date

    // Check YYYY-MM-DD pattern
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

    if !is_date {
        return Ok(None);
    }

    let date = parse_date_str(&s[..10], line, col)?;

    if s.len() == 10 {
        return Ok(Some(Value::LocalDate(date)));
    }

    // Delimiter: T, t, or space
    let delim = bytes[10];
    if delim != b'T' && delim != b't' && delim != b' ' {
        return Ok(Some(Value::LocalDate(date)));
    }

    let time_and_offset = &s[11..];
    let (time_str, offset_str) = split_time_offset(time_and_offset);
    let time = parse_time_str(time_str, line, col)?;

    if let Some(off) = offset_str {
        let offset_minutes = parse_offset(off, line, col)?;
        Ok(Some(Value::OffsetDateTime(OffsetDateTime {
            date,
            time,
            offset_minutes,
        })))
    } else {
        Ok(Some(Value::LocalDateTime(LocalDateTime { date, time })))
    }
}

fn parse_date_str(s: &str, line: u32, col: u32) -> Result<LocalDate, TomlError> {
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return Err(TomlError::parse("invalid date", line, col));
    }
    let year = parse_digits(&s[0..4], line, col)? as i32;
    let month = parse_digits(&s[5..7], line, col)? as u8;
    let day = parse_digits(&s[8..10], line, col)? as u8;
    Ok(LocalDate { year, month, day })
}

fn parse_time_str(s: &str, line: u32, col: u32) -> Result<LocalTime, TomlError> {
    let bytes = s.as_bytes();
    if bytes.len() < 5 {
        return Err(TomlError::parse("invalid time", line, col));
    }
    let hour = parse_digits(&s[0..2], line, col)? as u8;
    if bytes[2] != b':' {
        return Err(TomlError::parse("invalid time (expected ':')", line, col));
    }
    let minute = parse_digits(&s[3..5], line, col)? as u8;

    // Check if seconds follow (TOML 1.1: seconds are optional)
    if bytes.len() <= 5 || bytes[5] != b':' {
        return Ok(LocalTime {
            hour,
            minute,
            second: 0,
            nanosecond: 0,
        });
    }

    // A ':' at position 5 was found, so seconds must follow — need at least HH:MM:SS (8 bytes)
    if bytes.len() < 8 {
        return Err(TomlError::parse(
            "invalid time: seconds field incomplete after ':'",
            line,
            col,
        ));
    }

    let second = parse_digits(&s[6..8], line, col)? as u8;
    let nanosecond = if bytes.len() > 8 && bytes[8] == b'.' {
        parse_fractional_seconds(&s[9..], line, col)?
    } else {
        0
    };

    Ok(LocalTime {
        hour,
        minute,
        second,
        nanosecond,
    })
}

fn parse_fractional_seconds(s: &str, _line: u32, _col: u32) -> Result<u32, TomlError> {
    // s is the fractional part (digits only, possibly with offset suffix)
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Ok(0);
    }
    // Pad or truncate to 9 digits
    let padded = if digits.len() >= 9 {
        digits[..9].to_string()
    } else {
        format!("{:0<9}", digits)
    };
    let ns: u32 = padded.parse().unwrap_or(0);
    Ok(ns)
}

fn split_time_offset(s: &str) -> (&str, Option<&str>) {
    // Find Z, z, +, - that marks the offset
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'Z' | b'z' => return (&s[..i], Some(&s[i..])),
            b'+' | b'-' if i > 0 => {
                // Ensure it's not the sign for exponent in a float
                return (&s[..i], Some(&s[i..]));
            }
            _ => {}
        }
    }
    (s, None)
}

fn parse_offset(s: &str, line: u32, col: u32) -> Result<i32, TomlError> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err(TomlError::parse("empty offset", line, col));
    }
    match bytes[0] {
        b'Z' | b'z' => return Ok(OffsetDateTime::UTC_OFFSET),
        b'+' | b'-' => {}
        _ => return Err(TomlError::parse("invalid offset", line, col)),
    }
    let sign = if bytes[0] == b'-' { -1i32 } else { 1i32 };
    if s.len() < 6 {
        return Err(TomlError::parse("invalid offset format", line, col));
    }
    let hours = parse_digits(&s[1..3], line, col)? as i32;
    if bytes[3] != b':' {
        return Err(TomlError::parse("invalid offset (expected ':')", line, col));
    }
    let minutes = parse_digits(&s[4..6], line, col)? as i32;
    Ok(sign * (hours * 60 + minutes))
}

fn parse_digits(s: &str, line: u32, col: u32) -> Result<u64, TomlError> {
    s.parse::<u64>().map_err(|_| {
        TomlError::parse(format!("invalid digits: '{}'", s), line, col)
    })
}

/// Navigate to the table at `path` within `root`, creating tables as needed.
/// Errors if a non-table or inline-table is encountered.
fn navigate_to_table_mut<'a>(
    root: &'a mut Table,
    path: &[String],
    ctx: &mut ParseContext,
    line: u32,
    col: u32,
) -> Result<&'a mut Table, TomlError> {
    if path.is_empty() {
        return Ok(root);
    }
    let mut current = root;
    for (i, key) in path.iter().enumerate() {
        let partial_path = path[..=i].join(".");

        let entry = current
            .inner
            .get_mut(key.as_str())
            .ok_or_else(|| TomlError::parse(format!("table '{}' not found", partial_path), line, col))?;

        match entry {
            Value::Table(t) => current = t,
            Value::Array(arr) => {
                // Navigate to last element (for array-of-tables)
                if let Some(Value::Table(t)) = arr.0.last_mut() {
                    current = t;
                } else {
                    return Err(TomlError::parse(
                        format!("cannot navigate into array '{}' (not an array of tables)", key),
                        line,
                        col,
                    ));
                }
            }
            _ => {
                return Err(TomlError::parse(
                    format!("key '{}' is not a table", key),
                    line,
                    col,
                ));
            }
        }

        let status = ctx.get_status(&partial_path);
        if status == Some(TableStatus::Inline) {
            return Err(TomlError::parse(
                format!("cannot extend inline table '{}'", partial_path),
                line,
                col,
            ));
        }
    }
    Ok(current)
}

/// Ensure intermediate tables exist along `path`, marking them as implicit.
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

        // Check for inline table conflict
        if let Some(TableStatus::Inline) = ctx.get_status(&partial_path) {
            return Err(TomlError::parse(
                format!("cannot extend inline table '{}'", partial_path),
                line,
                col,
            ));
        }

        let entry = current
            .inner
            .entry(key.clone())
            .or_insert_with(|| {
                if !ctx.is_known(&partial_path) {
                    ctx.known.insert(partial_path.clone(), TableStatus::ImplicitlyCreated);
                }
                Value::Table(Table::new())
            });

        match entry {
            Value::Table(t) => current = t,
            Value::Array(arr) => {
                // Navigate to last element for array-of-tables
                if let Some(Value::Table(t)) = arr.0.last_mut() {
                    current = t;
                } else {
                    return Err(TomlError::parse(
                        format!("cannot use '{}' as table (it's an array)", key),
                        line,
                        col,
                    ));
                }
            }
            _ => {
                return Err(TomlError {
                    kind: TomlErrorKind::DuplicateKey,
                    message: format!("key '{}' is not a table", key),
                    location: Some(crate::error::SourceLocation {
                        line,
                        column: col,
                        source_file: None,
                    }),
                });
            }
        }
    }
    Ok(())
}

/// Navigate to (or create) an Array for array-of-tables at `path`.
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
                format!("cannot extend inline table '{}'", partial),
                line,
                col,
            ));
        }

        let entry = current
            .inner
            .entry(key.clone())
            .or_insert_with(|| {
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
                        format!("path component '{}' is not navigable", key),
                        line,
                        col,
                    ));
                }
            }
            _ => {
                return Err(TomlError::parse(
                    format!("'{}' is not a table", key),
                    line,
                    col,
                ));
            }
        }
    }

    let last_key = &path[path.len() - 1];
    let full_path = path.join(".");

    // Check for conflicts (without returning a borrow from the if-let block)
    let needs_insert = match current.inner.get(last_key.as_str()) {
        Some(Value::Array(_)) => false,  // already exists as array, just return it
        Some(_) => {
            return Err(TomlError {
                kind: TomlErrorKind::DuplicateKey,
                message: format!("'{}' is not an array", full_path),
                location: Some(crate::error::SourceLocation {
                    line,
                    column: col,
                    source_file: None,
                }),
            });
        }
        None => true,
    };

    if needs_insert {
        current
            .inner
            .insert(last_key.clone(), Value::Array(Array::new()));
    }

    match current.inner.get_mut(last_key.as_str()) {
        Some(Value::Array(arr)) => Ok(arr),
        _ => unreachable!(),
    }
}

/// Public parse entry point.
pub fn parse(src: &str) -> Result<Document, TomlError> {
    Parser::new(src).parse()
}
