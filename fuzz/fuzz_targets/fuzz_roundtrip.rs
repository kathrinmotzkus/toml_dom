#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Ignore non-UTF-8 input and parse failures (not what we're testing here)
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(doc) = toml_dom::Document::parse(s) else {
        return;
    };

    // Serialize once — uses the format-preserving items path
    let s2 = doc.serialize();

    // The serialized output must always be valid TOML
    let doc2 = toml_dom::Document::parse(&s2)
        .expect("format-preserving serialize must produce valid TOML");

    // Serializing again must be identical — the items path is idempotent
    let s3 = doc2.serialize();
    assert_eq!(s2, s3, "second serialize must equal first (idempotency)");
});
