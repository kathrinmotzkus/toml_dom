use crate::{Document, LocalDate, LocalDateTime, LocalTime, OffsetDateTime};

#[test]
fn test_local_date_fields() {
    let doc = Document::parse("d = 2024-03-15\n").unwrap();
    let v = doc.get::<LocalDate>("d").unwrap();
    assert_eq!(v.year, 2024);
    assert_eq!(v.month, 3);
    assert_eq!(v.day, 15);
}

#[test]
fn test_local_time_without_seconds() {
    // TOML 1.1: seconds are optional
    let doc = Document::parse("t = 14:30\n").unwrap();
    let v = doc.get::<LocalTime>("t").unwrap();
    assert_eq!(v.hour, 14);
    assert_eq!(v.minute, 30);
    assert_eq!(v.second, 0);
    assert_eq!(v.nanosecond, 0);
}

#[test]
fn test_local_time_with_seconds() {
    let doc = Document::parse("t = 14:30:59\n").unwrap();
    let v = doc.get::<LocalTime>("t").unwrap();
    assert_eq!(v.hour, 14);
    assert_eq!(v.minute, 30);
    assert_eq!(v.second, 59);
    assert_eq!(v.nanosecond, 0);
}

#[test]
fn test_local_time_nanoseconds() {
    let doc = Document::parse("t = 14:30:59.123456789\n").unwrap();
    let v = doc.get::<LocalTime>("t").unwrap();
    assert_eq!(v.nanosecond, 123_456_789);
}

#[test]
fn test_local_time_fraction_padding() {
    // ".1" → 100 ms = 100_000_000 ns
    let doc = Document::parse("t = 00:00:00.1\n").unwrap();
    let v = doc.get::<LocalTime>("t").unwrap();
    assert_eq!(v.nanosecond, 100_000_000);
}

#[test]
fn test_local_datetime_t_delimiter() {
    let doc = Document::parse("dt = 2024-03-15T14:30:00\n").unwrap();
    let v = doc.get::<LocalDateTime>("dt").unwrap();
    assert_eq!(v.date.year, 2024);
    assert_eq!(v.date.month, 3);
    assert_eq!(v.date.day, 15);
    assert_eq!(v.time.hour, 14);
    assert_eq!(v.time.minute, 30);
    assert_eq!(v.time.second, 0);
}

#[test]
fn test_local_datetime_lowercase_t() {
    let doc = Document::parse("dt = 2024-03-15t14:30:00\n").unwrap();
    let v = doc.get::<LocalDateTime>("dt").unwrap();
    assert_eq!(v.date.year, 2024);
    assert_eq!(v.time.hour, 14);
}

#[test]
fn test_local_datetime_space_delimiter() {
    let doc = Document::parse("dt = 2024-03-15 14:30:00\n").unwrap();
    let v = doc.get::<LocalDateTime>("dt").unwrap();
    assert_eq!(v.date.year, 2024);
    assert_eq!(v.time.hour, 14);
}

#[test]
fn test_offset_datetime_utc_z() {
    let doc = Document::parse("odt = 2024-03-15T14:30:00Z\n").unwrap();
    let v = doc.get::<OffsetDateTime>("odt").unwrap();
    assert!(v.is_utc());
    assert_eq!(v.offset_minutes, OffsetDateTime::UTC_OFFSET);
    assert_eq!(v.date.year, 2024);
    assert_eq!(v.time.hour, 14);
}

#[test]
fn test_offset_datetime_positive() {
    let doc = Document::parse("odt = 2024-03-15T14:30:00+05:30\n").unwrap();
    let v = doc.get::<OffsetDateTime>("odt").unwrap();
    assert!(!v.is_utc());
    assert_eq!(v.offset_minutes, 5 * 60 + 30);
}

#[test]
fn test_offset_datetime_negative() {
    let doc = Document::parse("odt = 2024-03-15T14:30:00-08:00\n").unwrap();
    let v = doc.get::<OffsetDateTime>("odt").unwrap();
    assert_eq!(v.offset_minutes, -(8 * 60));
}

#[test]
fn test_datetime_roundtrip() {
    let src = "d = 2024-03-15\nodt = 2024-03-15T14:30:00Z\n";
    let doc = Document::parse(src).unwrap();
    let serialized = doc.serialize();
    let doc2 = Document::parse(&serialized).unwrap();
    assert_eq!(
        doc2.get::<LocalDate>("d").unwrap(),
        doc.get::<LocalDate>("d").unwrap()
    );
    assert_eq!(
        doc2.get::<OffsetDateTime>("odt").unwrap(),
        doc.get::<OffsetDateTime>("odt").unwrap()
    );
}

#[test]
fn test_offset_datetime_display_utc() {
    let dt = OffsetDateTime {
        date: LocalDate { year: 1979, month: 5, day: 27 },
        time: LocalTime { hour: 7, minute: 32, second: 0, nanosecond: 0 },
        offset_minutes: OffsetDateTime::UTC_OFFSET,
    };
    assert_eq!(dt.to_string(), "1979-05-27T07:32:00Z");
}

#[test]
fn test_offset_datetime_display_positive_offset() {
    let dt = OffsetDateTime {
        date: LocalDate { year: 2024, month: 1, day: 1 },
        time: LocalTime { hour: 12, minute: 0, second: 0, nanosecond: 0 },
        offset_minutes: 5 * 60 + 30,
    };
    assert_eq!(dt.to_string(), "2024-01-01T12:00:00+05:30");
}

#[test]
fn test_local_date_display() {
    let d = LocalDate { year: 2024, month: 3, day: 15 };
    assert_eq!(d.to_string(), "2024-03-15");
}

#[test]
fn test_local_time_display_with_nanoseconds() {
    let t = LocalTime { hour: 14, minute: 30, second: 59, nanosecond: 123_000_000 };
    assert!(t.to_string().starts_with("14:30:59.123"));
}
