//! # mkb-core
//!
//! Core types, schemas, and temporal model for the MKB knowledge base.
//!
//! This crate defines the foundational types used across all other MKB crates:
//! - [`Document`] — the central knowledge unit
//! - Temporal types ([`TemporalFields`], [`TemporalPrecision`], [`TemporalGate`])
//! - [`RawTemporalInput`] — pre-validation temporal input
//! - [`DecayProfile`] — configurable decay for `valid_until` computation
//! - [`Link`] — typed relationships between documents
//! - [`SchemaDefinition`] — document type contracts
//! - Error hierarchy ([`MkbError`], [`TemporalError`], [`SchemaError`])
//! - Frontmatter parsing ([`frontmatter`])

pub mod document;
pub mod error;
pub mod frontmatter;
pub mod link;
pub mod schema;
pub mod temporal;
pub mod view;

pub use document::Document;
pub use error::{MkbError, Result};
pub use link::Link;
pub use temporal::{
    DecayModel, DecayProfile, RawTemporalInput, TemporalFields, TemporalGate, TemporalPrecision,
};
pub use view::SavedView;
