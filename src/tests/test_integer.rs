use crate::{Document, TomlErrorKind};

#[test]
fn test_integer_max_ok() {
    let doc = Document::parse("n = 9223372036854775807\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), i64::MAX);
}

#[test]
fn test_integer_max_plus1_overflow() {
    let result = Document::parse("n = 9223372036854775808\n");
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Must mention "overflow"
    let msg = err.message.to_lowercase();
    assert!(
        msg.contains("overflow"),
        "expected overflow message, got: {}",
        err.message
    );
}

#[test]
fn test_integer_min_ok() {
    let doc = Document::parse("n = -9223372036854775808\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), i64::MIN);
}

#[test]
fn test_integer_min_minus1_overflow() {
    let result = Document::parse("n = -9223372036854775809\n");
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.message.to_lowercase();
    assert!(
        msg.contains("overflow"),
        "expected overflow message, got: {}",
        err.message
    );
}

#[test]
fn test_integer_hex_max_ok() {
    let doc = Document::parse("n = 0x7fffffffffffffff\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), i64::MAX);
}

#[test]
fn test_integer_hex_overflow() {
    // 0x8000000000000000 = INT64_MAX + 1
    let result = Document::parse("n = 0x8000000000000000\n");
    assert!(result.is_err());
    match result.unwrap_err().kind {
        TomlErrorKind::IntegerOverflow => {}
        other => panic!("expected IntegerOverflow, got {:?}", other),
    }
}

#[test]
fn test_integer_hex_negative_min_ok() {
    // -0x8000000000000000 = INT64_MIN
    let doc = Document::parse("n = -0x8000000000000000\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), i64::MIN);
}

#[test]
fn test_integer_hex_negative_overflow() {
    let result = Document::parse("n = -0x8000000000000001\n");
    assert!(result.is_err());
    match result.unwrap_err().kind {
        TomlErrorKind::IntegerOverflow => {}
        other => panic!("expected IntegerOverflow, got {:?}", other),
    }
}

#[test]
fn test_integer_binary_max_ok() {
    let doc = Document::parse(
        "n = 0b0111111111111111111111111111111111111111111111111111111111111111\n",
    )
    .unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), i64::MAX);
}

#[test]
fn test_integer_binary_overflow() {
    let result = Document::parse(
        "n = 0b1000000000000000000000000000000000000000000000000000000000000000\n",
    );
    assert!(result.is_err());
    match result.unwrap_err().kind {
        TomlErrorKind::IntegerOverflow => {}
        other => panic!("expected IntegerOverflow, got {:?}", other),
    }
}

#[test]
fn test_integer_decimal_underscores() {
    let doc = Document::parse("n = 1_000_000\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), 1_000_000);
}

#[test]
fn test_integer_hex_underscores() {
    let doc = Document::parse("n = 0xdead_beef\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), 0xdead_beef);
}

#[test]
fn test_integer_octal() {
    let doc = Document::parse("n = 0o755\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), 0o755);
}

#[test]
fn test_integer_positive_sign() {
    let doc = Document::parse("n = +42\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), 42);
}

#[test]
fn test_integer_negative() {
    let doc = Document::parse("n = -42\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), -42);
}

#[test]
fn test_integer_zero() {
    let doc = Document::parse("n = 0\n").unwrap();
    assert_eq!(*doc.get::<i64>("n").unwrap(), 0);
}
