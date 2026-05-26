use crate::Document;

#[test]
fn test_float_positive_inf() {
    let doc = Document::parse("f = inf\n").unwrap();
    let v = *doc.get::<f64>("f").unwrap();
    assert!(v.is_infinite() && v.is_sign_positive());
}

#[test]
fn test_float_negative_inf() {
    let doc = Document::parse("f = -inf\n").unwrap();
    let v = *doc.get::<f64>("f").unwrap();
    assert!(v.is_infinite() && v.is_sign_negative());
}

#[test]
fn test_float_nan() {
    let doc = Document::parse("f = nan\n").unwrap();
    let v = *doc.get::<f64>("f").unwrap();
    assert!(v.is_nan());
}

#[test]
fn test_float_decimal_underscores() {
    let doc = Document::parse("f = 1_000.5\n").unwrap();
    assert_eq!(*doc.get::<f64>("f").unwrap(), 1000.5);
}

#[test]
fn test_float_exponent() {
    let doc = Document::parse("f = 6.626e-34\n").unwrap();
    assert_eq!(*doc.get::<f64>("f").unwrap(), 6.626e-34_f64);
}

#[test]
fn test_float_basic() {
    let doc = Document::parse("f = 3.14\n").unwrap();
    assert!((doc.get::<f64>("f").unwrap() - 3.14_f64).abs() < 1e-10);
}

#[test]
fn test_float_positive_sign() {
    let doc = Document::parse("f = +3.14\n").unwrap();
    assert!((doc.get::<f64>("f").unwrap() - 3.14_f64).abs() < 1e-10);
}

#[test]
fn test_float_negative() {
    let doc = Document::parse("f = -3.14\n").unwrap();
    assert!((*doc.get::<f64>("f").unwrap() + 3.14_f64).abs() < 1e-10);
}

#[test]
fn test_float_integer_looking() {
    // 1.0 should parse as float
    let doc = Document::parse("f = 1.0\n").unwrap();
    assert_eq!(*doc.get::<f64>("f").unwrap(), 1.0_f64);
}

#[test]
fn test_float_positive_nan() {
    let doc = Document::parse("f = +nan\n").unwrap();
    let v = *doc.get::<f64>("f").unwrap();
    assert!(v.is_nan());
}

#[test]
fn test_float_positive_inf_sign() {
    let doc = Document::parse("f = +inf\n").unwrap();
    let v = *doc.get::<f64>("f").unwrap();
    assert!(v.is_infinite() && v.is_sign_positive());
}
