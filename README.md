---
editor_options: 
  markdown: 
    wrap: sentence
---

# toml_dom

A complete TOML 1.1 library in Rust for reading, editing, and writing TOML documents — with **format-preserving round-trip**.

------------------------------------------------------------------------

## Why toml_dom?

|   | `toml` (Cargo team) | `toml_edit` | **`toml_dom`** |
|----|----|----|----|
| TOML version | 1.0 | 1.0 | **1.1** |
| Read data | ✓ (via serde) | ✓ | ✓ |
| Edit data | ✗ | ✓ (format-preserving) | **✓ (format-preserving)** |
| Format-preserving round-trip | ✗ | ✓ | **✓** |
| Mutable DOM without serde | ✗ | limited | **✓** |
| Targeted path access | ✗ | ✗ | **✓** |
| No serde dependency | ✗ | ✗ | **✓** |

`toml_dom` targets applications that need to **programmatically read, modify, and write back** a TOML document — without serde derive macros, with full TOML 1.1 support, and while **exactly preserving** comments, formatting, and notation of all unmodified entries.

------------------------------------------------------------------------

## Table of Contents

-   [What this library does](#what-this-library-does)
-   [How it works](#how-it-works)
-   [Adding to your project](#adding-to-your-project)
-   [Public API](#public-api)
    -   [Document](#document)
    -   [Table](#table)
    -   [Array](#array)
    -   [Value](#value)
    -   [Date and Time Types](#date-and-time-types)
    -   [Error Handling](#error-handling)
    -   [Serialization Options](#serialization-options)
-   [Extended Examples](#extended-examples)
-   [TOML 1.1 Features](#toml-11-features)
-   [Project Structure](#project-structure)
-   [Changelog](#changelog)

------------------------------------------------------------------------

## What this library does {#what-this-library-does}

`toml_dom` implements the [TOML 1.1 specification](https://toml.io/en/v1.1.0) in full.
It allows other Rust programs to:

-   Parse TOML documents from **strings, files, or any `Read` source**,
-   hold the document in memory as a **mutable data model**,
-   **read, modify, add, and delete** individual values,
-   serialize the data model back to **valid TOML text**,
-   while doing so **exactly preserving** comments, blank lines, string notation, number representations, and all other formatting of unmodified entries,
-   receive **precise error messages** with line and column numbers.

All ten TOML types are supported: `string`, `integer`, `float`, `boolean`, `offset-date-time`, `local-date-time`, `local-date`, `local-time`, `array`, `table`.

### Format-preserving round-trip

What is preserved when a TOML file is parsed:

| Feature                     | Example                                |
|-----------------------------|----------------------------------------|
| Comments                    | `# database settings`                  |
| Inline comments             | `port = 8080  # default port`          |
| Blank lines between entries | structural spacing                     |
| String notation             | `'literal'`, `"""multiline"""`         |
| Number format               | `0xFF`, `0o755`, `0b1010`, `1_000_000` |
| Inline vs. block tables     | `{ a = 1 }` vs. `[section]`            |
| Multiline arrays            | indentation and line breaks            |
| Trailing commas             | `[1, 2, 3,]` (TOML 1.1)                |
| Whitespace around `=`       | `key  =  "value"`                      |

------------------------------------------------------------------------

## How it works {#how-it-works}

The library is built in four layers:

```         
┌──────────────────────────────────────────────────────┐
│                  Public API                          │
│    Document · Table · Array · Value                  │
└──────────────┬────────────────────────┬──────────────┘
               │                        │
       ┌───────▼──────┐        ┌────────▼───────┐
       │    Parser    │        │   Serializer   │
       │              │        │                │
       │  DOM tree    │        │  ① items path  │
       │  + items     │        │  ② DOM path    │
       └──────┬───────┘        └────────────────┘
              │
       ┌──────▼───────┐
       │     CST      │
       │ Vec<Document │
       │    Item>     │
       └──────────────┘
```

### Parser (`src/parser.rs`)

The parser is a **recursive descent** operating directly on the UTF-8 input string.
An internal `Source` struct tracks the current byte position as well as line and column for error messages.

Since v0.2, the parser records the **complete original source text** for every token — comment lines before a key, the key exactly as written, the whitespace around the `=` sign, the value as raw text (e.g. `0xFF` instead of `255`), the inline comment after it, and the line ending.
This metadata is stored in a flat `Vec<DocumentItem>`.

### CST layer (`src/cst.rs`)

`DocumentItem` is the central enum of the CST layer:

``` rust
pub enum DocumentItem {
    Entry {
        node: EntryNode,    // formatting metadata + recursive ValueNode
        path: Vec<String>,  // DOM path for lookup
    },
    Section(SectionNode),   // [header] or [[array-of-tables]]
    Eof(String),            // trailing whitespace/comments at end of file
}
```

`EntryNode` stores all formatting information for a single entry.
Its core is `node: ValueNode`, which represents the value recursively with full formatting depth:

``` rust
pub struct EntryNode {
    pub leading:  String,    // comments/blank lines before the key
    pub raw_key:  String,    // key as written in the source
    pub pre_eq:   String,    // whitespace before "="
    pub post_eq:  String,    // whitespace after "="
    pub node:     ValueNode, // value with formatting (recursive)
    pub trailing: String,    // inline comment + line ending
}
```

`ValueNode` distinguishes three cases:

``` rust
pub enum ValueNode {
    // Scalar: original source text + semantic value
    // raw = None → serializer regenerates in canonical form
    Scalar { raw: Option<String>, value: Value },
    // Array with per-element formatting
    Array(ArrayNode),
    // Inline table with per-entry formatting
    InlineTable(InlineTableNode),
}
```

For arrays, `ArrayNode` stores the opening `[`, all elements with their individual indentation, optional commas, and the closing `]`:

``` rust
pub struct ArrayNode {
    pub open:     String,
    pub elements: Vec<ArrayElement>,
    pub close:    String,           // whitespace before "]" + "]"
}

pub struct ArrayElement {
    pub leading:  String,           // whitespace/comments before the value
    pub node:     ValueNode,        // recursive
    pub trailing: String,           // whitespace after the value
    pub comma:    Option<String>,   // "," if present
}
```

Inline tables follow the same pattern (`InlineTableNode` / `InlineEntry`).
This recursive structure allows `set_value` to surgically modify individual entries of an inline table or individual array elements — all surrounding formatting is preserved byte-for-byte.

### Data model (`src/value.rs`)

The core is the `Value` enum:

``` rust
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

`Table` is an ordered key-value map backed by `IndexMap<String, Value>` — insertion order is preserved, lookups run in O(1).

`Array` is a newtype over `Vec<Value>` and supports mixed element types.

### Serializer (`src/serializer.rs`)

The serializer has two paths:

**Format-preserving path** (parsed document, no `sort_keys`/`prefer_inline`):\
Walks the `Vec<DocumentItem>` in source order and recursively emits the stored original text of each `ValueNode`.
Only nodes whose `raw` was cleared to `None` by `Document::set_value` are regenerated — at any nesting depth.

**Canonical DOM path** (programmatically created document, or `sort_keys`/`prefer_inline` set):\
Traverses the DOM tree and produces TOML text by these rules: - Floats always include `.` or `e` (e.g. `1.0`, never `1`).
- Strings are emitted as basic strings `"…"`, special characters are escaped.
- Keys containing characters outside `A-Za-z0-9_-` are quoted.
- Nested tables appear as `[path]` headers.
- Arrays of tables appear as `[[path]]` headers.
- Very flat tables (≤ 4 entries, no sub-tables) are written as inline tables `{…}`.

------------------------------------------------------------------------

## Adding to your project {#adding-to-your-project}

### 1. Dependency in `Cargo.toml`

``` toml
[dependencies]
toml_dom = "0.3"
```

### 2. Import in your source file

``` rust
use toml_dom::{Document, Value, TomlError};
use toml_dom::{Table, Array};
use toml_dom::{LocalDate, LocalTime, LocalDateTime, OffsetDateTime};
use toml_dom::SerializeOptions;
```

Or import everything at once with a glob import (recommended only for scripts/prototypes):

``` rust
use toml_dom::*;
```

### 3. Minimal example

``` rust
use toml_dom::{Document, Value};

fn main() -> Result<(), toml_dom::TomlError> {
    let mut doc = Document::parse("name = \"World\"\nversion = 1\n")?;
    println!("{}", doc.get::<String>("name")?);  // "World"

    doc.root_mut().insert("version", Value::Integer(2));
    println!("{}", doc.serialize());
    Ok(())
}
```

------------------------------------------------------------------------

## Public API {#public-api}

### Document {#document}

`Document` is the central entry point.
It holds the entire TOML document as a root `Table` and — for parsed documents — the `Vec<DocumentItem>` for format-preserving round-trip.

#### Parsing

``` rust
// From a &str
let doc = Document::parse("key = \"value\"\n")?;

// From a file
let doc = Document::parse_file("config.toml")?;

// From any Reader (e.g. BufReader, Cursor, stdin)
use std::io::Cursor;
let reader = Cursor::new(b"x = 42\n");
let doc = Document::parse_reader(reader)?;
```

#### Building programmatically

`Document::from_table` creates a document from a pre-built `Table` without parsing.
Such documents contain no formatting metadata and always serialize to canonical TOML.

``` rust
use toml_dom::{Document, Table, Value};

let mut root = Table::new();
root.insert("name", Value::String("My Project".into()));
root.insert("version", Value::Integer(1));

let doc = Document::from_table(root);
println!("{}", doc.serialize());
```

#### Reading — typed access

`doc.get::<T>(key)` reads a value directly from the root table and returns `&T`.
Returns `Err(TomlError)` if the key is absent or the type does not match.

``` rust
let host: &String = doc.get::<String>("host")?;
let port: &i64    = doc.get::<i64>("port")?;
let debug: &bool  = doc.get::<bool>("debug")?;
```

Supported type parameters: `String`, `i64`, `f64`, `bool`, `LocalDate`, `LocalTime`, `LocalDateTime`, `OffsetDateTime`, `Array`, `Table`.

#### Reading — path access

`doc.path("a.b.c")` navigates arbitrarily deep nested tables via a dot-separated string.
Returns `Option<&Value>`.

``` rust
// [server]
// host = "example.com"
if let Some(val) = doc.path("server.host") {
    println!("{:?}", val);  // Value::String("example.com")
}
```

> **Note:** `path` splits the string at every `.`.
> Keys that themselves contain a dot (e.g. `"google.com"` as a quoted key) must be accessed via `root().get_path_segments(&["site", "google.com"])`.

#### Mutating — format-preserving

Three methods change values format-preservingly; all return `true` when the path was found and updated.

**`set_value(&[segments], value)`** — universal method for any path.
The path is given as a slice of string literals to unambiguously distinguish keys from path separators — even when a key contains a literal dot.

``` rust
// Scalar entry:
// config.toml contains: port = 0x1F90  # hex: 8080
doc.set_value(&["port"], Value::Integer(9090));
// Result: port = 9090  # hex: 8080

// Entry inside an inline table:
// File contains: addr = { host = 'localhost', port = 8080 }
doc.set_value(&["addr", "port"], Value::Integer(9090));
// Result: addr = { host = 'localhost', port = 9090 }

// Nested path inside a section:
doc.set_value(&["server", "addr", "port"], Value::Integer(443));
```

**`set_path(dotted, value)`** — shorthand for simple paths without dots in key names, mirroring `doc.path("a.b.c")`.

``` rust
doc.set_path("server.port", Value::Integer(443));
// equivalent to: doc.set_value(&["server", "port"], Value::Integer(443))
```

> **Note:** `set_path` splits the string at every `.`.
> Keys that contain a dot must be changed via `set_value` with explicit segments.

**`set_element(path, index, value)`** — type-safe array element mutation.
The index is `usize`, not a string literal like `"1"` in `set_value`.

``` rust
// File contains: ids = [100, 200, 300]
doc.set_element(&["ids"], 1, Value::Integer(999));
// Result: ids = [100, 999, 300]
// Indentation, commas, and all other elements are preserved byte-for-byte.

// Array inside a section:
doc.set_element(&["data", "ids"], 0, Value::Integer(999));
```

`set_element` only works for arrays that are direct document entries.
For arrays nested inside inline tables, use `set_value` with a stringified index:
`doc.set_value(&["tbl", "arr", "0"], val)`.

#### Mutating — directly via DOM

`root_mut()` gives direct mutable access to the DOM tree.
Changes made this way are reliably written to the output; formatting metadata for modified entries is not automatically updated.

``` rust
// Insert or overwrite a value
doc.root_mut().insert("debug", Value::Boolean(true));

// Insert via path (creates intermediate tables as needed)
doc.root_mut().insert_path("server.port", Value::Integer(8080))?;

// Modify via path
if let Some(v) = doc.path_mut("server.port") {
    *v = Value::Integer(9090);
}

// Delete an entry
doc.root_mut().remove("debug");
```

#### Serializing and writing

``` rust
// To string (default options, format-preserving for parsed documents)
let toml_text: String = doc.serialize();

// To string with custom options
// Note: sort_keys/prefer_inline force the canonical DOM path
let opts = SerializeOptions {
    sort_keys: true,
    ..Default::default()
};
let toml_text = doc.serialize_with(&opts);

// Write to file
doc.write_file("output.toml")?;
doc.write_file_with("output.toml", &opts)?;
```

#### CST access

``` rust
// Read the raw items list (for advanced use cases)
for item in doc.items() {
    match item {
        toml_dom::DocumentItem::Entry { node, path } => {
            println!("key: {} → node: {:?}", node.raw_key, node.node);
        }
        toml_dom::DocumentItem::Section(s) => {
            println!("section: {}", s.raw);
        }
        toml_dom::DocumentItem::Eof(_) => {}
    }
}
```

------------------------------------------------------------------------

### Table {#table}

`Table` stores key-value pairs in insertion order.

``` rust
use toml_dom::{Table, Value};

let mut t = Table::new();
t.insert("host", Value::String("localhost".into()));
t.insert("port", Value::Integer(3000));
```

#### Method overview

| Method | Return type | Description |
|------------------------|------------------------|------------------------|
| `contains_key("key")` | `bool` | Check whether key exists |
| `get("key")` | `Option<&Value>` | Read a value |
| `get_mut("key")` | `Option<&mut Value>` | Modify a value |
| `get_as::<T>("key")` | `Result<&T, TomlError>` | Typed access |
| `get_path("a.b.c")` | `Option<&Value>` | Path access (bare keys) |
| `get_path_mut("a.b")` | `Option<&mut Value>` | Mutable path access |
| `get_path_segments(&["a","b.c"])` | `Option<&Value>` | Path access with dots in keys |
| `get_path_segments_mut(&[…])` | `Option<&mut Value>` | Mutable segment path |
| `insert_path("a.b", val)` | `Result<…>` | Insert, creating intermediate tables |
| `insert_path_segments(&[…], val)` | `Result<…>` | Same, with dots in keys |
| `insert("key", val)` | `Option<Value>` | Insert or overwrite |
| `remove("key")` | `Option<Value>` | Delete |
| `keys()` | `Iterator<&str>` | All keys |
| `iter()` | `Iterator<(&str, &Value)>` | All entries |
| `iter_mut()` | `Iterator<(&str, &mut Value)>` | All entries (mutable) |
| `len()` / `is_empty()` | `usize` / `bool` | Size |
| `table["key"]` | `&Value` | Index operator (panics if absent) |

------------------------------------------------------------------------

### Array {#array}

`Array` is an ordered list of `Value` elements.
TOML allows mixed element types.

``` rust
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

| Method                  | Return type          | Description               |
|-------------------------|----------------------|---------------------------|
| `get(i)`                | `Option<&Value>`     | Read an element           |
| `get_mut(i)`            | `Option<&mut Value>` | Modify an element         |
| `push(val)`             | —                    | Append to the end         |
| `insert(i, val)`        | —                    | Insert at position        |
| `remove(i)`             | `Value`              | Remove an element         |
| `as_typed::<T>()`       | `Result<Vec<T>>`     | Convert homogeneous array |
| `iter()` / `iter_mut()` | Iterator             | Iteration                 |
| `len()` / `is_empty()`  | `usize` / `bool`     | Size                      |
| `arr[i]`                | `&Value`             | Index operator            |
| `for v in &arr`         | —                    | `IntoIterator` support    |

------------------------------------------------------------------------

### Value {#value}

`Value` is the universal TOML value type enum.

``` rust
use toml_dom::Value;

let s = Value::String("Hello".into());
let n = Value::Integer(42);
let f = Value::Float(3.14);
let b = Value::Boolean(true);
```

**Query the type name** (useful for error messages):

``` rust
let name: &str = val.type_name();  // "string", "integer", "float", …
```

**Pattern matching:**

``` rust
match val {
    Value::String(s)  => println!("string: {}", s),
    Value::Integer(n) => println!("integer: {}", n),
    Value::Table(t)   => println!("{} entries", t.len()),
    Value::Array(a)   => println!("{} elements", a.len()),
    _                 => println!("other type: {}", val.type_name()),
}
```

------------------------------------------------------------------------

### Date and Time Types {#date-and-time-types}

The library defines four structs because the Rust standard library has no timezone-free date/time types.

``` rust
use toml_dom::{LocalDate, LocalTime, LocalDateTime, OffsetDateTime};

let d: &LocalDate = doc.get::<LocalDate>("birthday")?;
println!("{}-{:02}-{:02}", d.year, d.month, d.day);

let t: &LocalTime = doc.get::<LocalTime>("alarm")?;
println!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second);
// t.nanosecond: nanosecond fraction (at least millisecond precision)

let odt: &OffsetDateTime = doc.get::<OffsetDateTime>("created_at")?;
if odt.is_utc() { println!("UTC time"); }
```

**Display** in RFC 3339 format:

``` rust
println!("{}", LocalDate { year: 2024, month: 3, day: 15 });  // "2024-03-15"
println!("{}", LocalTime { hour: 14, minute: 30, second: 0, nanosecond: 0 });  // "14:30:00"
```

**Conversion to/from `chrono`:**

``` rust
let ld = LocalDate { year: 2024, month: 6, day: 1 };
let naive: chrono::NaiveDate = ld.into();
let back: LocalDate = naive.into();
```

| toml_dom type    | chrono type                             |
|------------------|-----------------------------------------|
| `LocalDate`      | `chrono::NaiveDate`                     |
| `LocalTime`      | `chrono::NaiveTime`                     |
| `LocalDateTime`  | `chrono::NaiveDateTime`                 |
| `OffsetDateTime` | `chrono::DateTime<chrono::FixedOffset>` |

------------------------------------------------------------------------

### Error Handling {#error-handling}

All fallible operations return `Result<T, TomlError>`.

``` rust
use toml_dom::{TomlError, TomlErrorKind};

match Document::parse("invalid = \n") {
    Ok(doc) => { /* … */ }
    Err(e) => {
        eprintln!("{}", e);  // "2:1: unexpected newline"

        match &e.kind {
            TomlErrorKind::ParseError              => eprintln!("syntax error"),
            TomlErrorKind::DuplicateKey            => eprintln!("duplicate key"),
            TomlErrorKind::TypeError { expected, found } =>
                eprintln!("type error: expected {expected}, found {found}"),
            TomlErrorKind::KeyNotFound(key)        => eprintln!("key not found: {key}"),
            TomlErrorKind::IntegerOverflow         => eprintln!("integer overflow"),
            TomlErrorKind::InvalidEscape(s)        => eprintln!("invalid escape: {s}"),
            TomlErrorKind::Io(msg)                 => eprintln!("I/O error: {msg}"),
            _                                      => eprintln!("other error"),
        }

        if let Some(loc) = &e.location {
            eprintln!("line {}, column {}", loc.line, loc.column);
        }
    }
}
```

`TomlError` implements `std::error::Error` and works with `anyhow`, `thiserror`, etc.:

``` rust
use anyhow::Context;
let doc = Document::parse_file("config.toml")
    .context("failed to read configuration file")?;
```

------------------------------------------------------------------------

### Serialization Options {#serialization-options}

``` rust
use toml_dom::SerializeOptions;

let opts = SerializeOptions {
    sort_keys:        false,        // keys in insertion order
    prefer_inline:    false,        // tables as [header], not {…}
    indent:           "    ".into(),// four spaces (canonical path only)
    trailing_newline: true,         // append final \n (canonical path only)
};
```

| Option | Default | Description |
|------------------------|------------------------|------------------------|
| `sort_keys` | `false` | Sort keys alphabetically (forces canonical path) |
| `prefer_inline` | `false` | Small tables as inline tables `{…}` (forces canonical path) |
| `indent` | `"    "` | Indentation for nested tables (canonical path only) |
| `trailing_newline` | `true` | Append final `\n` (canonical path only) |

> **Note:** `sort_keys: true` and `prefer_inline: true` force the canonical DOM path — even for parsed documents.
> Comments and formatting are lost in that case.
> For format-preserving output, use `doc.serialize()` without options.

------------------------------------------------------------------------

## Extended Examples {#extended-examples}

### Reading a configuration file

``` rust
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

``` rust
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

### Format-preserving edit and write-back

``` rust
use toml_dom::{Document, Value};

fn main() -> Result<(), toml_dom::TomlError> {
    // Source file config.toml:
    //
    //   # Server settings
    //   [server]
    //   host = 'localhost'   # IPv4 address
    //   port = 0x1F90        # hex: 8080
    //
    let mut doc = Document::parse_file("config.toml")?;

    // Change only the port — everything else stays byte-identical
    doc.set_value(&["server", "port"], Value::Integer(9090));

    doc.write_file("config.toml")?;
    // Result:
    //
    //   # Server settings
    //   [server]
    //   host = 'localhost'   # IPv4 address
    //   port = 9090          # hex: 8080
    Ok(())
}
```

### Reading and writing back via the DOM path

``` rust
use toml_dom::{Document, Value, SerializeOptions};

fn main() -> Result<(), toml_dom::TomlError> {
    let mut doc = Document::parse_file("config.toml")?;

    if let Some(Value::Integer(p)) = doc.path_mut("server.port") {
        *p += 1;
    }

    doc.root_mut().insert("updated_at", Value::String("2024-06-01".into()));

    let opts = SerializeOptions { sort_keys: true, ..Default::default() };
    doc.write_file_with("config.toml", &opts)?;
    Ok(())
}
```

### Reading an array of tables

``` toml
# products.toml
[[product]]
name = "Hammer"
price = 9.99

[[product]]
name = "Saw"
price = 24.50
```

``` rust
use toml_dom::{Document, Value, Array};

fn main() -> Result<(), toml_dom::TomlError> {
    let doc = Document::parse_file("products.toml")?;

    let arr = doc.get::<Array>("product")?;
    for item in arr {
        if let Value::Table(t) = item {
            let name  = t.get_as::<String>("name")?;
            let price = t.get_as::<f64>("price")?;
            println!("{}: ${:.2}", name, price);
        }
    }
    Ok(())
}
```

### Keys with a dot in their name

``` toml
# special.toml
[site]
"google.com" = true
```

``` rust
use toml_dom::Document;

fn main() -> Result<(), toml_dom::TomlError> {
    let doc = Document::parse_file("special.toml")?;

    // get_path would incorrectly split on "google" → "com".
    // Use get_path_segments instead:
    let val = doc.root().get_path_segments(&["site", "google.com"]);
    println!("{:?}", val);  // Some(Value::Boolean(true))
    Ok(())
}
```

------------------------------------------------------------------------

## TOML 1.1 Features

| Feature | Example |
|------------------------------------|------------------------------------|
| `\e` escape (U+001B, ESC) | `s = "\e[31m"` |
| `\xHH` escape (up to U+00FF) | `s = "\x41"` → `"A"` |
| Newlines in inline tables | `t = {\n  a = 1,\n  b = 2\n}` |
| Trailing comma in inline tables | `t = {a = 1, b = 2,}` |
| Optional seconds in time literals | `t = 14:30` (equivalent to `14:30:00`) |
| Optional seconds in datetime literals | `dt = 2024-03-15T14:30Z` |

------------------------------------------------------------------------

## Project Structure {#project-structure}

```         
rust/
├── Cargo.toml
├── README.md
├── fuzz/
│   ├── Cargo.toml
│   └── fuzz_targets/
│       ├── fuzz_parse.rs        — no crash on arbitrary UTF-8 input
│       ├── fuzz_roundtrip.rs    — idempotency: serialize → re-parse → re-serialize
│       └── fuzz_set_value.rs    — set_value output is always valid TOML
└── src/
    ├── lib.rs                   — crate root, re-exports
    ├── error.rs                 — TomlError, TomlErrorKind, SourceLocation
    ├── datetime.rs              — LocalDate, LocalTime, LocalDateTime, OffsetDateTime
    ├── value.rs                 — Value, Array, Table, FromValue trait
    ├── cst.rs                   — CST layer: DocumentItem, EntryNode, ValueNode, …
    ├── parser.rs                — recursive descent, ParseContext, raw text capture
    ├── serializer.rs            — serializer (format-preserving + canonical), SerializeOptions
    ├── document.rs              — Document, set_value, items()
    └── tests/
        ├── mod.rs
        ├── test_datetime.rs
        ├── test_integer.rs
        ├── test_string.rs
        ├── test_float.rs
        ├── test_table.rs
        ├── test_serializer.rs
        ├── test_roundtrip.rs
        └── test_format_preserve.rs — format-preserving round-trip tests
```

Run tests:

``` sh
cargo test
```

Fuzzing (requires Nightly + cargo-fuzz):

``` sh
cargo +nightly fuzz run fuzz_parse
cargo +nightly fuzz run fuzz_roundtrip
cargo +nightly fuzz run fuzz_set_value
```

------------------------------------------------------------------------

## Changelog {#changelog}

### v0.3.1

**New methods on `Document`:**

- **`set_path(dotted: &str, value: Value) -> bool`** — shorthand for `set_value` that accepts a dot-separated path string (mirroring `doc.path("a.b.c")`).
  Reduces boilerplate for simple paths.
  Same limitation as `path()`: keys containing a dot must be changed via `set_value` with explicit segments.

- **`set_element(path: &[&str], index: usize, value: Value) -> bool`** — type-safe array element mutation.
  The index is `usize` instead of a string literal `"1"` as required by `set_value`.
  Returns `false` when the path does not exist, is not an array, or the index is out of bounds.

**Unchanged:** public API fully backwards-compatible with v0.3.0.

---

### v0.3.0

**Main feature: recursive `ValueNode` — inline tables and arrays are no longer opaque**

Previously, `EntryNode` stored the entire value of an entry as a single raw string (`raw_value: Option<String>`).
This allowed inline tables and arrays to be emitted format-preservingly, but not surgically modified.

From v0.3, every value is a `ValueNode` tree:

-   `ValueNode::Scalar` — scalar with original source text and semantic value
-   `ValueNode::Array(ArrayNode)` — array with per-element formatting (`ArrayElement`)
-   `ValueNode::InlineTable(InlineTableNode)` — inline table with per-entry formatting (`InlineEntry`)

**`Document::set_value` now navigates the entire tree:**

-   `set_value(&["point", "x"], val)` for `point = { x = 1, y = 2 }` → changes only `x`, preserves `y`, braces, commas, and whitespace
-   `set_value(&["ids", "1"], val)` for `ids = [100, 200, 300]` → changes only element 1
-   Arbitrary depth: `set_value(&["server", "addr", "port"], val)` for `addr = { host = 'localhost', port = 8080 }` inside section `[server]`

**New public types** (in `src/cst.rs`, re-exported from the crate root):

-   `ValueNode` — recursive value enum (Scalar / Array / InlineTable)
-   `ArrayNode` — array with `open`, `elements: Vec<ArrayElement>`, `close`
-   `ArrayElement` — one array element with `leading`, `node`, `trailing`, `comma`
-   `InlineTableNode` — inline table with `open`, `entries: Vec<InlineEntry>`, `close`
-   `InlineEntry` — one inline table entry with `leading`, `raw_key`, `pre_eq`, `post_eq`, `node`, `trailing`, `comma`

**Breaking change:**

-   `EntryNode.raw_value: Option<String>` → `EntryNode.node: ValueNode`

This is a breaking change for code that directly accesses `EntryNode.raw_value`.
All other public types (`Value`, `Table`, `Array`, `FromValue`, all error types) are unchanged.

**Parser bugfix:**

`raw_key` incorrectly included the whitespace that `parse_key` consumes internally when looking ahead for a `.`.
Consequence: `set_value` on inline table entries returned `false` (key comparison `"x "` ≠ `"x"`).
Fix: whitespace is correctly moved from `raw_key` into `pre_eq`.

------------------------------------------------------------------------

### v0.2.1

**Bug fixes** — found by the new fuzz targets (`fuzz_roundtrip`, `fuzz_set_value`):

-   **Idempotency broken for numeric keys** (`src/serializer.rs`): `lookup_path` incorrectly treated any path segment parseable as an integer (e.g. `"6"` in the key path `-.6.-`) as an array index — even when the current value was a table.
    Consequence: the entry appeared deleted from the DOM; the serializer emitted duplicate section headers instead; the second serialize pass differed from the first.
    Fix: the current value's type is checked first — array index access only for `Value::Array`.

-   **Section paths unknown to the serializer** (`src/serializer.rs`): `append_new_dom_values` only knew entry paths from the items list, not section paths (`[table]`, `[[array]]`).
    Consequence: tables already emitted via section items were re-emitted as new `[section]` headers; on re-parsing this triggered a duplicate-key error.
    Fix: section paths are added to the `covered` set; the array-of-tables loop exits early when the array path itself is covered.

**New fuzz targets:**

-   `fuzz_roundtrip` — checks idempotency: `parse → serialize → re-parse → re-serialize` must be byte-identical
-   `fuzz_set_value` — checks mutation safety: `parse → set_value → serialize` must always produce valid TOML with the updated value

**Unchanged:** public API fully backwards-compatible with v0.2.0.

------------------------------------------------------------------------

### v0.2.0

**Main feature: format-preserving round-trip**

The parser now records the complete original source text for every token.
During serialization these raw texts are emitted directly, so comments, formatting, and notation of all unmodified entries are preserved exactly.

**New public types** (in `src/cst.rs`, re-exported from the crate root):

-   `DocumentItem` — flat list of all source elements (entries, section headers, end of file)
-   `EntryNode` — formatting metadata for a key-value pair: `leading`, `raw_key`, `pre_eq`, `post_eq`, `raw_value`, `trailing`
-   `SectionNode` — section header `[…]` or `[[…]]` with `leading`, `raw`, `trailing`, `path`, `is_array`

**New methods on `Document`:**

-   `Document::set_value(&["path", "to", "key"], value)` — changes a value format-preservingly: only that value is regenerated, all other formatting is untouched
-   `Document::items() -> &[DocumentItem]` — read access to the CST items list

**Serializer behaviour:**

-   Parsed document without `sort_keys`/`prefer_inline` → format-preserving path (new)
-   `sort_keys: true` or `prefer_inline: true` → canonical DOM path (as in v0.1)
-   Programmatically created document → canonical DOM path (as in v0.1)
-   `trailing_newline` now only affects the canonical path; the format-preserving path reproduces the original line ending

**Bug fix:**

-   Crash in `parse_time_str` for incomplete seconds fields (e.g. `13:63:` with no following digits) — found by fuzzing

**Unchanged:**

`Value`, `Table`, `Array`, `FromValue`, `TomlError`, `TomlErrorKind`, `SourceLocation` — fully backwards-compatible.

------------------------------------------------------------------------

### v0.1.0

Initial release:

-   Complete TOML 1.1 implementation (parser + serializer)
-   All ten TOML types
-   `IndexMap`-backed `Table` (insertion order preserved)
-   Chrono conversions for all four datetime types
-   Path access (`get_path`, `get_path_segments`) with support for dots in key names
-   `SerializeOptions` (key sorting, inline preference, indentation)
-   Precise error messages with line and column numbers
-   cargo-fuzz integration
