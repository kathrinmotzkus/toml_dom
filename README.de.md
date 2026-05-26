# toml_dom

Eine vollständige TOML-1.1-Library in Rust zum Lesen, Bearbeiten und Schreiben von TOML-Dokumenten.

---

## Warum toml_dom?

| | `toml` (Cargo-Team) | `toml_edit` | **`toml_dom`** |
|---|---|---|---|
| TOML-Version | 1.0 | 1.0 | **1.1** |
| Daten lesen | ✓ (via serde) | ✓ | ✓ |
| Daten ändern | ✗ | ✓ (format-erhaltend) | ✓ |
| Mutabler DOM ohne serde | ✗ | eingeschränkt | **✓** |
| Gezielter Pfadzugriff | ✗ | ✗ | **✓** |
| Keine serde-Abhängigkeit | ✗ | ✗ | **✓** |

`toml_dom` richtet sich an Anwendungen, die ein TOML-Dokument **programmatisch lesen, verändern und zurückschreiben** müssen — ohne serde-Derive-Makros und mit voller TOML-1.1-Unterstützung.

---

## Inhaltsverzeichnis

- [Was die Library leistet](#was-die-library-leistet)
- [Wie sie es macht](#wie-sie-es-macht)
- [Einbindung in eigene Projekte](#einbindung-in-eigene-projekte)
- [Die öffentliche API](#die-öffentliche-api)
  - [Document](#document)
  - [Table](#table)
  - [Array](#array)
  - [Value](#value)
  - [Datums- und Zeittypen](#datums--und-zeittypen)
  - [Fehlerbehandlung](#fehlerbehandlung)
  - [Serialisierungsoptionen](#serialisierungsoptionen)
- [Ausführliche Beispiele](#ausführliche-beispiele)
- [TOML-1.1-Besonderheiten](#toml-11-besonderheiten)
- [Projektstruktur](#projektstruktur)

---

## Was die Library leistet

`toml_dom` implementiert die [TOML-Spezifikation Version 1.1](https://toml.io/en/v1.1.0) vollständig. Sie ermöglicht es, in anderen Rust-Programmen:

- TOML-Dokumente aus **Strings, Dateien oder beliebigen `Read`-Quellen** einzulesen,
- das Dokument im Speicher als **veränderliches Datenmodell** zu halten,
- einzelne Werte zu **lesen, ändern, hinzufügen und löschen**,
- das Datenmodell wieder als **validen TOML-Text** zu serialisieren,
- **präzise Fehlermeldungen** mit Zeile und Spalte zu erhalten.

Alle zehn TOML-Typen werden unterstützt: `string`, `integer`, `float`, `boolean`, `offset-date-time`, `local-date-time`, `local-date`, `local-time`, `array`, `table`.

---

## Wie sie es macht

Die Library ist in drei Schichten aufgebaut:

```
┌──────────────────────────────────────────────────────┐
│               Öffentliche API                        │
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

Der Parser ist ein **rekursiver Abstieg** direkt auf dem UTF-8-Eingabestring. Eine interne `Source`-Struktur verwaltet die aktuelle Byte-Position sowie Zeile und Spalte für Fehlermeldungen. Es gibt keine separate Lexer-Schicht; Tokenisierung und semantische Analyse laufen zusammen.

Für jede ABNF-Regel der TOML-Grammatik gibt es eine eigene Methode:
`parse_document` → `parse_keyval` → `parse_key` / `parse_value` → `parse_string_basic` / `parse_number_or_datetime` / `parse_array` / `parse_inline_table` usw.

Eine `ParseContext`-Struktur verfolgt den Zustand jedes Tabellenpfades (`ExplicitlyDefined`, `ImplicitlyCreated`, `Inline`, `ArrayElement`) und erkennt damit alle von der Spezifikation verbotenen Konstrukte — doppelte Schlüssel, erneute Tabellendefinition, nachträgliche Erweiterung von Inline-Tables.

### Datenmodell (`src/value.rs`)

Das Herzstück ist das `Value`-Enum:

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

`Table` ist eine geordnete Schlüssel-Wert-Zuordnung auf Basis von `IndexMap<String, Value>` — die Einfügereihenfolge bleibt erhalten, Suche läuft in O(1).

`Array` ist ein Newtype über `Vec<Value>` und kann gemischte Typen aufnehmen.

### Serializer (`src/serializer.rs`)

Der Serializer traversiert das Datenmodell und erzeugt TOML-Text nach folgenden Regeln:

- Skalare Werte werden direkt ausgegeben.
- Floats erhalten immer `.` oder `e` (z. B. `1.0`, nie `1`).
- Strings werden als Basic-Strings `"…"` ausgegeben, Sonderzeichen werden escapt.
- Schlüssel mit Zeichen außerhalb von `A-Za-z0-9_-` werden in Anführungszeichen gesetzt.
- Verschachtelte Tables erscheinen als `[pfad]`-Header.
- Arrays of Tables erscheinen als `[[pfad]]`-Header.
- Sehr flache Tables (≤ 4 Einträge, keine Sub-Tables) können optional als Inline-Tables `{…}` ausgegeben werden.

---

## Einbindung in eigene Projekte

### 1. Abhängigkeit in `Cargo.toml`

Da `toml_dom` noch nicht auf [crates.io](https://crates.io) veröffentlicht ist, binden Sie es über einen lokalen Pfad ein. Passen Sie den Pfad an Ihre Verzeichnisstruktur an:

```toml
[dependencies]
toml_dom = { path = "../toml/rust" }
```

### 2. In der Quelldatei importieren

```rust
use toml_dom::{Document, Value, TomlError};
use toml_dom::{Table, Array};
use toml_dom::{LocalDate, LocalTime, LocalDateTime, OffsetDateTime};
use toml_dom::SerializeOptions;
```

Oder alles auf einmal mit einem Glob-Import (nur für Skripte/Prototypen empfohlen):

```rust
use toml_dom::*;
```

### 3. Minimales Beispiel

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

## Die öffentliche API

### Document

`Document` ist der zentrale Einstiegspunkt. Es hält das gesamte TOML-Dokument als Root-`Table`.

#### Einlesen

```rust
// Aus einem &str
let doc = Document::parse("key = \"wert\"\n")?;

// Aus einer Datei
let doc = Document::parse_file("config.toml")?;

// Aus einem beliebigen Reader (z. B. BufReader, Cursor, stdin)
use std::io::Cursor;
let reader = Cursor::new(b"x = 42\n");
let doc = Document::parse_reader(reader)?;
```

#### Programmatisch aufbauen

`Document::from_table` erstellt ein Dokument aus einer fertig befüllten `Table`, ohne Parsing:

```rust
use toml_dom::{Document, Table, Value};

let mut root = Table::new();
root.insert("name", Value::String("Mein Projekt".into()));
root.insert("version", Value::Integer(1));

let doc = Document::from_table(root);
println!("{}", doc.serialize());
```

#### Lesen — typisierter Zugriff

`doc.get::<T>(key)` liest einen Wert direkt aus dem Root-Table und gibt `&T` zurück. Schlägt der Schlüssel nicht vor oder stimmt der Typ nicht, wird `Err(TomlError)` zurückgegeben.

```rust
let host: &String = doc.get::<String>("host")?;
let port: &i64    = doc.get::<i64>("port")?;
let debug: &bool  = doc.get::<bool>("debug")?;
```

Unterstützte Typparameter: `String`, `i64`, `f64`, `bool`, `LocalDate`, `LocalTime`, `LocalDateTime`, `OffsetDateTime`, `Array`, `Table`.

#### Lesen — Pfadzugriff

`doc.path("a.b.c")` navigiert durch beliebig tief verschachtelte Tables anhand einer Punkt-separierten Zeichenkette. Gibt `Option<&Value>` zurück.

```rust
// [server]
// host = "example.com"
if let Some(val) = doc.path("server.host") {
    println!("{:?}", val);  // Value::String("example.com")
}
```

> **Hinweis:** `path` teilt den String an jedem `.`. Schlüssel, die selbst einen Punkt enthalten (z. B. `"google.com"` als quoted key), müssen mit `root().get_path_segments(&["site", "google.com"])` abgerufen werden.

#### Ändern

```rust
// Wert überschreiben oder neu einfügen
doc.root_mut().insert("debug", Value::Boolean(true));

// Wert über Pfad einfügen (erzeugt fehlende Zwischen-Tables)
doc.root_mut().insert_path("server.port", Value::Integer(8080))?;

// Wert über Pfad ändern
if let Some(v) = doc.path_mut("server.port") {
    *v = Value::Integer(9090);
}

// Eintrag löschen
doc.root_mut().remove("debug");
```

#### Serialisieren und Schreiben

```rust
// Als String ausgeben (Standardoptionen)
let toml_text: String = doc.serialize();

// Als String mit eigenen Optionen
let opts = SerializeOptions {
    sort_keys: true,
    ..Default::default()
};
let toml_text = doc.serialize_with(&opts);

// In Datei schreiben
doc.write_file("output.toml")?;
doc.write_file_with("output.toml", &opts)?;
```

---

### Table

`Table` speichert Schlüssel-Wert-Paare in Einfügereihenfolge.

```rust
use toml_dom::{Table, Value};

let mut t = Table::new();
t.insert("host", Value::String("localhost".into()));
t.insert("port", Value::Integer(3000));
```

#### Methoden im Überblick

| Methode | Rückgabe | Beschreibung |
|---------|----------|-------------|
| `contains_key("key")` | `bool` | Prüft ob Schlüssel vorhanden |
| `get("key")` | `Option<&Value>` | Wert lesen |
| `get_mut("key")` | `Option<&mut Value>` | Wert ändern |
| `get_as::<T>("key")` | `Result<&T, TomlError>` | Typisierter Zugriff |
| `get_path("a.b.c")` | `Option<&Value>` | Pfadzugriff (Bare Keys) |
| `get_path_mut("a.b")` | `Option<&mut Value>` | Mutabler Pfadzugriff |
| `get_path_segments(&["a","b.c"])` | `Option<&Value>` | Pfadzugriff mit Punkt in Schlüsseln |
| `get_path_segments_mut(&[…])` | `Option<&mut Value>` | Mutabler Segmentpfad |
| `insert_path("a.b", val)` | `Result<…>` | Einfügen, erzeugt Zwischen-Tables |
| `insert_path_segments(&[…], val)` | `Result<…>` | Wie oben, Punkt in Schlüsseln |
| `insert("key", val)` | `Option<Value>` | Einfügen / Überschreiben |
| `remove("key")` | `Option<Value>` | Löschen |
| `keys()` | `Iterator<&str>` | Alle Schlüssel |
| `iter()` | `Iterator<(&str, &Value)>` | Alle Einträge |
| `iter_mut()` | `Iterator<(&str, &mut Value)>` | Alle Einträge (änderbar) |
| `len()` / `is_empty()` | `usize` / `bool` | Größe |
| `table["key"]` | `&Value` | Index-Operator (panikt wenn nicht vorhanden) |

---

### Array

`Array` ist eine geordnete Liste von `Value`-Elementen. TOML erlaubt gemischte Typen.

```rust
use toml_dom::{Array, Value};

let mut arr = Array::new();
arr.push(Value::Integer(1));
arr.push(Value::Integer(2));
arr.push(Value::String("drei".into()));

println!("{}", arr.len());      // 3
println!("{:?}", arr.get(0));   // Some(Value::Integer(1))

// Typisierter Zugriff auf homogene Arrays
let zahlen: Vec<i64> = arr.as_typed::<i64>()?;
```

#### Methoden im Überblick

| Methode | Rückgabe | Beschreibung |
|---------|----------|-------------|
| `get(i)` | `Option<&Value>` | Element lesen |
| `get_mut(i)` | `Option<&mut Value>` | Element ändern |
| `push(val)` | — | An das Ende anhängen |
| `insert(i, val)` | — | An Position einfügen |
| `remove(i)` | `Value` | Element entfernen |
| `as_typed::<T>()` | `Result<Vec<T>>` | Homogenes Array konvertieren |
| `iter()` / `iter_mut()` | Iterator | Iteration |
| `len()` / `is_empty()` | `usize` / `bool` | Größe |
| `arr[i]` | `&Value` | Index-Operator |
| `for v in &arr` | — | `IntoIterator`-Unterstützung |

---

### Value

`Value` ist das universelle TOML-Werttyp-Enum.

```rust
use toml_dom::Value;

let s = Value::String("Hallo".into());
let n = Value::Integer(42);
let f = Value::Float(3.14);
let b = Value::Boolean(true);
```

**Typname abfragen** (nützlich für Fehlermeldungen):

```rust
let name: &str = val.type_name();  // "string", "integer", "float", …
```

**Pattern Matching:**

```rust
match val {
    Value::String(s)  => println!("String: {}", s),
    Value::Integer(n) => println!("Zahl: {}", n),
    Value::Table(t)   => println!("{} Einträge", t.len()),
    Value::Array(a)   => println!("{} Elemente", a.len()),
    _                 => println!("Anderer Typ: {}", val.type_name()),
}
```

---

### Datums- und Zeittypen

Die Library definiert vier eigene Structs, da die Rust-Standardbibliothek keine zeitzonenfreien Datums-/Zeitwerte kennt.

```rust
use toml_dom::{LocalDate, LocalTime, LocalDateTime, OffsetDateTime};

let d: &LocalDate = doc.get::<LocalDate>("geburtstag")?;
println!("{}-{:02}-{:02}", d.year, d.month, d.day);

let t: &LocalTime = doc.get::<LocalTime>("uhrzeit")?;
println!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second);
// t.nanosecond: Nanosekundenanteil (mind. Millisekunden-Genauigkeit)

let odt: &OffsetDateTime = doc.get::<OffsetDateTime>("erstellt")?;
if odt.is_utc() { println!("UTC-Zeit"); }
```

**Display** im RFC-3339-Format:

```rust
println!("{}", LocalDate { year: 2024, month: 3, day: 15 });  // "2024-03-15"
println!("{}", LocalTime { hour: 14, minute: 30, second: 0, nanosecond: 0 });  // "14:30:00"
```

**Konvertierung zu/von `chrono`:**

```rust
let ld = LocalDate { year: 2024, month: 6, day: 1 };
let naive: chrono::NaiveDate = ld.into();
let zurueck: LocalDate = naive.into();
```

| toml_dom-Typ | chrono-Typ |
|---|---|
| `LocalDate` | `chrono::NaiveDate` |
| `LocalTime` | `chrono::NaiveTime` |
| `LocalDateTime` | `chrono::NaiveDateTime` |
| `OffsetDateTime` | `chrono::DateTime<chrono::FixedOffset>` |

---

### Fehlerbehandlung

Alle fehlbaren Operationen geben `Result<T, TomlError>` zurück.

```rust
use toml_dom::{TomlError, TomlErrorKind};

match Document::parse("ungültig = \n") {
    Ok(doc) => { /* … */ }
    Err(e) => {
        eprintln!("{}", e);  // "2:1: unexpected newline"

        match &e.kind {
            TomlErrorKind::ParseError              => eprintln!("Syntaxfehler"),
            TomlErrorKind::DuplicateKey            => eprintln!("Doppelter Schlüssel"),
            TomlErrorKind::TypeError { expected, found } =>
                eprintln!("Falscher Typ: erwartet {expected}, gefunden {found}"),
            TomlErrorKind::KeyNotFound(key)        => eprintln!("Schlüssel fehlt: {key}"),
            TomlErrorKind::IntegerOverflow         => eprintln!("Integer-Überlauf"),
            TomlErrorKind::InvalidEscape(s)        => eprintln!("Ungültiger Escape: {s}"),
            TomlErrorKind::Io(msg)                 => eprintln!("I/O-Fehler: {msg}"),
            _                                      => eprintln!("Sonstiger Fehler"),
        }

        if let Some(loc) = &e.location {
            eprintln!("Zeile {}, Spalte {}", loc.line, loc.column);
        }
    }
}
```

`TomlError` implementiert `std::error::Error` und ist mit `anyhow`, `thiserror` usw. kombinierbar:

```rust
use anyhow::Context;
let doc = Document::parse_file("config.toml")
    .context("Konfigurationsdatei konnte nicht gelesen werden")?;
```

---

### Serialisierungsoptionen

```rust
use toml_dom::SerializeOptions;

let opts = SerializeOptions {
    sort_keys:        false,        // Schlüssel in Einfügereihenfolge
    prefer_inline:    false,        // Tables als [header], nicht {…}
    indent:           "    ".into(),// vier Leerzeichen
    trailing_newline: true,         // Datei mit \n abschließen
};
```

| Option | Standard | Beschreibung |
|--------|----------|-------------|
| `sort_keys` | `false` | Schlüssel alphabetisch sortieren |
| `prefer_inline` | `false` | Kleine Tables als Inline-Tables `{…}` |
| `indent` | `"    "` | Einrückung für verschachtelte Tables |
| `trailing_newline` | `true` | Abschließendes `\n` |

---

## Ausführliche Beispiele

### Konfigurationsdatei lesen

```rust
use toml_dom::{Document, Value};

fn main() -> Result<(), toml_dom::TomlError> {
    let doc = Document::parse_file("config.toml")?;

    let titel   = doc.get::<String>("titel")?;
    let version = doc.get::<i64>("version")?;

    let host = doc.path("datenbank.host")
        .and_then(|v| if let Value::String(s) = v { Some(s.as_str()) } else { None })
        .unwrap_or("localhost");

    println!("{} v{} — DB: {}", titel, version, host);
    Ok(())
}
```

### Dokument programmatisch aufbauen

```rust
use toml_dom::{Document, Table, Array, Value};

fn main() -> Result<(), toml_dom::TomlError> {
    let mut root = Table::new();
    root.insert("name",    Value::String("Meine App".into()));
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

### Dokument lesen, ändern und zurückschreiben

```rust
use toml_dom::{Document, Value, SerializeOptions};

fn main() -> Result<(), toml_dom::TomlError> {
    let mut doc = Document::parse_file("config.toml")?;

    if let Some(Value::Integer(p)) = doc.path_mut("server.port") {
        *p += 1;
    }

    doc.root_mut().insert("geaendert_am", Value::String("2024-06-01".into()));

    let opts = SerializeOptions { sort_keys: true, ..Default::default() };
    doc.write_file_with("config.toml", &opts)?;
    Ok(())
}
```

### Array of Tables auslesen

```toml
# produkte.toml
[[produkt]]
name = "Hammer"
preis = 9.99

[[produkt]]
name = "Säge"
preis = 24.50
```

```rust
use toml_dom::{Document, Value, Array};

fn main() -> Result<(), toml_dom::TomlError> {
    let doc = Document::parse_file("produkte.toml")?;

    let arr = doc.get::<Array>("produkt")?;
    for item in arr {
        if let Value::Table(t) = item {
            let name  = t.get_as::<String>("name")?;
            let preis = t.get_as::<f64>("preis")?;
            println!("{}: {:.2} €", name, preis);
        }
    }
    Ok(())
}
```

### Schlüssel mit Punkt im Namen

```toml
# spezial.toml
[site]
"google.com" = true
```

```rust
use toml_dom::Document;

fn main() -> Result<(), toml_dom::TomlError> {
    let doc = Document::parse_file("spezial.toml")?;

    // get_path würde hier falsch auf "google" → "com" splitten.
    // Stattdessen get_path_segments verwenden:
    let val = doc.root().get_path_segments(&["site", "google.com"]);
    println!("{:?}", val);  // Some(Value::Boolean(true))
    Ok(())
}
```

---

## TOML-1.1-Besonderheiten

| Neuerung | Beispiel |
|----------|---------|
| `\e` Escape (U+001B, ESC) | `s = "\e[31m"` |
| `\xHH` Escape (bis U+00FF) | `s = "\x41"` → `"A"` |
| Zeilenumbrüche in Inline-Tables | `t = {\n  a = 1,\n  b = 2\n}` |
| Trailing Comma in Inline-Tables | `t = {a = 1, b = 2,}` |
| Sekunden in Zeit-Literalen optional | `t = 14:30` (entspricht `14:30:00`) |
| Sekunden in Datetime-Literalen optional | `dt = 2024-03-15T14:30Z` |

---

## Projektstruktur

```
rust/
├── Cargo.toml
├── README.md
├── fuzz/
│   ├── Cargo.toml
│   └── fuzz_targets/
│       └── fuzz_parse.rs   — Fuzzing-Einstiegspunkt für Document::parse
└── src/
    ├── lib.rs              — Crate-Root, Re-Exports
    ├── error.rs            — TomlError, TomlErrorKind, SourceLocation
    ├── datetime.rs         — LocalDate, LocalTime, LocalDateTime, OffsetDateTime
    ├── value.rs            — Value, Array, Table, FromValue-Trait
    ├── parser.rs           — Rekursiver Abstieg, ParseContext
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

Tests ausführen:

```sh
cargo test
```

Fuzzing (erfordert Nightly + cargo-fuzz):

```sh
cargo +nightly fuzz run fuzz_parse
```
