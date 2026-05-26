use crate::{Document, Table, Array, Value, TomlErrorKind};

#[test]
fn test_inline_table_basic() {
    let doc = Document::parse("p = {x = 1, y = 2}\n").unwrap();
    let tbl = doc.get::<Table>("p").unwrap();
    assert_eq!(*tbl.get_as::<i64>("x").unwrap(), 1);
    assert_eq!(*tbl.get_as::<i64>("y").unwrap(), 2);
}

#[test]
fn test_inline_table_trailing_comma_toml11() {
    // TOML 1.1: trailing comma in inline tables allowed
    let doc = Document::parse("p = {x = 1, y = 2,}\n").unwrap();
    let tbl = doc.get::<Table>("p").unwrap();
    assert_eq!(*tbl.get_as::<i64>("x").unwrap(), 1);
    assert_eq!(*tbl.get_as::<i64>("y").unwrap(), 2);
}

#[test]
fn test_array_of_tables() {
    let src = "[[fruits]]\nname = \"apple\"\n[[fruits]]\nname = \"banana\"\n";
    let doc = Document::parse(src).unwrap();
    let arr = doc.get::<Array>("fruits").unwrap();
    assert_eq!(arr.len(), 2);

    let t0 = arr.get(0).unwrap();
    let t1 = arr.get(1).unwrap();

    if let Value::Table(t) = t0 {
        assert_eq!(t.get_as::<String>("name").unwrap(), "apple");
    } else {
        panic!("expected Table at index 0");
    }
    if let Value::Table(t) = t1 {
        assert_eq!(t.get_as::<String>("name").unwrap(), "banana");
    } else {
        panic!("expected Table at index 1");
    }
}

#[test]
fn test_homogeneous_array() {
    let doc = Document::parse("a = [1, 2, 3]\n").unwrap();
    let arr = doc.get::<Array>("a").unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], Value::Integer(1));
    assert_eq!(arr[2], Value::Integer(3));
}

#[test]
fn test_duplicate_key_throws() {
    let result = Document::parse("a = 1\na = 2\n");
    assert!(result.is_err());
    match result.unwrap_err().kind {
        TomlErrorKind::DuplicateKey => {}
        other => panic!("expected DuplicateKey, got {:?}", other),
    }
}

#[test]
fn test_table_redefinition_throws() {
    let result = Document::parse("[a]\nb = 1\n[a]\nc = 2\n");
    assert!(result.is_err());
    match result.unwrap_err().kind {
        TomlErrorKind::DuplicateKey | TomlErrorKind::ParseError => {}
        other => panic!("expected DuplicateKey or ParseError, got {:?}", other),
    }
}

#[test]
fn test_inline_table_immutable_via_header() {
    // Inline table must not be extended by a subsequent [header]
    let result = Document::parse("a = {b = 1}\n[a]\nc = 2\n");
    assert!(result.is_err());
}

#[test]
fn test_dotted_key_navigation() {
    let doc = Document::parse("a.b.c = 42\n").unwrap();
    let val = doc.path("a.b.c").unwrap();
    assert_eq!(*val, Value::Integer(42));
}

#[test]
fn test_path_access() {
    let doc = Document::parse("[server]\nhost = \"example.com\"\nport = 8080\n").unwrap();
    let host = doc.path("server.host").unwrap();
    let port = doc.path("server.port").unwrap();
    assert_eq!(*host, Value::String("example.com".to_string()));
    assert_eq!(*port, Value::Integer(8080));
}

#[test]
fn test_boolean_values() {
    let doc = Document::parse("a = true\nb = false\n").unwrap();
    assert_eq!(*doc.get::<bool>("a").unwrap(), true);
    assert_eq!(*doc.get::<bool>("b").unwrap(), false);
}

#[test]
fn test_type_error_message_readable() {
    // TypeError message must contain the TOML type name, not a mangled name
    let doc = Document::parse("n = 42\n").unwrap();
    let result = doc.get::<String>("n");
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.message.to_lowercase();
    assert!(
        msg.contains("string"),
        "message must contain 'string', got: {}",
        err.message
    );
    // Must not contain typical C++ mangling patterns
    assert!(!msg.contains("nst"), "message must not contain mangled names");
}

#[test]
fn test_table_standard_section() {
    let doc = Document::parse("[database]\nserver = \"192.168.1.1\"\nport = 5432\n").unwrap();
    let db = doc.get::<Table>("database").unwrap();
    assert_eq!(db.get_as::<String>("server").unwrap(), "192.168.1.1");
    assert_eq!(*db.get_as::<i64>("port").unwrap(), 5432);
}

#[test]
fn test_key_not_found_error() {
    let doc = Document::parse("a = 1\n").unwrap();
    let result = doc.get::<i64>("missing");
    assert!(result.is_err());
    match result.unwrap_err().kind {
        TomlErrorKind::KeyNotFound(_) => {}
        other => panic!("expected KeyNotFound, got {:?}", other),
    }
}

#[test]
fn test_table_get_path() {
    let doc = Document::parse("[a]\n[a.b]\nc = 99\n").unwrap();
    let val = doc.path("a.b.c").unwrap();
    assert_eq!(*val, Value::Integer(99));
}

#[test]
fn test_array_trailing_comma() {
    let doc = Document::parse("a = [1, 2, 3,]\n").unwrap();
    let arr = doc.get::<Array>("a").unwrap();
    assert_eq!(arr.len(), 3);
}

#[test]
fn test_nested_inline_tables() {
    let doc = Document::parse("a = {x = {y = 1}}\n").unwrap();
    let val = doc.path("a.x.y").unwrap();
    assert_eq!(*val, Value::Integer(1));
}

#[test]
fn test_array_of_tables_with_values() {
    let src = "[[products]]\nname = \"widget\"\nprice = 9.99\n[[products]]\nname = \"gadget\"\nprice = 19.99\n";
    let doc = Document::parse(src).unwrap();
    let arr = doc.get::<Array>("products").unwrap();
    assert_eq!(arr.len(), 2);
}
