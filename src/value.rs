//! TOML value types: [`Value`], [`Array`], [`Table`], and the [`FromValue`] trait.

use std::ops::{Index, IndexMut};

use crate::datetime::{LocalDate, LocalDateTime, LocalTime, OffsetDateTime};
use crate::error::{TomlError, TomlErrorKind};

// ── Value ─────────────────────────────────────────────────────────────────────

/// A TOML value — the union of all types defined by the TOML 1.1 specification.
///
/// Each variant wraps the Rust type that most naturally represents the
/// corresponding TOML type.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A UTF-8 string (basic or literal, single- or multi-line).
    String(String),
    /// A 64-bit signed integer.
    Integer(i64),
    /// An IEEE 754 double-precision float, including `nan`, `inf`, and `-inf`.
    Float(f64),
    /// A boolean `true` or `false`.
    Boolean(bool),
    /// A date-time with an explicit UTC offset (`offset-date-time`).
    OffsetDateTime(OffsetDateTime),
    /// A date-time without any timezone information (`local-date-time`).
    LocalDateTime(LocalDateTime),
    /// A calendar date without time or timezone (`local-date`).
    LocalDate(LocalDate),
    /// A wall-clock time without date or timezone (`local-time`).
    LocalTime(LocalTime),
    /// An ordered list of TOML values.
    Array(Array),
    /// An inline or section table of key-value pairs.
    Table(Table),
}

impl Value {
    /// Return a lowercase, static TOML type name for this value.
    ///
    /// The returned string is the same label used in [`TomlErrorKind::TypeError`]
    /// messages (e.g. `"string"`, `"integer"`, `"array"`, …).
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(_) => "string",
            Value::Integer(_) => "integer",
            Value::Float(_) => "float",
            Value::Boolean(_) => "boolean",
            Value::OffsetDateTime(_) => "offset-date-time",
            Value::LocalDateTime(_) => "local-date-time",
            Value::LocalDate(_) => "local-date",
            Value::LocalTime(_) => "local-time",
            Value::Array(_) => "array",
            Value::Table(_) => "table",
        }
    }
}

// ── FromValue trait ───────────────────────────────────────────────────────────

/// Trait for types that can be borrowed directly from a [`Value`] reference.
///
/// Implement (or rely on the provided blanket impls) to use
/// [`Table::get_as`] and [`Array::as_typed`] with a concrete Rust type.
pub trait FromValue: Sized {
    /// The lowercase TOML type name, used in type-error messages.
    fn type_name() -> &'static str;
    /// Try to borrow `Self` from `v`; returns `None` when the variant does not match.
    fn from_value_ref(v: &Value) -> Option<&Self>;
}

macro_rules! impl_from_value {
    ($rust_type:ty, $variant:ident, $name:literal) => {
        impl FromValue for $rust_type {
            fn type_name() -> &'static str {
                $name
            }
            fn from_value_ref(v: &Value) -> Option<&Self> {
                if let Value::$variant(ref inner) = v {
                    Some(inner)
                } else {
                    None
                }
            }
        }
    };
}

impl_from_value!(String, String, "string");
impl_from_value!(i64, Integer, "integer");
impl_from_value!(f64, Float, "float");
impl_from_value!(bool, Boolean, "boolean");
impl_from_value!(OffsetDateTime, OffsetDateTime, "offset-date-time");
impl_from_value!(LocalDateTime, LocalDateTime, "local-date-time");
impl_from_value!(LocalDate, LocalDate, "local-date");
impl_from_value!(LocalTime, LocalTime, "local-time");
impl_from_value!(Array, Array, "array");
impl_from_value!(Table, Table, "table");

// ── Array ─────────────────────────────────────────────────────────────────────

/// An ordered, heterogeneous list of TOML values.
///
/// Wraps a `Vec<Value>` and exposes a Vec-like API.  Indexing with `[usize]`
/// panics for out-of-bounds access; use [`Array::get`] for a checked variant.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Array(pub(crate) Vec<Value>);

impl Array {
    /// Create an empty array.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a reference to the element at `index`, or `None` if out of bounds.
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.0.get(index)
    }

    /// Return a mutable reference to the element at `index`, or `None` if out of bounds.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Value> {
        self.0.get_mut(index)
    }

    /// Append `value` to the end of the array.
    pub fn push(&mut self, value: Value) {
        self.0.push(value);
    }

    /// Insert `value` at `index`, shifting subsequent elements right.
    pub fn insert(&mut self, index: usize, value: Value) {
        self.0.insert(index, value);
    }

    /// Remove and return the element at `index`, shifting subsequent elements left.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn remove(&mut self, index: usize) -> Value {
        self.0.remove(index)
    }

    /// Return the number of elements in the array.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Return `true` if the array contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return an iterator over shared references to the elements.
    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.0.iter()
    }

    /// Return an iterator over mutable references to the elements.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.0.iter_mut()
    }

    /// Try to collect all elements into a `Vec<T>`.
    ///
    /// Returns `Err(TomlError { kind: TypeError { … }, … })` if any element
    /// is not of the expected type `T`.
    pub fn as_typed<T: FromValue + Clone>(&self) -> Result<Vec<T>, TomlError> {
        self.0
            .iter()
            .map(|v| {
                T::from_value_ref(v)
                    .cloned()
                    .ok_or_else(|| TomlError {
                        kind: TomlErrorKind::TypeError {
                            expected: T::type_name(),
                            found: v.type_name(),
                        },
                        message: format!(
                            "array element type error: expected {}, found {}",
                            T::type_name(),
                            v.type_name()
                        ),
                        location: None,
                    })
            })
            .collect()
    }
}

impl Index<usize> for Array {
    type Output = Value;
    fn index(&self, index: usize) -> &Value {
        &self.0[index]
    }
}

impl IndexMut<usize> for Array {
    fn index_mut(&mut self, index: usize) -> &mut Value {
        &mut self.0[index]
    }
}

impl<'a> IntoIterator for &'a Array {
    type Item = &'a Value;
    type IntoIter = std::slice::Iter<'a, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut Array {
    type Item = &'a mut Value;
    type IntoIter = std::slice::IterMut<'a, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

// ── Table ─────────────────────────────────────────────────────────────────────

/// An ordered map from string keys to TOML values.
///
/// Key insertion order is preserved using [`indexmap::IndexMap`], so
/// serialized output matches the order in which keys were added.
/// Indexing with `[&str]` panics when the key is absent; use
/// [`Table::get`] for a non-panicking variant.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub(crate) inner: indexmap::IndexMap<String, Value>,
}

impl Table {
    /// Create an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return `true` if the table contains an entry with the given key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// Return a reference to the value for `key`, or `None` if the key is absent.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.inner.get(key)
    }

    /// Return a mutable reference to the value for `key`, or `None` if the key is absent.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.inner.get_mut(key)
    }

    /// Return a typed reference to the value at `key`.
    ///
    /// Returns `Err(TomlError { kind: KeyNotFound, … })` when the key does not
    /// exist, or `Err(TomlError { kind: TypeError { … }, … })` when the value
    /// is present but has the wrong type.
    pub fn get_as<T: FromValue>(&self, key: &str) -> Result<&T, TomlError> {
        match self.inner.get(key) {
            None => Err(TomlError::key_not_found(key)),
            Some(v) => T::from_value_ref(v).ok_or_else(|| {
                TomlError::type_error(T::type_name(), v.type_name(), key)
            }),
        }
    }

    /// Insert or replace the value at `key`.
    ///
    /// Returns the previous value if the key was already present, or `None`.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        self.inner.insert(key.into(), value)
    }

    /// Remove the entry with `key` and return its value, or `None` if absent.
    ///
    /// Removal preserves the relative order of remaining keys.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.inner.shift_remove(key)
    }

    /// Look up a value by dot-separated path, e.g. `"server.host"`.
    ///
    /// Each path component is treated as a literal key name (no escaping).
    /// Returns `None` if any component along the path is missing or if an
    /// intermediate value is not a table.
    ///
    /// Note: the path is split at every `.`. Keys that themselves contain a dot
    /// must be looked up via [`Table::get_path_segments`].
    pub fn get_path(&self, dotted_key: &str) -> Option<&Value> {
        let parts: Vec<&str> = dotted_key.split('.').collect();
        let mut current: &Value;
        // Start from the table itself
        if parts.is_empty() {
            return None;
        }
        let first = self.inner.get(parts[0])?;
        if parts.len() == 1 {
            return Some(first);
        }
        current = first;
        for part in &parts[1..] {
            match current {
                Value::Table(t) => {
                    current = t.inner.get(*part)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }

    /// Look up a value mutably by dot-separated path, e.g. `"server.port"`.
    ///
    /// Returns `None` under the same conditions as [`Table::get_path`].
    pub fn get_path_mut(&mut self, dotted_key: &str) -> Option<&mut Value> {
        let parts: Vec<&str> = dotted_key.split('.').collect();
        if parts.is_empty() {
            return None;
        }
        if parts.len() == 1 {
            return self.inner.get_mut(parts[0]);
        }
        let first = self.inner.get_mut(parts[0])?;
        let mut current: &mut Value = first;
        for part in &parts[1..] {
            match current {
                Value::Table(t) => {
                    current = t.inner.get_mut(*part)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }

    /// Insert `value` at a dot-separated path, creating intermediate tables
    /// automatically when they do not yet exist.
    ///
    /// Returns `Ok(Some(old_value))` if the final key already had a value,
    /// `Ok(None)` when a new key was created, or `Err(TomlError { … })` when
    /// an intermediate path component exists but is not a table, or when
    /// `dotted_key` is empty.
    pub fn insert_path(
        &mut self,
        dotted_key: &str,
        value: Value,
    ) -> Result<Option<Value>, TomlError> {
        let parts: Vec<&str> = dotted_key.split('.').collect();
        if parts.is_empty() {
            return Err(TomlError {
                kind: crate::error::TomlErrorKind::ParseError,
                message: "empty key path".to_string(),
                location: None,
            });
        }
        if parts.len() == 1 {
            return Ok(self.inner.insert(parts[0].to_string(), value));
        }
        // Navigate/create intermediate tables
        let mut current = self;
        for &part in &parts[..parts.len() - 1] {
            let entry = current
                .inner
                .entry(part.to_string())
                .or_insert_with(|| Value::Table(Table::new()));
            match entry {
                Value::Table(t) => current = t,
                _ => {
                    return Err(TomlError {
                        kind: crate::error::TomlErrorKind::ParseError,
                        message: format!("path component '{}' is not a table", part),
                        location: None,
                    });
                }
            }
        }
        let last = parts[parts.len() - 1];
        Ok(current.inner.insert(last.to_string(), value))
    }

    /// Look up a value using an explicit list of key segments.
    /// Use this method when individual keys contain a dot character.
    pub fn get_path_segments(&self, parts: &[&str]) -> Option<&Value> {
        if parts.is_empty() { return None; }
        let first = self.inner.get(parts[0])?;
        if parts.len() == 1 { return Some(first); }
        let mut current = first;
        for part in &parts[1..] {
            match current {
                Value::Table(t) => current = t.inner.get(*part)?,
                _ => return None,
            }
        }
        Some(current)
    }

    /// Mutable version of [`Table::get_path_segments`].
    pub fn get_path_segments_mut(&mut self, parts: &[&str]) -> Option<&mut Value> {
        if parts.is_empty() { return None; }
        if parts.len() == 1 {
            return self.inner.get_mut(parts[0]);
        }
        let first = self.inner.get_mut(parts[0])?;
        let mut current: &mut Value = first;
        for part in &parts[1..] {
            match current {
                Value::Table(t) => current = t.inner.get_mut(*part)?,
                _ => return None,
            }
        }
        Some(current)
    }

    /// Insert a value at an explicit key path.
    pub fn insert_path_segments(
        &mut self,
        parts: &[&str],
        value: Value,
    ) -> Result<Option<Value>, TomlError> {
        if parts.is_empty() {
            return Err(TomlError {
                kind: crate::error::TomlErrorKind::ParseError,
                message: "empty key path".to_string(),
                location: None,
            });
        }
        if parts.len() == 1 {
            return Ok(self.inner.insert(parts[0].to_string(), value));
        }
        // Navigate/create intermediate tables
        let mut current = self;
        for &part in &parts[..parts.len() - 1] {
            let entry = current
                .inner
                .entry(part.to_string())
                .or_insert_with(|| Value::Table(Table::new()));
            match entry {
                Value::Table(t) => current = t,
                _ => {
                    return Err(TomlError {
                        kind: crate::error::TomlErrorKind::ParseError,
                        message: format!("path component '{}' is not a table", part),
                        location: None,
                    });
                }
            }
        }
        let last = parts[parts.len() - 1];
        Ok(current.inner.insert(last.to_string(), value))
    }

    /// Return an iterator over `(key, value)` pairs in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.inner.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Return a mutable iterator over `(key, value)` pairs in insertion order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut Value)> {
        self.inner.iter_mut().map(|(k, v)| (k.as_str(), v))
    }

    /// Return the number of key-value pairs in the table.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Return `true` if the table contains no key-value pairs.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Return an iterator over the keys in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.inner.keys().map(|k| k.as_str())
    }
}

impl Index<&str> for Table {
    type Output = Value;
    /// Index into the table by key.
    ///
    /// # Panics
    ///
    /// Panics with the message `"key '<key>' not found in Table"` if the key
    /// is absent.  Use [`Table::get`] for a non-panicking alternative.
    fn index(&self, key: &str) -> &Value {
        self.inner
            .get(key)
            .unwrap_or_else(|| panic!("key '{}' not found in Table", key))
    }
}

impl IndexMut<&str> for Table {
    /// Mutably index into the table by key.
    ///
    /// # Panics
    ///
    /// Panics with the message `"key '<key>' not found in Table"` if the key
    /// is absent.  Use [`Table::get_mut`] for a non-panicking alternative.
    fn index_mut(&mut self, key: &str) -> &mut Value {
        self.inner
            .get_mut(key)
            .unwrap_or_else(|| panic!("key '{}' not found in Table", key))
    }
}
