use crate::{Document, SerializeOptions};

#[test]
fn test_serializer_full_roundtrip() {
    let src = "title = \"Test\"\ncount = 42\nratio = 3.14\nflag = true\n";
    let doc = Document::parse(src).unwrap();
    let serialized = doc.serialize();
    let doc2 = Document::parse(&serialized).unwrap();
    assert_eq!(doc2.get::<String>("title").unwrap(), "Test");
    assert_eq!(*doc2.get::<i64>("count").unwrap(), 42);
    assert_eq!(*doc2.get::<f64>("ratio").unwrap(), 3.14);
    assert_eq!(*doc2.get::<bool>("flag").unwrap(), true);
}

#[test]
fn test_serializer_float_always_has_dot_or_e() {
    // Float must always have '.' or 'e' in serialized form
    let doc = Document::parse("f = 1.0\n").unwrap();
    let out = doc.serialize();
    // Find the value part (after '=')
    let pos_eq = out.find('=').unwrap();
    let num_part = &out[pos_eq + 2..];
    assert!(
        num_part.contains('.') || num_part.contains('e') || num_part.contains('E'),
        "float must contain '.' or 'e': got '{}'",
        num_part.trim()
    );
    // Must be re-parseable
    let doc2 = Document::parse(&out).unwrap();
    assert_eq!(*doc2.get::<f64>("f").unwrap(), 1.0);
}

#[test]
fn test_serializer_quoted_keys_roundtrip() {
    let doc = Document::parse(r#""key with spaces" = 1"#).unwrap();
    let out = doc.serialize();
    let doc2 = Document::parse(&out).unwrap();
    assert_eq!(*doc2.get::<i64>("key with spaces").unwrap(), 1);
}

#[test]
fn test_serializer_sort_keys() {
    let src = "z = 3\na = 1\nm = 2\n";
    let doc = Document::parse(src).unwrap();
    let opts = SerializeOptions {
        sort_keys: true,
        ..Default::default()
    };
    let out = doc.serialize_with(&opts);
    let pos_a = out.find("a =").unwrap();
    let pos_m = out.find("m =").unwrap();
    let pos_z = out.find("z =").unwrap();
    assert!(pos_a < pos_m, "a should come before m");
    assert!(pos_m < pos_z, "m should come before z");
}

#[test]
fn test_serializer_trailing_newline_true() {
    let doc = Document::parse("x = 1\n").unwrap();
    let opts = SerializeOptions {
        trailing_newline: true,
        ..Default::default()
    };
    let out = doc.serialize_with(&opts);
    assert!(out.ends_with('\n'), "should end with newline");
}

#[test]
fn test_serializer_float_inf() {
    let src = "f = inf\n";
    let doc = Document::parse(src).unwrap();
    let out = doc.serialize();
    assert!(out.contains("inf"), "inf should serialize as 'inf'");
    let doc2 = Document::parse(&out).unwrap();
    let v = *doc2.get::<f64>("f").unwrap();
    assert!(v.is_infinite() && v.is_sign_positive());
}

#[test]
fn test_serializer_float_neg_inf() {
    let src = "f = -inf\n";
    let doc = Document::parse(src).unwrap();
    let out = doc.serialize();
    assert!(out.contains("-inf"), "-inf should serialize as '-inf'");
    let doc2 = Document::parse(&out).unwrap();
    let v = *doc2.get::<f64>("f").unwrap();
    assert!(v.is_infinite() && v.is_sign_negative());
}

#[test]
fn test_serializer_float_nan() {
    let src = "f = nan\n";
    let doc = Document::parse(src).unwrap();
    let out = doc.serialize();
    assert!(out.contains("nan"), "nan should serialize as 'nan'");
    let doc2 = Document::parse(&out).unwrap();
    let v = *doc2.get::<f64>("f").unwrap();
    assert!(v.is_nan());
}

#[test]
fn test_serializer_string_escaping() {
    use crate::Value;
    let mut doc = Document::parse("").unwrap();
    doc.root_mut().insert("s", Value::String("tab:\there".to_string()));
    let out = doc.serialize();
    // Must contain escaped \t
    assert!(out.contains(r"\t"), "tab must be escaped in output");
    let doc2 = Document::parse(&out).unwrap();
    assert!(doc2.get::<String>("s").unwrap().contains('\t'));
}

#[test]
fn test_serializer_array() {
    let src = "a = [1, 2, 3]\n";
    let doc = Document::parse(src).unwrap();
    let out = doc.serialize();
    let doc2 = Document::parse(&out).unwrap();
    let arr = doc2.get::<crate::Array>("a").unwrap();
    assert_eq!(arr.len(), 3);
}

#[test]
fn test_serializer_table_section() {
    let src = "[server]\nhost = \"localhost\"\nport = 8080\n";
    let doc = Document::parse(src).unwrap();
    let out = doc.serialize();
    let doc2 = Document::parse(&out).unwrap();
    assert_eq!(
        doc2.path("server.host").unwrap(),
        &crate::Value::String("localhost".to_string())
    );
}
