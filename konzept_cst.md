# Konzept: toml_dom CST-Update (v0.2)

## Ziel

`toml_dom` soll zum **format-erhaltenden Roundtrip** fähig werden: wer ein
TOML-Dokument einliest, einen Wert ändert und wieder serialisiert, erhält
exakt den ursprünglichen Text zurück – bis auf die gezielt geänderten Stellen.

Das schließt ein:

| Was heute verloren geht | Beispiel |
|---|---|
| Kommentare | `# Datenbankeinstellungen` |
| String-Schreibweise | `'literal'`, `"""mehrzeilig"""` |
| Zahlenformat | `0xFF`, `0o77`, `0b1010`, `1_000_000` |
| Inline- vs. Block-Tabellen | `{ a = 1 }` vs. `[section]` |
| Mehrzeilige Arrays | Array über mehrere Zeilen mit Einrückung |
| Leerzeilen zwischen Einträgen | Strukturgebende Abstände |
| Einrückung | Originale Whitespace-Zeichen |
| Trailing commas | `[1, 2, 3,]` (TOML 1.1) |
| Inline-Kommentare | `port = 8080  # Standard-Port` |

---

## Kernproblem: zwei widersprüchliche Anforderungen

Der aktuelle DOM-Ansatz speichert Werte in einem
`IndexMap<String, Value>` – das ermöglicht O(1)-Zugriff per Schlüssel,
löscht aber alle Formatierungsinformationen.

Ein reiner CST-Ansatz (nur Quelltext-Knoten) erhält alles, macht aber
schnellen Wert-Zugriff aufwendig.

**Lösung: Duale Darstellung** – beide Strukturen im `Document` gleichzeitig:

```
Document
├── items: Vec<DocumentItem>   ← Reihenfolge + Format (für Serialisierung)
└── root: Table                ← IndexMap-Baum (für schnellen Zugriff)
```

Beide Strukturen zeigen auf dieselben Werte; Änderungen müssen in
beide Richtungen synchronisiert werden.

---

## Neue Typen

### `Trivia` – Whitespace und Kommentare

```rust
/// Rohtext, der vor oder nach einem syntaktischen Element steht:
/// Leerzeilen, Kommentarzeilen, horizontaler Whitespace.
pub struct Trivia(pub String);
```

Beispiel: Alles zwischen dem Ende einer Zeile und dem Beginn des
nächsten Schlüssels (Leerzeilen + `# ...`-Zeilen) ist `leading_trivia`
des folgenden Eintrags.

---

### `KeyRepr` – originale Schreibweise eines Schlüssels

```rust
pub enum KeyRepr {
    Bare(String),             // foo
    BasicQuoted(String),      // "foo bar"
    LiteralQuoted(String),    // 'foo.bar'
    Dotted(Vec<KeyRepr>),     // a.b.c  (als gepunkteter Schlüssel)
}
```

Der logische Schlüsselname (für `IndexMap`) lässt sich aus `KeyRepr`
ableiten; `KeyRepr` selbst enthält den Originaltext.

---

### `Node` – Wert mit vollem Quelltextkontext

Für Skalare reicht es, den Originaltext neben dem geparsten Wert zu
speichern. Arrays und Inline-Tabellen sind rekursiv und bekommen eigene
Unterknotenstrukturen:

```rust
pub enum Node {
    Scalar {
        raw:   String,   // "0xFF", "'hello'", "true", "1979-05-27", …
        value: Value,    // geparster Wert (unveränderter Typ aus value.rs)
    },
    Array {
        open:     String,             // "["
        elements: Vec<ArrayElement>,
        close:    String,             // "]"
    },
    InlineTable {
        open:    String,              // "{"
        entries: Vec<InlineEntry>,
        close:   String,              // "}"
    },
}

pub struct ArrayElement {
    pub leading:        Trivia,
    pub node:           Node,
    pub trailing_comma: Option<String>,  // "," falls vorhanden
    pub trailing:       Trivia,
}

pub struct InlineEntry {
    pub leading:        Trivia,
    pub key:            KeyRepr,
    pub pre_eq:         String,
    pub post_eq:        String,
    pub node:           Node,
    pub trailing_comma: Option<String>,
    pub trailing:       Trivia,
}
```

---

### `EntryNode` – ein Key-Value-Paar mit allem Drumherum

```rust
pub struct EntryNode {
    pub leading:  Trivia,      // Leerzeilen + Kommentare vor dem Schlüssel
    pub key:      KeyRepr,     // originale Schlüsseldarstellung
    pub pre_eq:   String,      // Whitespace vor "="
    pub post_eq:  String,      // Whitespace nach "="
    pub node:     Node,        // Wert mit Originaltext
    pub trailing: Trivia,      // Inline-Kommentar (falls vorhanden) + Zeilenende
}
```

---

### `SectionHeader` – Tabellenüberschrift `[…]` oder `[[…]]`

```rust
pub struct SectionHeader {
    pub leading:    Trivia,
    pub raw:        String,    // vollständiger Originaltext inkl. Klammern
                               // z.B. "[server.config]" oder "[[products]]"
    pub trailing:   Trivia,    // Inline-Kommentar auf der Header-Zeile
}
```

---

### `DocumentItem` – alle Elemente in Quellreihenfolge

```rust
pub enum DocumentItem {
    Entry(EntryNode),
    Section(SectionHeader),
    TrailingTrivia(Trivia),    // Leerzeilen/Kommentare am Dateiende
}
```

---

### `Document` – erweiterte Hauptstruktur

```rust
pub struct Document {
    items: Vec<DocumentItem>,  // format-erhaltendes Abbild der Quelldatei
    root:  Table,              // unveränderter IndexMap-Baum für Zugriff
}
```

---

## Was sich an der öffentlichen API ändert

### Unverändert (vollständige Rückwärtskompatibilität)

```rust
Document::parse(s)
Document::serialize()
doc.root() / doc.root_mut()
doc.get::<T>(path)
doc.insert_path(path, value)
Table::get / get_mut / insert / remove / iter / keys
Value (alle Varianten)
Array / FromValue
```

### Neu

```rust
// Format-bewusste Methoden (optional nutzbar)
doc.items() -> &[DocumentItem]
doc.entry_node(path) -> Option<&EntryNode>
doc.entry_node_mut(path) -> Option<&mut EntryNode>
```

### Änderungsverhalten

| Operation | Verhalten |
|---|---|
| `doc.root_mut().insert(key, value)` | Neuer Eintrag mit Standard-Formatierung (`key = value\n`) |
| `doc.root_mut().remove(key)` | Entfernt Eintrag inkl. seiner `leading_trivia` aus `items` |
| Wert eines bestehenden Eintrags ändern | `raw` in `Node::Scalar` wird auf `None` gesetzt → Serializer gibt neu generierten Text aus |
| `doc.serialize()` | Läuft über `items`, gibt jeden `raw`-Text direkt aus |

---

## Serializer: radikal vereinfacht

Der aktuelle Serializer enthält Logik für Einrückung, Inline-Heuristiken
und Schlüssel-Formatierung (~300 Zeilen). Mit dem CST-Ansatz entfällt das
fast vollständig:

```
für jedes DocumentItem:
    führe leading_trivia aus
    falls Entry:
        gib key.raw + pre_eq + "=" + post_eq + node.raw + trailing.raw aus
    falls Section:
        gib header.raw + trailing.raw aus
    falls TrailingTrivia:
        gib trivia.raw aus
```

Neue oder geänderte Werte (ohne `raw`) durchlaufen einen minimalen
Fallback-Formatter, der einen kanonischen Einzeiler erzeugt.

---

## Parser: Erweiterungen

Der Parser muss jetzt **alles** aufzeichnen, was er überfliegt:

- `skip_whitespace()` → gibt den übersprungenen Text zurück (statt ihn zu verwerfen)
- `skip_comment()` → gibt Kommentartext zurück
- `parse_string()` → gibt Originaltext *und* dekodierten Wert zurück
- `parse_integer()` / `parse_float()` → gibt Originaltext zurück
- `parse_key()` → gibt `KeyRepr` zurück

Der grundlegende Algorithmus (rekursiver Abstieg) bleibt unverändert.

---

## Versionierung und Umfang

- **Version 0.2.0** – Breaking change (neues `Document`-Internas, neue Typen in
  `pub`-API; `Value`, `Table`, `Array`, `FromValue` bleiben stabil)
- **MSRV** bleibt Rust 1.70
- Die bestehenden 97 Unit-Tests bleiben vollständig gültig
- Neue Tests für Format-Erhalt hinzufügen (vorher/nachher Vergleiche)

---

## Komplexitätsabschätzung

| Komponente | Aufwand |
|---|---|
| Neue Typen (`Trivia`, `KeyRepr`, `Node`, …) | mittel |
| Parser-Erweiterung (Trivia erfassen) | hoch |
| `Document`-Umbau (duale Darstellung) | hoch |
| Synchronisierung bei Mutationen | mittel |
| Serializer-Vereinfachung | niedrig |
| Tests | mittel |

Gesamtaufwand: vollständige Neufassung von `parser.rs` und `document.rs`,
kleinere Änderungen in `value.rs` und `serializer.rs`. Die öffentliche
API bleibt für bestehende Nutzer weitgehend kompatibel.
