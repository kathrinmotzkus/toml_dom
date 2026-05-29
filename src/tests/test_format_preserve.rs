use crate::document::Document;
use crate::value::Value;

fn roundtrip(src: &str) {
    let doc = Document::parse(src).expect("parse failed");
    let out = doc.serialize();
    assert_eq!(out, src, "format was not preserved");
}

#[test]
fn test_preserve_inline_comment() {
    roundtrip("port = 8080  # default port\n");
}

#[test]
fn test_preserve_comment_before_key() {
    roundtrip("# database settings\nhost = \"localhost\"\n");
}

#[test]
fn test_preserve_blank_lines() {
    roundtrip("a = 1\n\nb = 2\n");
}

#[test]
fn test_preserve_literal_string() {
    roundtrip("path = 'C:\\Users\\foo'\n");
}

#[test]
fn test_preserve_multiline_basic_string() {
    roundtrip("msg = \"\"\"\nhello\nworld\"\"\"\n");
}

#[test]
fn test_preserve_multiline_literal_string() {
    roundtrip("msg = '''\nhello\nworld'''\n");
}

#[test]
fn test_preserve_hex_integer() {
    roundtrip("color = 0xFF_AA_00\n");
}

#[test]
fn test_preserve_octal_integer() {
    roundtrip("perm = 0o755\n");
}

#[test]
fn test_preserve_binary_integer() {
    roundtrip("mask = 0b1010_1010\n");
}

#[test]
fn test_preserve_section_comment() {
    roundtrip("# server section\n[server]\nhost = \"localhost\"\n");
}

#[test]
fn test_preserve_inline_table() {
    roundtrip("point = { x = 1, y = 2 }\n");
}

#[test]
fn test_preserve_multiline_array() {
    roundtrip("nums = [\n  1,\n  2,\n  3,\n]\n");
}

#[test]
fn test_preserve_trailing_comma_array() {
    roundtrip("tags = [\"a\", \"b\", \"c\",]\n");
}

#[test]
fn test_preserve_whitespace_around_equals() {
    roundtrip("key  =  \"value\"\n");
}

#[test]
fn test_preserve_full_document() {
    let src = r#"# Configuration file

[server]
# The host to bind to
host = 'localhost'  # IPv4 only
port = 0xFF80  # hex port

[database]
path = 'C:\data\db.sqlite'
max_connections = 1_000

[[jobs]]
name = "backup"
interval = 3600

[[jobs]]
name = "cleanup"
interval = 86_400
"#;
    roundtrip(src);
}

#[test]
fn test_set_value_preserves_rest() {
    let src = "# comment\nport = 8080\nhost = 'localhost'\n";
    let mut doc = Document::parse(src).unwrap();
    let updated = doc.set_value(&["port"], Value::Integer(9090));
    assert!(updated);
    let out = doc.serialize();
    // comment and host line preserved, port regenerated
    assert!(out.contains("# comment\n"), "comment lost");
    assert!(out.contains("host = 'localhost'"), "literal string lost");
    assert!(out.contains("9090"), "new value missing");
}

#[test]
fn test_empty_document_roundtrip() {
    roundtrip("");
}

#[test]
fn test_only_comment_roundtrip() {
    roundtrip("# just a comment\n");
}

#[test]
fn test_dotted_key_preserved() {
    roundtrip("a.b = 1\n");
}

#[test]
fn test_roundtrip_idempotency_crash_1() {
    let s = "-.6.-=3   #";
    let doc = Document::parse(s).unwrap();
    let s2 = doc.serialize();
    eprintln!("s  = {:?}", s);
    eprintln!("s2 = {:?}", s2);
    let doc2 = Document::parse(&s2).unwrap();
    let s3 = doc2.serialize();
    eprintln!("s3 = {:?}", s3);
    assert_eq!(s2, s3, "idempotency violated");
}

#[test]
fn test_roundtrip_idempotency_debug() {
    let s = "-.6.-=3   #";
    let doc = crate::document::Document::parse(s).unwrap();
    let s2 = doc.serialize();
    let doc2 = crate::document::Document::parse(&s2).unwrap();
    let s3 = doc2.serialize();
    assert_eq!(s2, s3);
}

// ── Inline table: format preservation ────────────────────────────────────────

#[test]
fn test_preserve_inline_table_roundtrip() {
    roundtrip("point = { x = 1, y = 2 }\n");
}

#[test]
fn test_preserve_inline_table_multiline() {
    roundtrip("t = {\n  a = 1,\n  b = 2,\n}\n");
}

#[test]
fn test_preserve_inline_table_trailing_comma() {
    roundtrip("t = { a = 1, b = 2, }\n");
}

#[test]
fn test_preserve_inline_table_hex_value() {
    roundtrip("t = { port = 0x1F90, host = 'localhost' }\n");
}

// ── Inline table: set_value on individual entries ─────────────────────────────

#[test]
fn test_set_value_inline_table_entry() {
    let src = "point = { x = 1, y = 2 }\n";
    let mut doc = Document::parse(src).unwrap();
    let updated = doc.set_value(&["point", "x"], Value::Integer(99));
    assert!(updated, "set_value returned false");
    let out = doc.serialize();
    // y preserved as literal; x regenerated
    assert!(out.contains("y = 2"), "y lost");
    assert!(out.contains("x = 99"), "new x missing");
    // surrounding structure preserved
    assert!(out.starts_with("point = {"), "inline table structure lost");
    // value is still parseable and correct
    let doc2 = Document::parse(&out).unwrap();
    assert_eq!(*doc2.root().get_path("point.x").unwrap(), Value::Integer(99));
    assert_eq!(*doc2.root().get_path("point.y").unwrap(), Value::Integer(2));
}

#[test]
fn test_set_value_inline_table_string_entry() {
    let src = "conn = { host = 'localhost', port = 5432 }\n";
    let mut doc = Document::parse(src).unwrap();
    let ok = doc.set_value(&["conn", "host"], Value::String("db.example.com".into()));
    assert!(ok);
    let out = doc.serialize();
    assert!(out.contains("port = 5432"), "port lost");
    let doc2 = Document::parse(&out).unwrap();
    assert_eq!(
        *doc2.root().get_path("conn.host").unwrap(),
        Value::String("db.example.com".into())
    );
}

#[test]
fn test_set_value_inline_table_not_found() {
    let src = "point = { x = 1, y = 2 }\n";
    let mut doc = Document::parse(src).unwrap();
    let ok = doc.set_value(&["point", "z"], Value::Integer(3));
    assert!(!ok, "should return false for missing key");
}

// ── Array: format preservation ────────────────────────────────────────────────

#[test]
fn test_preserve_array_inline() {
    roundtrip("nums = [1, 2, 3]\n");
}

#[test]
fn test_preserve_array_trailing_comma() {
    roundtrip("tags = [\"a\", \"b\", \"c\",]\n");
}

#[test]
fn test_preserve_array_multiline() {
    roundtrip("nums = [\n  1,\n  2,\n  3,\n]\n");
}

#[test]
fn test_preserve_array_mixed_spacing() {
    roundtrip("v = [  1 ,  2  ]\n");
}

// ── Array: set_value on individual elements ───────────────────────────────────

#[test]
fn test_set_value_array_element() {
    let src = "nums = [10, 20, 30]\n";
    let mut doc = Document::parse(src).unwrap();
    let ok = doc.set_value(&["nums", "1"], Value::Integer(99));
    assert!(ok, "set_value returned false");
    let out = doc.serialize();
    let doc2 = Document::parse(&out).unwrap();
    let arr = doc2.get::<crate::value::Array>("nums").unwrap();
    assert_eq!(arr[0], Value::Integer(10));
    assert_eq!(arr[1], Value::Integer(99));
    assert_eq!(arr[2], Value::Integer(30));
}

#[test]
fn test_set_value_array_first_element() {
    let src = "v = [1, 2, 3]\n";
    let mut doc = Document::parse(src).unwrap();
    doc.set_value(&["v", "0"], Value::Integer(100));
    let out = doc.serialize();
    let doc2 = Document::parse(&out).unwrap();
    assert_eq!(doc2.get::<crate::value::Array>("v").unwrap()[0], Value::Integer(100));
}

// ── Nested: inline table inside a section ────────────────────────────────────

#[test]
fn test_set_value_inline_table_in_section() {
    let src = "[server]\naddr = { host = 'localhost', port = 8080 }\n";
    let mut doc = Document::parse(src).unwrap();
    let ok = doc.set_value(&["server", "addr", "port"], Value::Integer(9090));
    assert!(ok);
    let out = doc.serialize();
    assert!(out.contains("host = 'localhost'"), "host literal string lost");
    let doc2 = Document::parse(&out).unwrap();
    assert_eq!(
        *doc2.root().get_path("server.addr.port").unwrap(),
        Value::Integer(9090)
    );
}

// ── set_path: dot-string convenience ─────────────────────────────────────────

#[test]
fn test_set_path_scalar() {
    let src = "port = 8080  # default\n";
    let mut doc = Document::parse(src).unwrap();
    let ok = doc.set_path("port", Value::Integer(9090));
    assert!(ok);
    let out = doc.serialize();
    assert!(out.contains("9090"), "new value missing");
    assert!(out.contains("# default"), "comment lost");
}

#[test]
fn test_set_path_nested() {
    let src = "[server]\nport = 8080\n";
    let mut doc = Document::parse(src).unwrap();
    let ok = doc.set_path("server.port", Value::Integer(443));
    assert!(ok);
    assert_eq!(*doc.root().get_path("server.port").unwrap(), Value::Integer(443));
    // Section header preserved
    assert!(doc.serialize().contains("[server]"));
}

#[test]
fn test_set_path_not_found() {
    let src = "x = 1\n";
    let mut doc = Document::parse(src).unwrap();
    assert!(!doc.set_path("missing", Value::Integer(0)));
}

#[test]
fn test_set_path_preserves_other_entries() {
    let src = "# comment\nhost = 'localhost'\nport = 8080\n";
    let mut doc = Document::parse(src).unwrap();
    doc.set_path("port", Value::Integer(9090));
    let out = doc.serialize();
    assert!(out.contains("# comment\n"), "comment lost");
    assert!(out.contains("host = 'localhost'"), "literal string lost");
    assert!(out.contains("9090"), "new value missing");
}

// ── set_element: typed array-element mutation ─────────────────────────────────

#[test]
fn test_set_element_middle() {
    let src = "nums = [10, 20, 30]\n";
    let mut doc = Document::parse(src).unwrap();
    let ok = doc.set_element(&["nums"], 1, Value::Integer(99));
    assert!(ok);
    let out = doc.serialize();
    let doc2 = Document::parse(&out).unwrap();
    let arr = doc2.get::<crate::value::Array>("nums").unwrap();
    assert_eq!(arr[0], Value::Integer(10));
    assert_eq!(arr[1], Value::Integer(99));
    assert_eq!(arr[2], Value::Integer(30));
}

#[test]
fn test_set_element_first() {
    let src = "v = [1, 2, 3]\n";
    let mut doc = Document::parse(src).unwrap();
    assert!(doc.set_element(&["v"], 0, Value::Integer(100)));
    let out = doc.serialize();
    let doc2 = Document::parse(&out).unwrap();
    assert_eq!(doc2.get::<crate::value::Array>("v").unwrap()[0], Value::Integer(100));
}

#[test]
fn test_set_element_last() {
    let src = "v = [1, 2, 3]\n";
    let mut doc = Document::parse(src).unwrap();
    assert!(doc.set_element(&["v"], 2, Value::Integer(999)));
    let out = doc.serialize();
    let doc2 = Document::parse(&out).unwrap();
    let arr = doc2.get::<crate::value::Array>("v").unwrap();
    assert_eq!(arr[2], Value::Integer(999));
}

#[test]
fn test_set_element_out_of_bounds() {
    let src = "v = [1, 2, 3]\n";
    let mut doc = Document::parse(src).unwrap();
    assert!(!doc.set_element(&["v"], 5, Value::Integer(0)));
    // Document unchanged
    assert_eq!(doc.serialize(), src);
}

#[test]
fn test_set_element_path_not_found() {
    let src = "v = [1, 2, 3]\n";
    let mut doc = Document::parse(src).unwrap();
    assert!(!doc.set_element(&["missing"], 0, Value::Integer(0)));
}

#[test]
fn test_set_element_not_an_array() {
    let src = "x = 42\n";
    let mut doc = Document::parse(src).unwrap();
    assert!(!doc.set_element(&["x"], 0, Value::Integer(0)));
}

#[test]
fn test_set_element_in_section() {
    let src = "[data]\nids = [100, 200, 300]\n";
    let mut doc = Document::parse(src).unwrap();
    assert!(doc.set_element(&["data", "ids"], 0, Value::Integer(999)));
    let out = doc.serialize();
    assert!(out.contains("[data]"), "section header lost");
    let doc2 = Document::parse(&out).unwrap();
    let arr = doc2.root()
        .get_path("data.ids")
        .and_then(|v| if let Value::Array(a) = v { Some(a) } else { None })
        .unwrap();
    assert_eq!(arr[0], Value::Integer(999));
    assert_eq!(arr[1], Value::Integer(200));
}

#[test]
fn test_set_element_preserves_formatting() {
    // Multiline array: only the targeted element's raw is cleared
    let src = "nums = [\n  10,\n  20,\n  30,\n]\n";
    let mut doc = Document::parse(src).unwrap();
    assert!(doc.set_element(&["nums"], 1, Value::Integer(99)));
    let out = doc.serialize();
    // Multiline structure preserved for untouched elements
    assert!(out.contains("  10,"), "first element formatting lost");
    assert!(out.contains("  30,"), "last element formatting lost");
    // New value present
    assert!(out.contains("99"), "new value missing");
    // Re-parseable and correct
    let doc2 = Document::parse(&out).unwrap();
    let arr = doc2.get::<crate::value::Array>("nums").unwrap();
    assert_eq!(arr[0], Value::Integer(10));
    assert_eq!(arr[1], Value::Integer(99));
    assert_eq!(arr[2], Value::Integer(30));
}
