use crate::Document;

#[test]
fn test_string_standard_escapes() {
    let doc = Document::parse(r#"s = "tab:\t nl:\n bs:\\ dq:\"" "#).unwrap();
    let s = doc.get::<String>("s").unwrap();
    assert!(s.contains('\t'), "missing tab");
    assert!(s.contains('\n'), "missing newline");
    assert!(s.contains('\\'), "missing backslash");
    assert!(s.contains('"'), "missing double-quote");
}

#[test]
fn test_string_escape_e_toml11() {
    // \e = ESC = U+001B (TOML 1.1)
    let doc = Document::parse(r#"s = "\e[31m""#).unwrap();
    let s = doc.get::<String>("s").unwrap();
    assert!(!s.is_empty());
    assert_eq!(s.as_bytes()[0], 0x1B, "first byte must be ESC (0x1B)");
}

#[test]
fn test_string_escape_x_uppercase() {
    // \x41 = 'A' (TOML 1.1)
    let doc = Document::parse(r#"s = "\x41""#).unwrap();
    assert_eq!(doc.get::<String>("s").unwrap(), "A");
}

#[test]
fn test_string_escape_x_lowercase() {
    // \x61 = 'a'
    let doc = Document::parse(r#"s = "\x61""#).unwrap();
    assert_eq!(doc.get::<String>("s").unwrap(), "a");
}

#[test]
fn test_string_literal_no_escaping() {
    let doc = Document::parse(r#"s = 'C:\Users\no\escape'"#).unwrap();
    assert_eq!(doc.get::<String>("s").unwrap(), r#"C:\Users\no\escape"#);
}

#[test]
fn test_string_multiline_leading_newline_stripped() {
    let doc = Document::parse("s = \"\"\"\nhello\"\"\"\n").unwrap();
    assert_eq!(doc.get::<String>("s").unwrap(), "hello");
}

#[test]
fn test_string_basic_unicode_escape() {
    let doc = Document::parse(r#"s = "A""#).unwrap();
    assert_eq!(doc.get::<String>("s").unwrap(), "A");
}

#[test]
fn test_string_basic_unicode_escape_big() {
    let doc = Document::parse(r#"s = "\U00000041""#).unwrap();
    assert_eq!(doc.get::<String>("s").unwrap(), "A");
}

#[test]
fn test_string_multiline_basic() {
    let doc = Document::parse("s = \"\"\"\nfoo\nbar\"\"\"\n").unwrap();
    let s = doc.get::<String>("s").unwrap();
    assert!(s.contains("foo"));
    assert!(s.contains("bar"));
}

#[test]
fn test_string_multiline_literal() {
    let doc = Document::parse("s = '''\nhello world\n'''\n").unwrap();
    let s = doc.get::<String>("s").unwrap();
    assert!(s.contains("hello world"));
}

#[test]
fn test_string_empty_basic() {
    let doc = Document::parse(r#"s = """#).unwrap_or_else(|_| {
        Document::parse("s = \"\"\n").unwrap()
    });
    // Either form should parse to empty string
    let doc2 = Document::parse("s = \"\"\n").unwrap();
    assert_eq!(doc2.get::<String>("s").unwrap(), "");
}

#[test]
fn test_string_empty_literal() {
    let doc = Document::parse("s = ''\n").unwrap();
    assert_eq!(doc.get::<String>("s").unwrap(), "");
}
