use crate::{Document, Value};

fn roundtrip(src: &str) -> Document {
    let doc = Document::parse(src).unwrap_or_else(|e| panic!("parse error: {}", e));
    let serialized = doc.serialize();
    Document::parse(&serialized).unwrap_or_else(|e| {
        panic!("roundtrip parse error: {}\nSerialized:\n{}", e, serialized)
    })
}

#[test]
fn test_roundtrip_simple_values() {
    let src = "title = \"Test\"\ncount = 42\nratio = 3.14\nflag = true\n";
    let doc = roundtrip(src);
    assert_eq!(doc.get::<String>("title").unwrap(), "Test");
    assert_eq!(*doc.get::<i64>("count").unwrap(), 42);
    assert_eq!(*doc.get::<bool>("flag").unwrap(), true);
}

#[test]
fn test_roundtrip_table() {
    let src = "[server]\nhost = \"example.com\"\nport = 443\n";
    let doc = roundtrip(src);
    assert_eq!(
        doc.path("server.host").unwrap(),
        &Value::String("example.com".to_string())
    );
    assert_eq!(doc.path("server.port").unwrap(), &Value::Integer(443));
}

#[test]
fn test_roundtrip_array() {
    let src = "tags = [\"rust\", \"toml\", \"library\"]\n";
    let doc = roundtrip(src);
    let arr = doc.get::<crate::Array>("tags").unwrap();
    assert_eq!(arr.len(), 3);
}

#[test]
fn test_roundtrip_array_of_tables() {
    let src = "[[fruits]]\nname = \"apple\"\n[[fruits]]\nname = \"banana\"\n";
    let doc = roundtrip(src);
    let arr = doc.get::<crate::Array>("fruits").unwrap();
    assert_eq!(arr.len(), 2);
}

#[test]
fn test_roundtrip_integer_extremes() {
    let src = format!("n = {}\n", i64::MAX);
    let doc = roundtrip(&src);
    assert_eq!(*doc.get::<i64>("n").unwrap(), i64::MAX);
}

#[test]
fn test_roundtrip_float_special() {
    let src = "a = inf\nb = -inf\nc = nan\n";
    let doc = Document::parse(src).unwrap();
    let serialized = doc.serialize();
    let doc2 = Document::parse(&serialized).unwrap();
    assert!(doc2.get::<f64>("a").unwrap().is_infinite());
    assert!(doc2.get::<f64>("c").unwrap().is_nan());
}

#[test]
fn test_roundtrip_string_with_escapes() {
    let src = "s = \"hello\\nworld\"\n";
    let doc = roundtrip(src);
    let s = doc.get::<String>("s").unwrap();
    assert!(s.contains('\n'));
}

#[test]
fn test_roundtrip_datetime() {
    let src = "d = 2024-03-15\nodt = 2024-03-15T14:30:00Z\n";
    let doc = roundtrip(src);
    let d = doc.get::<crate::LocalDate>("d").unwrap();
    assert_eq!(d.year, 2024);
    let odt = doc.get::<crate::OffsetDateTime>("odt").unwrap();
    assert!(odt.is_utc());
}

#[test]
fn test_roundtrip_dotted_keys() {
    let src = "a.b.c = 42\n";
    let doc = Document::parse(src).unwrap();
    let serialized = doc.serialize();
    let doc2 = Document::parse(&serialized).unwrap();
    assert_eq!(doc2.path("a.b.c").unwrap(), &Value::Integer(42));
}

#[test]
fn test_roundtrip_quoted_keys() {
    let src = r#""key with spaces" = 99"#;
    let doc = Document::parse(src).unwrap();
    let serialized = doc.serialize();
    let doc2 = Document::parse(&serialized).unwrap();
    assert_eq!(*doc2.get::<i64>("key with spaces").unwrap(), 99);
}

#[test]
fn test_roundtrip_multiline_string() {
    let src = "s = \"\"\"\nhello\nworld\"\"\"\n";
    let doc = Document::parse(src).unwrap();
    let s = doc.get::<String>("s").unwrap().clone();
    assert!(s.contains("hello"));
    assert!(s.contains("world"));
    // After roundtrip the value should be the same
    let serialized = doc.serialize();
    let doc2 = Document::parse(&serialized).unwrap();
    let s2 = doc2.get::<String>("s").unwrap();
    assert_eq!(s, *s2);
}

#[test]
fn test_roundtrip_nested_tables() {
    let src = "[a]\n[a.b]\n[a.b.c]\nval = 1\n";
    let doc = roundtrip(src);
    assert_eq!(doc.path("a.b.c.val").unwrap(), &Value::Integer(1));
}

#[test]
fn test_document_parse_empty() {
    let doc = Document::parse("").unwrap();
    assert!(doc.root().is_empty());
}

#[test]
fn test_document_parse_comment_only() {
    let doc = Document::parse("# just a comment\n").unwrap();
    assert!(doc.root().is_empty());
}
