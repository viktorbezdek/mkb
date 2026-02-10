//! # mkb-core
//!
//! Core types, schemas, and temporal model for the MKB knowledge base.
//!
//! This crate defines the foundational types used across all other MKB crates:
//! - [`Document`] — the central knowledge unit
//! - Temporal types ([`ObservedAt`], [`ValidUntil`], [`TemporalPrecision`])
//! - [`Link`] — typed relationships between documents
//! - [`SchemaDefinition`] — document type contracts
//! - Error hierarchy ([`MkbError`], [`TemporalError`], [`SchemaError`])

pub mod document;
pub mod error;
pub mod frontmatter;
pub mod link;
pub mod schema;
pub mod temporal;

pub use document::Document;
pub use error::{MkbError, Result};
pub use link::Link;
pub use temporal::{TemporalFields, TemporalPrecision};
