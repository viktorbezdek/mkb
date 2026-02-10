//! Document type — the central knowledge unit in MKB.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::link::Link;
use crate::temporal::TemporalFields;

/// A knowledge unit in the vault. Every document is a markdown file
/// with YAML frontmatter containing structured metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    // === Identity ===
    pub id: String,
    #[serde(rename = "type")]
    pub doc_type: String,

    // === System temporal (file lifecycle) ===
    #[serde(rename = "_created_at")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "_modified_at")]
    pub modified_at: DateTime<Utc>,

    // === Content temporal (knowledge lifecycle, MANDATORY) ===
    #[serde(flatten)]
    pub temporal: TemporalFields,

    // === Provenance ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,

    // === Supersession ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_at: Option<DateTime<Utc>>,

    // === Schema fields (type-specific, stored as dynamic map) ===
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,

    // === Tags & Links ===
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,

    // === Body (markdown content below frontmatter) ===
    #[serde(skip)]
    pub body: String,
}

fn default_confidence() -> f64 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_default_confidence_is_one() {
        assert!((default_confidence() - 1.0).abs() < f64::EPSILON);
    }
}
