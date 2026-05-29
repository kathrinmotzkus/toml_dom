# toml_dom

Eine vollständige TOML-1.1-Library in Rust zum Lesen, Bearbeiten und Schreiben von TOML-Dokumenten — mit **format-erhaltendem Roundtrip**.

---

## Warum toml_dom?

| | `toml` (Cargo-Team) | `toml_edit` | **`toml_dom`** |
|---|---|---|---|
| TOML-Version | 1.0 | 1.0 | **1.1** |
| Daten lesen | ✓ (via serde) | ✓ | ✓ |
| Daten ändern | ✗ | ✓ (format-erhaltend) | **✓ (format-erhaltend)** |
| Format-erhaltender Roundtrip | ✗ | ✓ | **✓** |
| Mutabler DOM ohne serde | ✗ | eingeschränkt | **✓** |
| Gezielter Pfadzugriff | ✗ | ✗ | **✓** |
| Keine serde-Abhängigkeit | ✗ | ✗ | **✓** |

`toml_dom` richtet sich an Anwendungen, die ein TOML-Dokument **programmatisch lesen, verändern und zurückschreiben** müssen — ohne serde-Derive-Makros, mit voller TOML-1.1-Unterstützung, und dabei Kommentare, Formatierungen und Schreibweisen aller unveränderten Einträge **exakt erhalten**.

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
- [Changelog](#changelog)

---

## Was die Library leistet

`toml_dom` implementiert die [TOML-Spezifikation Version 1.1](https://toml.io/en/v1.1.0) vollständig. Sie ermöglicht es, in anderen Rust-Programmen:

- TOML-Dokumente aus **Strings, Dateien oder beliebigen `Read`-Quellen** einzulesen,
- das Dokument im Speicher als **veränderliches Datenmodell** zu halten,
- einzelne Werte zu **lesen, ändern, hinzufügen und löschen**,
- das Datenmodell wieder als **validen TOML-Text** zu serialisieren,
- dabei **Kommentare, Leerzeilen, String-Schreibweisen, Zahlendarstellungen** und sonstige Formatierungen aller nicht veränderten Einträge **exakt zu erhalten**,
- **präzise Fehlermeldungen** mit Zeile und Spalte zu erhalten.

Alle zehn TOML-Typen werden unterstützt: `string`, `integer`, `float`, `boolean`, `offset-date-time`, `local-date-time`, `local-date`, `local-time`, `array`, `table`.

### Format-erhaltender Roundtrip

Was beim Einlesen einer TOML-Datei erhalten bleibt:

| Merkmal | Beispiel |
|---------|---------|
| Kommentare | `# Datenbankeinstellungen` |
| Inline-Kommentare | `port = 8080  # Standard-Port` |
| Leerzeilen zwischen Einträgen | strukturgebende Abstände |
| String-Schreibweise | `'literal'`, `"""mehrzeilig"""` |
| Zahlenformat | `0xFF`, `0o755`, `0b1010`, `1_000_000` |
| Inline- vs. Block-Tables | `{ a = 1 }` vs. `[section]` |
| Mehrzeilige Arrays | Einrückung und Zeilenumbrüche |
| Trailing commas | `[1, 2, 3,]` (TOML 1.1) |
| Whitespace um `=` | `key  =  "wert"` |

---

## Wie sie es macht

Die Library ist in vier Schichten aufgebaut:

```
┌──────────────────────────────────────────────────────┐
│               Öffentliche API                        │
│    Document · Table · Array · Value                  │
└──────────────┬────────────────────────┬──────────────┘
               │                        │
       ┌───────▼──────┐        ┌────────▼───────┐
       │    Parser    │        │   Serializer   │
       │              │        │                │
       │  DOM-Baum    │        │  ① items-Pfad  │
       │  + items     │        │  ② DOM-Pfad    │
       └──────┬───────┘        └────────────────┘
              │
       ┌──────▼───────┐
       │     CST      │
       │ Vec<Document │
       │    Item>     │
       └──────────────┘
```

### Parser (`src/parser.rs`)

Der Parser ist ein **rekursiver Abstieg** direkt auf dem UTF-8-Eingabestring. Eine interne `Source`-Struktur verwaltet die aktuelle Byte-Position sowie Zeile und Spalte für Fehlermeldungen.

Neu in v0.2: Der Parser zeichnet für jedes Token den **vollständigen Originaltext** auf — Kommentarzeilen vor einem Schlüssel, den Schlüssel exakt wie geschrieben, den Whitespace um das `=`-Zeichen, den Wert als Rohtext (also z. B. `0xFF` statt `255`), den Inline-Kommentar danach und das Zeilenende. Diese Metadaten werden in einer flachen `Vec<DocumentItem>` gespeichert.

### CST-Schicht (`src/cst.rs`)

`DocumentItem` ist das zentrale Enum der CST-Schicht:

```rust
pub enum DocumentItem {
    Entry {
        node: EntryNode,    // Formatierungsmetadaten + Rohtexte
        path: Vec<String>,  // DOM-Pfad für Lookup
    },
    Section(SectionNode),   // [header] oder [[array-of-tables]]
    Eof(String),            // Whitespace/Kommentare am Dateiende
}
```

`EntryNode` speichert alle Formatierungsinformationen eines Eintrags:

```rust
pub struct EntryNode {
    pub leading:   String,         // Kommentare/Leerzeilen vor dem Schlüssel
    pub raw_key:   String,         // Schlüssel im Originaltext
    pub pre_eq:    String,         // Whitespace vor "="
    pub post_eq:   String,         // Whitespace nach "="
    pub raw_value: Option<String>, // Wert im Originaltext; None = neu generieren
    pub trailing:  String,         // Inline-Kommentar + Zeilenende
}
```

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

Der Serializer hat zwei Pfade:

**Format-erhaltender Pfad** (geparstes Dokument, kein `sort_keys`/`prefer_inline`):  
Läuft die `Vec<DocumentItem>` in Quellreihenfolge durch und gibt für jeden Eintrag den gespeicherten Originaltext aus. Nur Einträge, deren `raw_value` durch `Document::set_value` geleert wurde, werden neu generiert.

**Kanonischer DOM-Pfad** (programmatisch erstelltes Dokument, oder `sort_keys`/`prefer_inline` gesetzt):  
Traversiert den DOM-Baum und erzeugt TOML-Text nach folgenden Regeln:
- Floats erhalten immer `.` oder `e` (z. B. `1.0`, nie `1`).
- Strings werden als Basic-Strings `"…"` ausgegeben, Sonderzeichen werden escapt.
- Schlüssel mit Zeichen außerhalb von `A-Za-z0-9_-` werden in Anführungszeichen gesetzt.
- Verschachtelte Tables erscheinen als `[pfad]`-Header.
- Arrays of Tables erscheinen als `[[pfad]]`-Header.
- Sehr flache Tables (≤ 4 Einträge, keine Sub-Tables) werden als Inline-Tables `{…}` ausgegeben.

---

## Einbindung in eigene Projekte

### 1. Abhängigkeit in `Cargo.toml`

```toml
[dependencies]
toml_dom = "0.2"
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

`Document` ist der zentrale Einstiegspunkt. Es hält das gesamte TOML-Dokument als Root-`Table` sowie — bei geparsten Dokumenten — die `Vec<DocumentItem>` für den format-erhaltenden Roundtrip.

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

`Document::from_table` erstellt ein Dokument aus einer fertig befüllten `Table`, ohne Parsing. Solche Dokumente enthalten keine Formatierungsmetadaten und serialisieren immer kanonisch.

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

#### Ändern — format-erhaltend (`set_value`)

`Document::set_value` ist der bevorzugte Mutationspfad bei geparsten Dokumenten. Er aktualisiert sowohl den DOM-Baum als auch die Formatierungsliste: der `raw_value` des betreffenden Eintrags wird geleert, sodass der Serializer genau diesen einen Wert neu generiert — alle anderen Einträge bleiben unberührt.

```rust
// config.toml enthält: port = 8080  # Standard-Port
let mut doc = Document::parse_file("config.toml")?;

// Ändert den Wert, bewahrt Kommentar und alle anderen Formatierungen
let ok = doc.set_value(&["port"], Value::Integer(9090));
assert!(ok);  // false, wenn der Pfad nicht existiert

doc.write_file("config.toml")?;
// Ergebnis: port = 9090  # Standard-Port
```

Für verschachtelte Pfade:

```rust
doc.set_value(&["server", "port"], Value::Integer(443));
```

#### Ändern — direkt über DOM

Über `root_mut()` kann der DOM-Baum direkt verändert werden. Diese Änderungen werden beim Serialisieren zuverlässig ausgegeben; Formatierungsmetadaten für geänderte Einträge werden dabei nicht automatisch aktualisiert.

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
// Als String ausgeben (Standardoptionen, format-erhaltend bei geparsten Dokumenten)
let toml_text: String = doc.serialize();

// Als String mit eigenen Optionen
// Hinweis: sort_keys/prefer_inline erzwingen den kanonischen DOM-Pfad
let opts = SerializeOptions {
    sort_keys: true,
    ..Default::default()
};
let toml_text = doc.serialize_with(&opts);

// In Datei schreiben
doc.write_file("output.toml")?;
doc.write_file_with("output.toml", &opts)?;
```

#### CST-Zugriff

```rust
// Rohe Item-Liste lesen (für fortgeschrittene Anwendungsfälle)
for item in doc.items() {
    match item {
        toml_dom::DocumentItem::Entry { node, path } => {
            println!("Schlüssel: {} → Rohwert: {:?}", node.raw_key, node.raw_value);
        }
        toml_dom::DocumentItem::Section(s) => {
            println!("Abschnitt: {}", s.raw);
        }
        toml_dom::DocumentItem::Eof(_) => {}
    }
}
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
    indent:           "    ".into(),// vier Leerzeichen (nur kanonischer Pfad)
    trailing_newline: true,         // Datei mit \n abschließen (nur kanonischer Pfad)
};
```

| Option | Standard | Beschreibung |
|--------|----------|-------------|
| `sort_keys` | `false` | Schlüssel alphabetisch sortieren (erzwingt kanonischen Pfad) |
| `prefer_inline` | `false` | Kleine Tables als Inline-Tables `{…}` (erzwingt kanonischen Pfad) |
| `indent` | `"    "` | Einrückung für verschachtelte Tables (nur kanonischer Pfad) |
| `trailing_newline` | `true` | Abschließendes `\n` (nur kanonischer Pfad) |

> **Hinweis:** `sort_keys: true` und `prefer_inline: true` erzwingen den kanonischen DOM-Pfad — auch bei geparsten Dokumenten. Format und Kommentare gehen dabei verloren. Für format-erhaltendes Schreiben einfach `doc.serialize()` ohne Optionen verwenden.

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

### Format-erhaltend ändern und zurückschreiben

```rust
use toml_dom::{Document, Value};

fn main() -> Result<(), toml_dom::TomlError> {
    // Ausgangsdatei config.toml:
    //
    //   # Servereinstellungen
    //   [server]
    //   host = 'localhost'   # IPv4-Adresse
    //   port = 0x1F90        # Hex: 8080
    //
    let mut doc = Document::parse_file("config.toml")?;

    // Nur den Port ändern — alles andere bleibt byte-identisch
    doc.set_value(&["server", "port"], Value::Integer(9090));

    doc.write_file("config.toml")?;
    // Ergebnis:
    //
    //   # Servereinstellungen
    //   [server]
    //   host = 'localhost'   # IPv4-Adresse
    //   port = 9090          # Hex: 8080
    Ok(())
}
```

### Dokument lesen, ändern und zurückschreiben (DOM-Pfad)

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
│       └── fuzz_parse.rs        — Fuzzing-Einstiegspunkt für Document::parse
└── src/
    ├── lib.rs                   — Crate-Root, Re-Exports
    ├── error.rs                 — TomlError, TomlErrorKind, SourceLocation
    ├── datetime.rs              — LocalDate, LocalTime, LocalDateTime, OffsetDateTime
    ├── value.rs                 — Value, Array, Table, FromValue-Trait
    ├── cst.rs                   — DocumentItem, EntryNode, SectionNode (CST-Schicht)
    ├── parser.rs                — Rekursiver Abstieg, ParseContext, Rohtexterfassung
    ├── serializer.rs            — Serializer (format-erhaltend + kanonisch), SerializeOptions
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
        └── test_format_preserve.rs — Tests für format-erhaltenden Roundtrip
```

Tests ausführen:

```sh
cargo test
```

Fuzzing (erfordert Nightly + cargo-fuzz):

```sh
cargo +nightly fuzz run fuzz_parse
```

---

## Changelog

### v0.2.0

**Hauptfeature: Format-erhaltender Roundtrip**

Der Parser zeichnet jetzt für jeden Token den vollständigen Originaltext auf. Beim Serialisieren werden diese Rohtexte direkt ausgegeben, sodass Kommentare, Formatierungen und Schreibweisen aller unveränderten Einträge exakt erhalten bleiben.

**Neue öffentliche Typen** (in `src/cst.rs`, re-exportiert aus dem Crate-Root):

- `DocumentItem` — flache Liste aller Quellelemente (Einträge, Abschnittsheader, Dateiende)
- `EntryNode` — Formatierungsmetadaten eines Schlüssel-Wert-Paares: `leading`, `raw_key`, `pre_eq`, `post_eq`, `raw_value`, `trailing`
- `SectionNode` — Abschnittsheader `[…]` oder `[[…]]` mit `leading`, `raw`, `trailing`, `path`, `is_array`

**Neue Methoden auf `Document`:**

- `Document::set_value(&["pfad", "zum", "key"], value)` — ändert einen Wert format-erhaltend: nur dieser Wert wird neu generiert, alle anderen Formatierungen bleiben unberührt
- `Document::items() -> &[DocumentItem]` — Lesezugriff auf die CST-Itemliste

**Serializer-Verhalten:**

- Geparstes Dokument ohne `sort_keys`/`prefer_inline` → format-erhaltender Pfad (neu)
- `sort_keys: true` oder `prefer_inline: true` → kanonischer DOM-Pfad (wie v0.1)
- Programmatisch erstelltes Dokument → kanonischer DOM-Pfad (wie v0.1)
- `trailing_newline` wirkt jetzt nur noch auf den kanonischen Pfad; der format-erhaltende Pfad reproduziert das originale Zeilenende

**Bugfix:**

- Absturz in `parse_time_str` bei unvollständigem Sekundenfeld (`13:63:` ohne folgende Ziffern) — durch Fuzzing gefunden, in v0.1.1 auf crates.io behoben, in v0.2.0 enthalten

**Nicht geändert:**

`Value`, `Table`, `Array`, `FromValue`, `TomlError`, `TomlErrorKind`, `SourceLocation` — vollständig rückwärtskompatibel.

---

### v0.1.0

Erstveröffentlichung:

- Vollständige TOML-1.1-Implementierung (Parser + Serializer)
- Alle zehn TOML-Typen
- `IndexMap`-basierte `Table` (Einfügereihenfolge erhalten)
- Chrono-Konvertierungen für alle vier Datetime-Typen
- Pfadzugriff (`get_path`, `get_path_segments`) mit Unterstützung für Punkte in Schlüsselnamen
- `SerializeOptions` (Schlüsselsortierung, Inline-Präferenz, Einrückung)
- Präzise Fehlermeldungen mit Zeile und Spalte
- cargo-fuzz-Integration
