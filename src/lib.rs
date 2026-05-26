#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! TOML 1.1 library — read, modify, and write TOML documents.
//!
//! This crate implements the [TOML 1.1](https://toml.io/en/v1.1.0) specification.
//! It supports all value types, inline tables with trailing commas and newlines,
//! the `\e` and `\xHH` escape sequences, optional seconds in datetime/time values,
//! precise error messages with line/column information, and round-trip serialization.
//!
//! # Quick start
//!
//! ```rust
//! use toml_dom::Document;
//!
//! // Parse a TOML string
//! let doc = Document::parse(r#"
//! [server]
//! host = "localhost"
//! port = 8080
//! "#).unwrap();
//!
//! // Read a typed value
//! let host: &String = doc.get::<String>("server.host")
//!     .unwrap_or_else(|_| doc.path("server.host")
//!         .and_then(|v| if let toml_dom::Value::String(s) = v { Some(s) } else { None })
//!         .unwrap());
//!
//! // Serialize back to TOML text
//! let toml_text = doc.serialize();
//! println!("{}", toml_text);
//! ```
//!
//! # Features
//!
//! - **`indexmap`** — Tables preserve insertion order thanks to [`indexmap::IndexMap`].
//!   This dependency is always enabled; key order in the parsed document is retained
//!   exactly as written.
//! - **`chrono`** — Bidirectional conversions between the four TOML datetime types
//!   ([`LocalDate`], [`LocalTime`], [`LocalDateTime`], [`OffsetDateTime`]) and the
//!   corresponding `chrono` types (`NaiveDate`, `NaiveTime`, `NaiveDateTime`,
//!   `DateTime<FixedOffset>`).

#![warn(missing_docs)]

pub mod datetime;
pub mod document;
pub mod error;
pub mod parser;
pub mod serializer;
pub mod value;

#[cfg(test)]
mod tests;

// Re-exports for convenient usage
pub use datetime::{LocalDate, LocalDateTime, LocalTime, OffsetDateTime};
pub use document::Document;
pub use error::{SourceLocation, TomlError, TomlErrorKind};
pub use serializer::SerializeOptions;
pub use value::{Array, FromValue, Table, Value};
