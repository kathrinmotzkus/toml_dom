#![no_main]
use libfuzzer_sys::fuzz_target;
use toml_dom::{Document, Value};

// Input layout:
//   byte[0]  — which top-level key to mutate (modulo key count)
//   byte[1]  — which value type (0=Integer 1=Boolean 2=Float 3=String)
//   byte[2]  — value payload
//   rest     — TOML source text
fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let (ctrl, rest) = data.split_at(3);
    let Ok(s) = std::str::from_utf8(rest) else {
        return;
    };
    let Ok(mut doc) = Document::parse(s) else {
        return;
    };

    // Collect top-level keys; skip documents with no keys
    let keys: Vec<String> = doc.root().keys().map(|k| k.to_string()).collect();
    if keys.is_empty() {
        return;
    }
    let key = &keys[ctrl[0] as usize % keys.len()];

    // Build a new value from the control bytes
    let new_val = match ctrl[1] % 4 {
        0 => Value::Integer(ctrl[2] as i64 - 128),
        1 => Value::Boolean(ctrl[2] % 2 == 0),
        2 => Value::Float(ctrl[2] as f64 / 10.0),
        _ => Value::String(format!("fuzz_{}", ctrl[2])),
    };

    // Mutate and serialize — must never panic
    doc.set_value(&[key.as_str()], new_val.clone());
    let serialized = doc.serialize();

    // The result must always be valid TOML
    let doc2 = Document::parse(&serialized)
        .expect("set_value output must be valid TOML");

    // The modified key must carry the new value in the re-parsed document
    if let Some(v) = doc2.root().get(key) {
        match (&new_val, v) {
            (Value::Integer(a), Value::Integer(b)) => assert_eq!(a, b),
            (Value::Boolean(a), Value::Boolean(b)) => assert_eq!(a, b),
            (Value::String(a),  Value::String(b))  => assert_eq!(a, b),
            // Float: NaN != NaN, so only compare finite values
            (Value::Float(a), Value::Float(b)) if a.is_finite() => {
                assert!((a - b).abs() < 1e-9, "float roundtrip mismatch: {} vs {}", a, b);
            }
            _ => {} // type mismatch can happen if key held a table/array
        }
    }
});
