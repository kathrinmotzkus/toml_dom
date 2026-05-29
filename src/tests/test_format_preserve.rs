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
