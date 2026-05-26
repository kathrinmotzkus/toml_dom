# toml_dom

A complete TOML 1.1 library in Rust for reading, editing, and writing TOML documents.

---

## Why toml_dom?

| | `toml` (Cargo team) | `toml_edit` | **`toml_dom`** |
|---|---|---|---|
| TOML version | 1.0 | 1.0 | **1.1** |
| Read data | ✓ (via serde) | ✓ | ✓ |
| Modify data | ✗ | ✓ (format-preserving) | ✓ |
| Mutable DOM without serde | ✗ | limited | **✓** |
| Targeted path access | ✗ | ✗ | **✓** |
| No serde dependency | ✗ | ✗ | **✓** |

`toml_dom` is designed for applications that need to **read, modify, and write back a TOML document programmatically** — without serde derive macros and with full TOML 1.1 support.

---

## Table of Contents

- [What the library does](#what-the-library-does)
- [How it works](#how-it-works)
- [Integration into your projects](#integration-into-your-projects)
- [The public API](#the-public-api)
  - [Document](#document)
  - [Table](#table)
  - [Array](#array)
  - [Value](#value)
  - [Date and time types](#date-and-time-types)
  - [Error handling](#error-handling)
  - [Serialization options](#serialization-options)
- [Detailed examples](#detailed-examples)
- [TOML 1.1 specifics](#toml-11-specifics)
- [Project structure](#project-structure)

---

## What the library does

`toml_dom` fully implements the [TOML specification version 1.1](https://toml.io/en/v1.1.0). It allows other Rust programs to:

- read TOML documents from **strings, files, or arbitrary `Read` sources**,
- hold the document in memory as a **mutable data model**,
- **read, modify, add, and delete** individual values,
- serialize the data model back as **valid TOML text**,
- receive **precise error messages** with line and column information.

All ten TOML types are supported: `string`, `integer`, `float`, `boolean`, `offset-date-time`, `local-date-time`, `local-date`, `local-time`, `array`, `table`.

---

## How it works

The library is structured in three layers:

```
┌──────────────────────────────────────────────────────┐
│               Public API                             │
│    Document · Table · Array · Value                  │
└──────────────┬───────────────────────┬───────────────┘
               │                       │
       ┌───────▼──────┐       ┌────────▼───────┐
       │    Parser    │       │   Serializer   │
       └───────┬──────┘       └────────────────┘
               │
       ┌───────▼──────┐
       │    Source    │
       │  (&str-Scan) │
       └──────────────┘
```

### Parser (`src/parser.rs`)

The parser is a **recursive descent** operating directly on the UTF-8 input string. An internal `Source` struct tracks the current byte position as well as line and column for error messages. There is no separate lexer layer; tokenization and semantic analysis run together.

For each ABNF rule of the TOML grammar there is a dedicated method:
`parse_document` → `parse_keyval` → `parse_key` / `parse_value` → `parse_string_basic` / `parse_number_or_datetime` / `parse_array` / `parse_inline_table` etc.

A `ParseContext` struct tracks the state of each table path (`ExplicitlyDefined`, `ImplicitlyCreated`, `Inline`, `ArrayElement`) and thereby detects all constructs forbidden by the specification — duplicate keys, re-definition of tables, subsequent extension of inline tables.

### Data model (`src/value.rs`)

The core is the `Value` enum:

```rust
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    OffsetDateTime(OffsetDateTime),
    LocalDateTime(LocalDateTime),
    LocalDate(LocalDate),
    LocalTime(LocalTime),
    Array(Array),
    Table(Table),
}
```

`Table` is an ordered key-value map based on `IndexMap<String, Value>` — insertion order is preserved, lookup runs in O(1).

`Array` is a newtype over `Vec<Value>` and can hold mixed types.

### Serializer (`src/serializer.rs`)

The serializer traverses the data model and produces TOML text according to the following rules:

- Scalar values are emitted directly.
- Floats always include `.` or `e` (e.g. `1.0`, never `1`).
- Strings are emitted as basic strings `"…"`, with special characters escaped.
- Keys containing characters outside `A-Za-z0-9_-` are quoted.
- Nested tables appear as `[path]` headers.
- Arrays of tables appear as `[[path]]` headers.
- Very flat tables (≤ 4 entries, no sub-tables) can optionally be emitted as inline tables `{…}`.

---

## Integration into your projects

### 1. Dependency in `Cargo.toml`

Since `toml_dom` has not yet been published on [crates.io](https://crates.io), include it via a local path. Adjust the path to match your directory structure:

```toml
[dependencies]
toml_dom = { path = "../toml/rust" }
```

### 2. Import in your source file

```rust
use toml_dom::{Document, Value, TomlError};
use toml_dom::{Table, Array};
use toml_dom::{LocalDate, LocalTime, LocalDateTime, OffsetDateTime};
use toml_dom::SerializeOptions;
```

Or import everything at once with a glob import (recommended only for scripts/prototypes):

```rust
use toml_dom::*;
```

### 3. Minimal example

```rust
use toml_dom::{Document, Value};

fn main() -> Result<(), toml_dom::TomlError> {
    let mut doc = Document::parse("name = \"Welt\"\nversion = 1\n")?;
    println!("{}", doc.get::<String>("name")?);  // "Welt"

    doc.root_mut().insert("version", Value::Integer(2));
    println!("{}", doc.serialize());
    Ok(())
}
```

---

## The public API

### Document

`Document` is the central entry point. It holds the entire TOML document as a root `Table`.

#### Reading

```rust
// From a &str
let doc = Document::parse("key = \"value\"\n")?;

// From a file
let doc = Document::parse_file("config.toml")?;

// From an arbitrary reader (e.g. BufReader, Cursor, stdin)
use std::io::Cursor;
let reader = Cursor::new(b"x = 42\n");
let doc = Document::parse_reader(reader)?;
```

#### Building programmatically

`Document::from_table` creates a document from a fully populated `Table`, without parsing:

```rust
use toml_dom::{Document, Table, Value};

let mut root = Table::new();
root.insert("name", Value::String("My Project".into()));
root.insert("version", Value::Integer(1));

let doc = Document::from_table(root);
println!("{}", doc.serialize());
```

#### Reading — typed access

`doc.get::<T>(key)` reads a value directly from the root table and returns `&T`. If the key is not found or the type does not match, `Err(TomlError)` is returned.

```rust
let host: &String = doc.get::<String>("host")?;
let port: &i64    = doc.get::<i64>("port")?;
let debug: &bool  = doc.get::<bool>("debug")?;
```

Supported type parameters: `String`, `i64`, `f64`, `bool`, `LocalDate`, `LocalTime`, `LocalDateTime`, `OffsetDateTime`, `Array`, `Table`.

#### Reading — path access

`doc.path("a.b.c")` navigates through arbitrarily deeply nested tables using a dot-separated string. Returns `Option<&Value>`.

```rust
// [server]
// host = "example.com"
if let Some(val) = doc.path("server.host") {
    println!("{:?}", val);  // Value::String("example.com")
}
```

> **Note:** `path` splits the string at every `.`. Keys that themselves contain a dot (e.g. `"google.com"` as a quoted key) must be accessed with `root().get_path_segments(&["site", "google.com"])`.

#### Modifying

```rust
// Overwrite or insert a value
doc.root_mut().insert("debug", Value::Boolean(true));

// Insert a value via path (creates missing intermediate tables)
doc.root_mut().insert_path("server.port", Value::Integer(8080))?;

// Modify a value via path
if let Some(v) = doc.path_mut("server.port") {
    *v = Value::Integer(9090);
}

// Delete an entry
doc.root_mut().remove("debug");
```

#### Serializing and writing

```rust
// Emit as string (default options)
let toml_text: String = doc.serialize();

// Emit as string with custom options
let opts = SerializeOptions {
    sort_keys: true,
    ..Default::default()
};
let toml_text = doc.serialize_with(&opts);

// Write to file
doc.write_file("output.toml")?;
doc.write_file_with("output.toml", &opts)?;
```

---

### Table

`Table` stores key-value pairs in insertion order.

```rust
use toml_dom::{Table, Value};

let mut t = Table::new();
t.insert("host", Value::String("localhost".into()));
t.insert("port", Value::Integer(3000));
```

#### Method overview

| Method | Return type | Description |
|--------|-------------|-------------|
| `contains_key("key")` | `bool` | Check whether a key exists |
| `get("key")` | `Option<&Value>` | Read a value |
| `get_mut("key")` | `Option<&mut Value>` | Modify a value |
| `get_as::<T>("key")` | `Result<&T, TomlError>` | Typed access |
| `get_path("a.b.c")` | `Option<&Value>` | Path access (bare keys) |
| `get_path_mut("a.b")` | `Option<&mut Value>` | Mutable path access |
| `get_path_segments(&["a","b.c"])` | `Option<&Value>` | Path access with dots in keys |
| `get_path_segments_mut(&[…])` | `Option<&mut Value>` | Mutable segment path |
| `insert_path("a.b", val)` | `Result<…>` | Insert, creates intermediate tables |
| `insert_path_segments(&[…], val)` | `Result<…>` | Same, with dots in keys |
| `insert("key", val)` | `Option<Value>` | Insert / overwrite |
| `remove("key")` | `Option<Value>` | Delete |
| `keys()` | `Iterator<&str>` | All keys |
| `iter()` | `Iterator<(&str, &Value)>` | All entries |
| `iter_mut()` | `Iterator<(&str, &mut Value)>` | All entries (mutable) |
| `len()` / `is_empty()` | `usize` / `bool` | Size |
| `table["key"]` | `&Value` | Index operator (panics if not present) |

---

### Array

`Array` is an ordered list of `Value` elements. TOML allows mixed types.

```rust
use toml_dom::{Array, Value};

let mut arr = Array::new();
arr.push(Value::Integer(1));
arr.push(Value::Integer(2));
arr.push(Value::String("three".into()));

println!("{}", arr.len());      // 3
println!("{:?}", arr.get(0));   // Some(Value::Integer(1))

// Typed access for homogeneous arrays
let numbers: Vec<i64> = arr.as_typed::<i64>()?;
```

#### Method overview

| Method | Return type | Description |
|--------|-------------|-------------|
| `get(i)` | `Option<&Value>` | Read an element |
| `get_mut(i)` | `Option<&mut Value>` | Modify an element |
| `push(val)` | — | Append to the end |
| `insert(i, val)` | — | Insert at position |
| `remove(i)` | `Value` | Remove an element |
| `as_typed::<T>()` | `Result<Vec<T>>` | Convert a homogeneous array |
| `iter()` / `iter_mut()` | Iterator | Iteration |
| `len()` / `is_empty()` | `usize` / `bool` | Size |
| `arr[i]` | `&Value` | Index operator |
| `for v in &arr` | — | `IntoIterator` support |

---

### Value

`Value` is the universal TOML value type enum.

```rust
use toml_dom::Value;

let s = Value::String("Hello".into());
let n = Value::Integer(42);
let f = Value::Float(3.14);
let b = Value::Boolean(true);
```

**Query type name** (useful for error messages):

```rust
let name: &str = val.type_name();  // "string", "integer", "float", …
```

**Pattern matching:**

```rust
match val {
    Value::String(s)  => println!("String: {}", s),
    Value::Integer(n) => println!("Integer: {}", n),
    Value::Table(t)   => println!("{} entries", t.len()),
    Value::Array(a)   => println!("{} elements", a.len()),
    _                 => println!("Other type: {}", val.type_name()),
}
```

---

### Date and time types

The library defines four custom structs, since the Rust standard library has no timezone-free date/time values.

```rust
use toml_dom::{LocalDate, LocalTime, LocalDateTime, OffsetDateTime};

let d: &LocalDate = doc.get::<LocalDate>("birthday")?;
println!("{}-{:02}-{:02}", d.year, d.month, d.day);

let t: &LocalTime = doc.get::<LocalTime>("time")?;
println!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second);
// t.nanosecond: nanosecond fraction (at least millisecond precision)

let odt: &OffsetDateTime = doc.get::<OffsetDateTime>("created")?;
if odt.is_utc() { println!("UTC time"); }
```

**Display** in RFC 3339 format:

```rust
println!("{}", LocalDate { year: 2024, month: 3, day: 15 });  // "2024-03-15"
println!("{}", LocalTime { hour: 14, minute: 30, second: 0, nanosecond: 0 });  // "14:30:00"
```

**Conversion to/from `chrono`:**

```rust
let ld = LocalDate { year: 2024, month: 6, day: 1 };
let naive: chrono::NaiveDate = ld.into();
let back: LocalDate = naive.into();
```

| toml_dom type | chrono type |
|---|---|
| `LocalDate` | `chrono::NaiveDate` |
| `LocalTime` | `chrono::NaiveTime` |
| `LocalDateTime` | `chrono::NaiveDateTime` |
| `OffsetDateTime` | `chrono::DateTime<chrono::FixedOffset>` |

---

### Error handling

All fallible operations return `Result<T, TomlError>`.

```rust
use toml_dom::{TomlError, TomlErrorKind};

match Document::parse("invalid = \n") {
    Ok(doc) => { /* … */ }
    Err(e) => {
        eprintln!("{}", e);  // "2:1: unexpected newline"

        match &e.kind {
            TomlErrorKind::ParseError              => eprintln!("Syntax error"),
            TomlErrorKind::DuplicateKey            => eprintln!("Duplicate key"),
            TomlErrorKind::TypeError { expected, found } =>
                eprintln!("Wrong type: expected {expected}, found {found}"),
            TomlErrorKind::KeyNotFound(key)        => eprintln!("Key not found: {key}"),
            TomlErrorKind::IntegerOverflow         => eprintln!("Integer overflow"),
            TomlErrorKind::InvalidEscape(s)        => eprintln!("Invalid escape: {s}"),
            TomlErrorKind::Io(msg)                 => eprintln!("I/O error: {msg}"),
            _                                      => eprintln!("Other error"),
        }

        if let Some(loc) = &e.location {
            eprintln!("Line {}, column {}", loc.line, loc.column);
        }
    }
}
```

`TomlError` implements `std::error::Error` and is compatible with `anyhow`, `thiserror`, etc.:

```rust
use anyhow::Context;
let doc = Document::parse_file("config.toml")
    .context("Could not read configuration file")?;
```

---

### Serialization options

```rust
use toml_dom::SerializeOptions;

let opts = SerializeOptions {
    sort_keys:        false,        // keys in insertion order
    prefer_inline:    false,        // tables as [header], not {…}
    indent:           "    ".into(),// four spaces
    trailing_newline: true,         // end file with \n
};
```

| Option | Default | Description |
|--------|---------|-------------|
| `sort_keys` | `false` | Sort keys alphabetically |
| `prefer_inline` | `false` | Small tables as inline tables `{…}` |
| `indent` | `"    "` | Indentation for nested tables |
| `trailing_newline` | `true` | Trailing `\n` |

---

## Detailed examples

### Reading a configuration file

```rust
use toml_dom::{Document, Value};

fn main() -> Result<(), toml_dom::TomlError> {
    let doc = Document::parse_file("config.toml")?;

    let title   = doc.get::<String>("title")?;
    let version = doc.get::<i64>("version")?;

    let host = doc.path("database.host")
        .and_then(|v| if let Value::String(s) = v { Some(s.as_str()) } else { None })
        .unwrap_or("localhost");

    println!("{} v{} — DB: {}", title, version, host);
    Ok(())
}
```

### Building a document programmatically

```rust
use toml_dom::{Document, Table, Array, Value};

fn main() -> Result<(), toml_dom::TomlError> {
    let mut root = Table::new();
    root.insert("name",    Value::String("My App".into()));
    root.insert("version", Value::Integer(1));

    let mut server = Table::new();
    server.insert("host", Value::String("0.0.0.0".into()));
    server.insert("port", Value::Integer(8080));
    root.insert("server", Value::Table(server));

    let mut plugins = Array::new();
    let mut p1 = Table::new();
    p1.insert("name",    Value::String("auth".into()));
    p1.insert("enabled", Value::Boolean(true));
    plugins.push(Value::Table(p1));
    root.insert("plugin", Value::Array(plugins));

    Document::from_table(root).write_file("output.toml")?;
    Ok(())
}
```

### Reading, modifying, and writing back a document

```rust
use toml_dom::{Document, Value, SerializeOptions};

fn main() -> Result<(), toml_dom::TomlError> {
    let mut doc = Document::parse_file("config.toml")?;

    if let Some(Value::Integer(p)) = doc.path_mut("server.port") {
        *p += 1;
    }

    doc.root_mut().insert("modified_at", Value::String("2024-06-01".into()));

    let opts = SerializeOptions { sort_keys: true, ..Default::default() };
    doc.write_file_with("config.toml", &opts)?;
    Ok(())
}
```

### Reading an array of tables

```toml
# products.toml
[[product]]
name = "Hammer"
price = 9.99

[[product]]
name = "Saw"
price = 24.50
```

```rust
use toml_dom::{Document, Value, Array};

fn main() -> Result<(), toml_dom::TomlError> {
    let doc = Document::parse_file("products.toml")?;

    let arr = doc.get::<Array>("product")?;
    for item in arr {
        if let Value::Table(t) = item {
            let name  = t.get_as::<String>("name")?;
            let price = t.get_as::<f64>("price")?;
            println!("{}: {:.2}", name, price);
        }
    }
    Ok(())
}
```

### Keys with dots in their name

```toml
# special.toml
[site]
"google.com" = true
```

```rust
use toml_dom::Document;

fn main() -> Result<(), toml_dom::TomlError> {
    let doc = Document::parse_file("special.toml")?;

    // get_path would incorrectly split on "google" → "com" here.
    // Use get_path_segments instead:
    let val = doc.root().get_path_segments(&["site", "google.com"]);
    println!("{:?}", val);  // Some(Value::Boolean(true))
    Ok(())
}
```

---

## TOML 1.1 specifics

| Feature | Example |
|---------|---------|
| `\e` escape (U+001B, ESC) | `s = "\e[31m"` |
| `\xHH` escape (up to U+00FF) | `s = "\x41"` → `"A"` |
| Newlines in inline tables | `t = {\n  a = 1,\n  b = 2\n}` |
| Trailing comma in inline tables | `t = {a = 1, b = 2,}` |
| Seconds optional in time literals | `t = 14:30` (equivalent to `14:30:00`) |
| Seconds optional in datetime literals | `dt = 2024-03-15T14:30Z` |

---

## Project structure

```
rust/
├── Cargo.toml
├── README.md
├── fuzz/
│   ├── Cargo.toml
│   └── fuzz_targets/
│       └── fuzz_parse.rs   — fuzzing entry point for Document::parse
└── src/
    ├── lib.rs              — crate root, re-exports
    ├── error.rs            — TomlError, TomlErrorKind, SourceLocation
    ├── datetime.rs         — LocalDate, LocalTime, LocalDateTime, OffsetDateTime
    ├── value.rs            — Value, Array, Table, FromValue trait
    ├── parser.rs           — recursive descent, ParseContext
    ├── serializer.rs       — Serializer, SerializeOptions
    ├── document.rs         — Document
    └── tests/
        ├── mod.rs
        ├── test_datetime.rs
        ├── test_integer.rs
        ├── test_string.rs
        ├── test_float.rs
        ├── test_table.rs
        ├── test_serializer.rs
        └── test_roundtrip.rs
```

Run tests:

```sh
cargo test
```

Fuzzing (requires Nightly + cargo-fuzz):

```sh
cargo +nightly fuzz run fuzz_parse
```
