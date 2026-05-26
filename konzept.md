# Konzept: TOML-Library in Rust

Zielversion: TOML 1.1.0  
Rust-Edition: 2021 (Minimum Rust 1.70)

---

## 1. Ziele und Abgrenzung

Die Library soll:

- TOML-Dokumente aus `&str`, `String`, `Read`-Streams oder Dateipfaden **einlesen** (parsen)
- Das Dokument im Speicher als veränderliches **Datenmodell** halten
- Einzelne Werte **lesen, ändern, hinzufügen und löschen**
- Das Datenmodell wieder als **TOML-Text serialisieren** (schreiben)
- Alle Typen aus der TOML-1.1-Spezifikation vollständig abbilden
- Präzise **Fehlermeldungen** mit Zeile/Spalte liefern
- **Keine unsafe-Blöcke** in der öffentlichen Implementierung
- Als **Single-Crate** verwendbar sein (kein Workspace erforderlich)

Externe Abhängigkeiten:
- `indexmap` – für die geordnete Schlüsselspeicherung in `Table`
- `chrono` – für Konvertierung in `chrono`-Datums-/Zeittypen

Nicht in Scope:
- JSON- oder YAML-Kompatibilitätsmodus
- Schema-Validierung
- Streaming-Parser für sehr große Dateien (> 100 MB)
- `serde`-Integration (kann als separates `toml-serde`-Crate folgen)

---

## 2. Architektur-Überblick

```
┌────────────────────────────────────────────────────┐
│                  Öffentliche API                   │
│      Document, Table, Array, Value                 │
└────────────┬──────────────────────┬────────────────┘
             │                      │
     ┌───────▼──────┐      ┌────────▼───────┐
     │    Parser    │      │   Serializer   │
     │ (Lesen/AST) │      │  (Schreiben)   │
     └───────┬──────┘      └────────────────┘
             │
     ┌───────▼──────┐
     │    Lexer     │
     │ (Tokenizer)  │
     └───────┬──────┘
             │
     ┌───────▼──────┐
     │  UTF-8-Input │
     │ (&str / File)│
     └──────────────┘
```

| Schicht | Aufgabe |
|---------|---------|
| **Lexer** | Zerlegt den UTF-8-Text in Token (Schlüssel, Werte, Trennzeichen …) |
| **Parser** | Verarbeitet die Tokenfolge nach der ABNF-Grammatik; baut das Datenmodell auf |
| **Serializer** | Traversiert das Datenmodell und erzeugt wieder valides TOML |

---

## 3. TOML-Typensystem → Rust-Typen

| TOML-Typ | Rust-Repräsentation |
|----------|---------------------|
| String (alle 4 Varianten) | `String` (UTF-8) |
| Integer | `i64` |
| Float | `f64` (IEEE 754 binary64) |
| Boolean | `bool` |
| Offset Date-Time | `OffsetDateTime` (eigener Struct) |
| Local Date-Time | `LocalDateTime` (eigener Struct) |
| Local Date | `LocalDate` (eigener Struct) |
| Local Time | `LocalTime` (eigener Struct) |
| Array | `Array` (= `Vec<Value>`) |
| Table / Inline Table | `Table` (geordnete Map) |
| Array of Tables | `Array` aus `Table`-Elementen |

Das Herzstück ist ein `Value`-Enum, das jeden TOML-Wert halten kann:

```rust
/// Alle möglichen TOML-Werttypen.
#[derive(Debug, Clone, PartialEq)]
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

`Array` und `Table` werden als Wert-Typen gehalten (nicht als `Box` oder `Rc`),
weil Rust's Ownership-Modell Zyklen bereits verhindert und der Enum selbst
über den Heap alloziert wird, sobald er in eine `Vec` oder `HashMap` gesteckt wird.

---

## 4. Datums- und Zeittypen

Die Rust-Standardbibliothek kennt keine lokalen Datums-/Zeitwerte ohne
Zeitzone. Eigene Structs werden definiert:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalDate {
    pub year:  i32,   // 4-stellig
    pub month: u8,    // 1–12
    pub day:   u8,    // 1–31
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub hour:       u8,  // 0–23
    pub minute:     u8,  // 0–59
    pub second:     u8,  // 0–60 (Schaltsekunde); Default 0 (TOML 1.1)
    pub nanosecond: u32, // 0–999_999_999 (min. ms-Genauigkeit gefordert)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDateTime {
    pub date: LocalDate,
    pub time: LocalTime,
}

/// `offset_minutes == i32::MIN` kodiert "Z" (UTC ohne expliziten Offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetDateTime {
    pub date:           LocalDate,
    pub time:           LocalTime,
    pub offset_minutes: i32,  // UTC-Offset in Minuten; i32::MIN = 'Z'
}

impl OffsetDateTime {
    pub const UTC_OFFSET: i32 = i32::MIN;

    pub fn is_utc(self) -> bool {
        self.offset_minutes == Self::UTC_OFFSET
    }
}
```

Hilfsmethoden für Anzeige und Vergleich werden auf den Structs implementiert.
Zusätzlich werden `From`/`Into`-Implementierungen für `chrono::NaiveDate`,
`chrono::NaiveTime`, `chrono::NaiveDateTime` und
`chrono::DateTime<chrono::FixedOffset>` bereitgestellt.

---

## 5. Datenmodell: Table und Array

### 5.1 Table

Eine TOML-Table ist eine geordnete Schlüssel-Wert-Zuordnung.
Die Einfügereihenfolge muss für die Serialisierung erhalten bleiben.

```rust
/// Geordnete Schlüssel-Wert-Tabelle.
///
/// Intern: `IndexMap<String, Value>` – erhält die Einfügereihenfolge
/// und bietet O(1)-Suche.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    inner: indexmap::IndexMap<String, Value>,
}

impl Table {
    pub fn new() -> Self { Self::default() }

    // ── Lesender Zugriff ──────────────────────────────────────────────────

    pub fn contains_key(&self, key: &str) -> bool;
    pub fn get(&self, key: &str) -> Option<&Value>;
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value>;

    /// Typisierter Zugriff; gibt `Err(TypeError)` bei falschem Typ.
    pub fn get_as<T: FromValue>(&self, key: &str) -> Result<&T, TomlError>;

    // ── Schreibender Zugriff ──────────────────────────────────────────────

    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value>;
    pub fn remove(&mut self, key: &str) -> Option<Value>;

    // ── Pfadzugriff mit Punkt-Notation: "server.host" ─────────────────────

    pub fn get_path(&self, dotted_key: &str) -> Option<&Value>;
    pub fn get_path_mut(&mut self, dotted_key: &str) -> Option<&mut Value>;
    pub fn insert_path(&mut self, dotted_key: &str, value: Value)
        -> Result<Option<Value>, TomlError>;

    // ── Iteration (Einfügereihenfolge) ────────────────────────────────────

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)>;
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut Value)>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn keys(&self) -> impl Iterator<Item = &str>;
}
```

**Index-Operator:** Über `std::ops::Index<&str>` und `IndexMut<&str>`
kann mit `table["key"]` zugegriffen werden. `Index` panikt bei fehlendem
Schlüssel; `get` gibt `Option` zurück – beides idiomatic Rust.

### 5.2 Array

```rust
/// Geordnete Liste von TOML-Werten.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Array(Vec<Value>);

impl Array {
    pub fn new() -> Self { Self::default() }

    pub fn get(&self, index: usize) -> Option<&Value>;
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Value>;

    pub fn push(&mut self, value: Value);
    pub fn insert(&mut self, index: usize, value: Value);
    pub fn remove(&mut self, index: usize) -> Value;

    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;

    pub fn iter(&self) -> impl Iterator<Item = &Value>;
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Value>;

    /// Gibt `Err(TypeError)` wenn die Elemente nicht alle vom Typ `T` sind.
    pub fn as_typed<T: FromValue + Clone>(&self) -> Result<Vec<T>, TomlError>;
}

impl std::ops::Index<usize> for Array { type Output = Value; … }
impl std::ops::IndexMut<usize> for Array { … }

/// Ermöglicht `for v in &array { … }`.
impl<'a> IntoIterator for &'a Array { … }
```

---

## 6. Trait: FromValue und IntoValue

Anstelle von Template-Methoden aus C++ werden in Rust Traits verwendet:

```rust
/// Auslesen eines konkreten Typs aus einem `&Value`.
pub trait FromValue: Sized {
    fn from_value(v: &Value) -> Result<&Self, TomlError>;
}

/// Konvertierung eines Rust-Typs in einen `Value`.
pub trait IntoValue {
    fn into_value(self) -> Value;
}
```

Implementierungen werden für alle primitiven TOML-Typen mitgeliefert:

```rust
impl FromValue for String        { … }
impl FromValue for i64           { … }
impl FromValue for f64           { … }
impl FromValue for bool          { … }
impl FromValue for LocalDate     { … }
impl FromValue for LocalTime     { … }
impl FromValue for LocalDateTime { … }
impl FromValue for OffsetDateTime{ … }
impl FromValue for Array         { … }
impl FromValue for Table         { … }
```

---

## 7. Öffentliche API: Document

`Document` ist der Einstiegspunkt für alle Operationen.

```rust
/// Repr äsentiert ein vollständiges TOML-Dokument.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    root: Table,
}

impl Document {
    // ── Einlesen ──────────────────────────────────────────────────────────

    /// Parst TOML aus einem `&str`.
    pub fn parse(text: &str) -> Result<Self, TomlError>;

    /// Parst TOML aus einem `impl Read`.
    pub fn parse_reader(reader: impl std::io::Read) -> Result<Self, TomlError>;

    /// Liest und parst eine Datei.
    pub fn parse_file(path: impl AsRef<std::path::Path>) -> Result<Self, TomlError>;

    // ── Zugriff auf den Root-Table ────────────────────────────────────────

    pub fn root(&self) -> &Table;
    pub fn root_mut(&mut self) -> &mut Table;

    /// Kurzform für `self.root().get_as::<T>(key)`.
    pub fn get<T: FromValue>(&self, key: &str) -> Result<&T, TomlError>;

    /// Pfadzugriff über den Root-Table: `doc.path("server.host")`.
    pub fn path(&self, dotted: &str) -> Option<&Value>;
    pub fn path_mut(&mut self, dotted: &str) -> Option<&mut Value>;

    // ── Serialisieren / Schreiben ─────────────────────────────────────────

    /// Gibt das Dokument als TOML-String zurück.
    pub fn serialize(&self) -> String;

    /// Serialisiert mit Optionen.
    pub fn serialize_with(&self, opts: &SerializeOptions) -> String;

    /// Schreibt das Dokument in eine Datei.
    pub fn write_file(&self, path: impl AsRef<std::path::Path>)
        -> Result<(), TomlError>;

    pub fn write_file_with(
        &self,
        path: impl AsRef<std::path::Path>,
        opts: &SerializeOptions,
    ) -> Result<(), TomlError>;

    // ── Metadaten ─────────────────────────────────────────────────────────

    /// TOML-Versionsnummer, die diese Library implementiert.
    pub const TOML_VERSION: (u32, u32, u32) = (1, 1, 0);
}

/// Optionen für die Serialisierung.
#[derive(Debug, Clone)]
pub struct SerializeOptions {
    /// Schlüssel alphabetisch sortieren (Standard: false).
    pub sort_keys: bool,
    /// Tables bevorzugt als Inline-Tables `{…}` schreiben (Standard: false).
    pub prefer_inline: bool,
    /// Einrückungsstring für verschachtelte Tables (Standard: vier Leerzeichen).
    pub indent: String,
    /// Abschließenden Zeilenumbruch hinzufügen (Standard: true).
    pub trailing_newline: bool,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            sort_keys:        false,
            prefer_inline:    false,
            indent:           "    ".to_owned(),
            trailing_newline: true,
        }
    }
}
```

---

## 8. Lexer-Design

Der Lexer arbeitet auf einem `&str` und liefert `Token`-Werte. Er hält
nur eine Byte-Position, keine Kopie des Eingabepuffers.

### 8.1 Token-Typen

```rust
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    // Schlüssel
    UnquotedKey,        // bare key: A-Za-z0-9_-
    BasicString,        // "..."
    MlBasicString,      // """..."""
    LiteralString,      // '...'
    MlLiteralString,    // '''...'''

    // Werte
    Integer,            // +99 / 0xDEAD / 0o755 / 0b1010
    Float,              // 3.14 / 1e6 / inf / nan
    BoolTrue,           // true
    BoolFalse,          // false
    OffsetDateTime,     // 1979-05-27T07:32:00Z
    LocalDateTime,      // 1979-05-27T07:32:00
    LocalDate,          // 1979-05-27
    LocalTime,          // 07:32:00 / 07:32

    // Struktur
    Equals,             // =
    Dot,                // .
    Comma,              // ,
    LBracket,           // [
    RBracket,           // ]
    DoubleLBracket,     // [[
    DoubleRBracket,     // ]]
    LBrace,             // {
    RBrace,             // }
    Newline,            // \n oder \r\n
    Comment,            // # ...

    Eof,
}

#[derive(Debug, Clone)]
pub(crate) struct Token<'src> {
    pub kind:   TokenKind,
    /// Rohtext aus dem Originalpuffer (keine Kopie, daher Lifetime 'src).
    pub text:   &'src str,
    pub line:   u32,
    pub column: u32,
}
```

### 8.2 Lexer-Struct

```rust
pub(crate) struct Lexer<'src> {
    src:  &'src str,
    pos:  usize,    // Byte-Position im UTF-8-String
    line: u32,
    col:  u32,
    // Lookahead-Puffer für peek()
    peeked: Option<Token<'src>>,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self;

    /// Gibt das nächste Token zurück und konsumiert es.
    pub fn next(&mut self) -> Result<Token<'src>, TomlError>;

    /// Gibt das nächste Token zurück ohne es zu konsumieren.
    pub fn peek(&mut self) -> Result<&Token<'src>, TomlError>;

    // ── Private Hilfsmethoden ─────────────────────────────────────────────

    fn current_char(&self) -> Option<char>;
    fn advance(&mut self);
    fn advance_by(&mut self, n: usize);
    fn read_basic_string(&mut self) -> Result<Token<'src>, TomlError>;
    fn read_ml_basic_string(&mut self) -> Result<Token<'src>, TomlError>;
    fn read_literal_string(&mut self) -> Result<Token<'src>, TomlError>;
    fn read_ml_literal_string(&mut self) -> Result<Token<'src>, TomlError>;
    fn read_number_or_datetime(&mut self) -> Result<Token<'src>, TomlError>;
    fn read_key_or_keyword(&mut self) -> Result<Token<'src>, TomlError>;
    fn skip_whitespace_inline(&mut self);
}
```

**Besondere Lexer-Herausforderungen:**

- Datum-/Zeit-Token sehen aus wie `1979-05-27T07:32:00Z` — der Lexer muss
  erkennen, wann eine Ziffernfolge ein Datum ist und kein Integer.
- `[[` und `]]` sind zwei benachbarte Einzelzeichen; ein Lookahead wird benötigt.
- Multi-Line-Strings erlauben bis zu zwei schließende Anführungszeichen
  innerhalb des Strings (`"""a""b"""`).
- In TOML 1.1 sind `\e` (U+001B) und `\xHH` neue Escape-Sequenzen.
- In TOML 1.1 dürfen Inline-Tables Zeilenumbrüche enthalten; der Lexer muss
  innerhalb von `{…}` Newlines nicht als Anweisungsende behandeln.

---

## 9. Parser-Design

Der Parser ist ein **rekursiver Abstieg** direkt nach der ABNF-Grammatik.
Er konsumiert Token vom Lexer und baut das Datenmodell auf.

### 9.1 Parser-Struct

```rust
struct Parser<'src> {
    lexer:   Lexer<'src>,
    /// Tracking-Struktur für Semantikregeln (Doppelschlüssel, Array-of-Tables …)
    context: ParseContext,
}

impl<'src> Parser<'src> {
    fn new(src: &'src str) -> Self;
    fn parse(mut self) -> Result<Document, TomlError>;

    // ── Grammatik-Regeln (je eine Methode pro ABNF-Regel) ─────────────────

    fn parse_document(&mut self, root: &mut Table) -> Result<(), TomlError>;
    fn parse_expression(
        &mut self,
        current: &mut Table,
        root: &mut Table,
    ) -> Result<(), TomlError>;
    fn parse_keyval(&mut self, target: &mut Table) -> Result<(), TomlError>;
    fn parse_key(&mut self) -> Result<Vec<String>, TomlError>;
    fn parse_simple_key(&mut self) -> Result<String, TomlError>;
    fn parse_val(&mut self) -> Result<Value, TomlError>;

    fn parse_string(&mut self, tok: &Token) -> Result<String, TomlError>;
    fn parse_integer(&mut self, tok: &Token) -> Result<i64, TomlError>;
    fn parse_float(&mut self, tok: &Token) -> Result<f64, TomlError>;
    fn parse_datetime(&mut self, tok: &Token)
        -> Result<Value, TomlError>;  // kann 4 Datetime-Typen zurückgeben
    fn parse_array(&mut self) -> Result<Array, TomlError>;
    fn parse_inline_table(&mut self) -> Result<Table, TomlError>;

    /// Navigiert (und erzeugt ggf.) die Sub-Tables für einen Pfad.
    fn resolve_table_path<'t>(
        root:  &'t mut Table,
        path:  &[String],
        ctx:   &mut ParseContext,
        kind:  TablePathKind,   // Standard | ArrayElement
    ) -> Result<&'t mut Table, TomlError>;

    fn expect(&mut self, kind: TokenKind) -> Result<Token, TomlError>;
}
```

### 9.2 ParseContext

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableStatus {
    /// Per `[header]` explizit eingeführt.
    ExplicitlyDefined,
    /// Implizit durch dotted keys oder Eltern-Tabellen erzeugt.
    ImplicitlyCreated,
    /// Inline-Table `{…}` – darf danach nicht mehr erweitert werden.
    Inline,
    /// Element eines `[[array of tables]]`.
    ArrayElement,
}

struct ParseContext {
    /// Jede bekannte Table-Adresse (Punkt-getrennt) → Zustand
    known: std::collections::HashMap<String, TableStatus>,
}

impl ParseContext {
    fn is_known(&self, path: &str) -> bool;
    fn mark(&mut self, path: &str, status: TableStatus)
        -> Result<(), TomlError>;   // Err bei Konflikten
}
```

### 9.3 Verarbeitungsreihenfolge (vereinfacht)

```
parse_document(root):
    current_table = &mut root
    loop:
        match next expression:
            keyval          → parse_keyval(current_table)
            [table]         → current_table = resolve or create sub-table
                              mark as ExplicitlyDefined
            [[array-table]] → append new Table to Array at path
                              current_table = last element of that Array
            EOF             → break
```

---

## 10. Serializer-Design

```rust
struct Serializer<'opts> {
    opts: &'opts SerializeOptions,
}

impl<'opts> Serializer<'opts> {
    fn new(opts: &'opts SerializeOptions) -> Self;
    fn serialize(&self, doc: &Document) -> String;

    fn write_table(
        &self,
        out:         &mut String,
        table:       &Table,
        path_prefix: &str,
        depth:       usize,
    );
    fn write_value(&self, out: &mut String, val: &Value, depth: usize);
    fn write_array(&self, out: &mut String, arr: &Array, depth: usize);
    fn write_inline_table(&self, out: &mut String, tbl: &Table);
    fn write_string(&self, out: &mut String, s: &str);
    fn write_key(&self, out: &mut String, key: &str);

    fn needs_quoting(key: &str) -> bool;
    fn is_array_of_tables(val: &Value) -> bool;
    fn prefer_inline_table(tbl: &Table, depth: usize) -> bool;
}
```

**Serialisierungsregeln (in Prioritätsreihenfolge):**

1. Skalare Werte (String, Integer, Float, Boolean, Datetime) direkt schreiben.
2. Inline-Tables `{…}` verwenden, wenn `opts.prefer_inline == true` oder
   die Table sehr flach ist (keine verschachtelten Tables, ≤ 4 Einträge).
3. Standard-Tables `[path]` für alle anderen Tables.
4. Array-of-Tables mit `[[path]]`-Headern.
5. Strings escapen: bevorzugt Basic-String `"…"`, Literal `'…'` nur wenn
   der String Backslashes oder andere unescapte Sonderzeichen enthält.
6. Schlüssel mit Zeichen außerhalb von `A-Za-z0-9_-` in Anführungszeichen setzen.
7. Float-Werte müssen immer einen `.` oder `e` enthalten (`1.0`, nie `1`).
8. `inf`, `-inf`, `nan` für Spezial-Float-Werte.

---

## 11. Fehlerbehandlung

```rust
/// Quellenposition für Fehlermeldungen.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub line:        u32,
    pub column:      u32,
    pub source_file: Option<String>, // None bei Strings/Streams
}

/// Alle möglichen Fehlerarten der Library.
#[derive(Debug, Clone)]
pub enum TomlErrorKind {
    /// Syntaxfehler beim Parsen.
    ParseError,
    /// Schlüssel mehrfach definiert oder Tabellenkonflikte.
    DuplicateKey,
    /// Erwarteter Typ stimmt nicht mit tatsächlichem Werttyp überein.
    TypeError { expected: &'static str, found: &'static str },
    /// Schlüssel existiert nicht.
    KeyNotFound(String),
    /// Integer-Überlauf (außerhalb von i64).
    IntegerOverflow,
    /// Ungültige Escape-Sequenz in einem String.
    InvalidEscape(String),
    /// Ungültiger UTF-8-Eingabe.
    InvalidUtf8,
    /// I/O-Fehler beim Lesen/Schreiben von Dateien.
    Io(String),
    /// Fehler bei der Serialisierung.
    SerializeError(String),
}

#[derive(Debug, Clone)]
pub struct TomlError {
    pub kind:     TomlErrorKind,
    pub message:  String,
    pub location: Option<SourceLocation>,
}

impl TomlError {
    pub fn formatted(&self) -> String; // "datei.toml:12:5: ungültiger Escape \q"
}

impl std::fmt::Display for TomlError { … }
impl std::error::Error for TomlError {}

// Fehler-Umwandlungen
impl From<std::io::Error> for TomlError { … }
```

**Kein `unwrap()` in der öffentlichen API.** Alle fehlbaren Operationen geben
`Result<T, TomlError>` zurück. Intern wird `?`-Operator verwendet.

---

## 12. Dateistruktur des Crates

```
toml/rust/
├── konzept.md                 ← dieses Dokument
├── Cargo.toml
└── src/
    ├── lib.rs                 ← Re-Exports; Dokumentation auf Crate-Ebene
    ├── error.rs               ← TomlError, TomlErrorKind, SourceLocation
    ├── datetime.rs            ← LocalDate, LocalTime, LocalDateTime,
    │                             OffsetDateTime
    ├── value.rs               ← Value-Enum, Array, Table
    ├── document.rs            ← Document, SerializeOptions
    ├── lexer.rs               ← Token, TokenKind, Lexer
    ├── parser.rs              ← Parser, ParseContext, TableStatus
    ├── serializer.rs          ← Serializer
    └── tests/
        ├── test_datetime.rs
        ├── test_integer.rs
        ├── test_string.rs
        ├── test_float.rs
        ├── test_table.rs
        ├── test_serializer.rs
        └── test_roundtrip.rs
```

---

## 13. Cargo.toml

```toml
[package]
name    = "toml-rs"
version = "0.1.0"
edition = "2021"
rust-version = "1.70"
description = "TOML 1.1 library – read, modify, write"
license = "MIT"

[dependencies]
indexmap = "2"
chrono   = { version = "0.4", default-features = false }

[dev-dependencies]
# keine externen Test-Frameworks; Rust's eingebautes `#[test]` genügt
```

---

## 14. Minimales Verwendungsbeispiel

```rust
use toml_rs::{Document, Value, SerializeOptions};

fn main() -> Result<(), toml_rs::TomlError> {
    // ── Lesen ─────────────────────────────────────────────────────────────
    let mut doc = Document::parse_file("config.toml")?;

    // Einzelnen Wert lesen
    let host = doc.get::<String>("server.host")?;
    let port = doc.get::<i64>("server.port")?;
    println!("{}:{}", host, port);

    // ── Ändern ────────────────────────────────────────────────────────────
    doc.root_mut().insert("server.port", Value::Integer(9090));
    doc.root_mut().insert("debug", Value::Boolean(true));

    // Array erweitern
    if let Some(Value::Array(tags)) = doc.path_mut("tags") {
        tags.push(Value::String("new-tag".to_owned()));
    }

    // ── Schreiben ─────────────────────────────────────────────────────────
    doc.write_file("config.toml")?;

    // Oder als String ausgeben
    let opts = SerializeOptions {
        sort_keys: true,
        ..Default::default()
    };
    println!("{}", doc.serialize_with(&opts));

    Ok(())
}
```

---

## 15. Besonderheiten aus TOML 1.1 (gegenüber 1.0)

| Neuerung | Auswirkung auf die Implementierung |
|----------|-----------------------------------|
| `\e` Escape (U+001B) | Im Lexer als weiterer Escape-Code registrieren |
| `\xHH` Escape (≤ U+00FF) | Im Lexer 2-Hex-Digit-Escape parsen und in UTF-8 umwandeln |
| Inline-Tables mit Zeilenumbrüchen | Lexer darf `\n`/`\r\n` innerhalb `{…}` nicht als Statement-Ende behandeln |
| Trailing Comma in Inline-Tables | Parser akzeptiert optionales `,` nach dem letzten Wert |
| Sekunden in Datetime/Time optional | Datetime-Parser akzeptiert `HH:MM` ohne `:SS`; Default 0 |

---

## 16. Qualitätssicherung

- **Round-trip-Invariante:** `Document::parse(&doc.serialize()).unwrap()` muss
  dasselbe `Document` ergeben wie das Original (`PartialEq` auf allen Typen).
- **Alle ABNF-Beispiele** aus `toml.md` (gültig wie ungültig) als
  `#[test]`-Fälle.
- **Fehlertests:** Jedes mit `# INVALID` markierte Beispiel aus `toml.md`
  muss `Err(TomlError { kind: ParseError | DuplicateKey, … })` zurückgeben.
- **Kein `unwrap()` / `expect()`** im Library-Code (nur in Tests erlaubt).
- **Clippy:** `#![deny(clippy::all, clippy::pedantic)]` im `lib.rs`.
- **Fuzzing:** `cargo-fuzz` Harness auf `Document::parse` ansetzen, um
  Panics bei beliebiger Eingabe auszuschließen.

---

## 17. Abgrenzung zur C++-Referenzimplementierung

| Aspekt | C++-Library | Rust-Library |
|--------|-------------|--------------|
| Fehlerbehandlung | Exceptions (`TomlError`) | `Result<T, TomlError>` |
| Typpolymorphismus | `std::variant` + Templates | `enum Value` + Traits |
| Speicher für Value | `shared_ptr` für Table/Array | Direkt im Enum (Box wird automatisch genutzt) |
| Null-Wert | nicht vorhanden | nicht vorhanden |
| Header-Only | ja (`.inl.hpp`) | nicht anwendbar (Crate-Modell) |
| Geordnete Map | `vector` + `unordered_map` | `IndexMap` (Feature) |
| Externe Deps | keine | `indexmap`, `chrono` |
| Speichersicherheit | RAII, manuell überprüft | Compile-Zeit garantiert |
| Parallelzugriff | nicht spezifiziert | `Send + Sync` für alle Typen |
