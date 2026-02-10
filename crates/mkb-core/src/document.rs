//! Document type — the central knowledge unit in MKB.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::TemporalError;
use crate::link::Link;
use crate::temporal::{DecayProfile, RawTemporalInput, TemporalFields, TemporalGate};

/// A knowledge unit in the vault. Every document is a markdown file
/// with YAML frontmatter containing structured metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    // === Identity ===
    pub id: String,
    #[serde(rename = "type")]
    pub doc_type: String,
    pub title: String,

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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
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

impl Document {
    /// Create a new document with temporal gate validation.
    ///
    /// # Errors
    ///
    /// Returns [`TemporalError`] if temporal fields fail validation
    /// (e.g., missing `observed_at`).
    pub fn new(
        id: String,
        doc_type: String,
        title: String,
        temporal_input: RawTemporalInput,
        decay_profile: &DecayProfile,
    ) -> Result<Self, TemporalError> {
        let now = Utc::now();
        let temporal = TemporalGate::validate(&temporal_input, decay_profile)?;

        Ok(Self {
            id,
            doc_type,
            title,
            created_at: now,
            modified_at: now,
            temporal,
            source: None,
            source_hash: None,
            confidence: 1.0,
            provenance: None,
            supersedes: None,
            superseded_by: None,
            superseded_at: None,
            fields: HashMap::new(),
            tags: Vec::new(),
            links: Vec::new(),
            body: String::new(),
        })
    }

    /// Generate a document ID from type and title.
    ///
    /// Format: `<type>-<slug>-<counter>`
    /// Example: `proj-alpha-001`
    #[must_use]
    pub fn generate_id(doc_type: &str, title: &str, counter: u32) -> String {
        let type_prefix = &doc_type[..doc_type.len().min(4)];
        let slug: String = title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join("-");
        let slug = &slug[..slug.len().min(30)];
        format!("{type_prefix}-{slug}-{counter:03}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temporal::TemporalPrecision;
    use chrono::TimeZone;

    fn utc(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    #[test]
    fn document_default_confidence_is_one() {
        assert!((default_confidence() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn document_requires_observed_at() {
        let input = RawTemporalInput::default(); // no observed_at!
        let profile = DecayProfile::default_profile();

        let result = Document::new(
            "test-001".to_string(),
            "project".to_string(),
            "Test Project".to_string(),
            input,
            &profile,
        );

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("observed_at"));
    }

    #[test]
    fn document_creates_with_valid_temporal() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 2, 10)),
            valid_until: None,
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();

        let doc = Document::new(
            "proj-alpha-001".to_string(),
            "project".to_string(),
            "Alpha Project".to_string(),
            input,
            &profile,
        )
        .expect("should create document");

        assert_eq!(doc.id, "proj-alpha-001");
        assert_eq!(doc.doc_type, "project");
        assert_eq!(doc.title, "Alpha Project");
        assert_eq!(doc.temporal.observed_at, utc(2025, 2, 10));
        assert_eq!(doc.temporal.temporal_precision, TemporalPrecision::Day);
        assert!((doc.confidence - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn document_serializes_to_yaml_frontmatter() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 2, 10)),
            valid_until: Some(utc(2025, 8, 10)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();

        let doc = Document::new(
            "proj-alpha-001".to_string(),
            "project".to_string(),
            "Alpha Project".to_string(),
            input,
            &profile,
        )
        .unwrap();

        let yaml = serde_yaml::to_string(&doc).expect("should serialize to YAML");
        assert!(yaml.contains("proj-alpha-001"));
        assert!(yaml.contains("project"));
        assert!(yaml.contains("Alpha Project"));
        assert!(yaml.contains("observed_at"));
        assert!(yaml.contains("valid_until"));
    }

    #[test]
    fn document_roundtrip_preserves_all_fields() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 2, 10)),
            valid_until: Some(utc(2025, 8, 10)),
            temporal_precision: Some(TemporalPrecision::Exact),
            occurred_at: Some(utc(2025, 2, 9)),
        };
        let profile = DecayProfile::default_profile();

        let mut doc = Document::new(
            "proj-alpha-001".to_string(),
            "project".to_string(),
            "Alpha Project".to_string(),
            input,
            &profile,
        )
        .unwrap();
        doc.tags = vec!["rust".to_string(), "ai".to_string()];
        doc.source = Some("manual".to_string());
        doc.confidence = 0.95;

        let yaml = serde_yaml::to_string(&doc).expect("serialize");
        let back: Document = serde_yaml::from_str(&yaml).expect("deserialize");

        assert_eq!(doc.id, back.id);
        assert_eq!(doc.doc_type, back.doc_type);
        assert_eq!(doc.title, back.title);
        assert_eq!(doc.temporal.observed_at, back.temporal.observed_at);
        assert_eq!(doc.temporal.valid_until, back.temporal.valid_until);
        assert_eq!(
            doc.temporal.temporal_precision,
            back.temporal.temporal_precision
        );
        assert_eq!(doc.temporal.occurred_at, back.temporal.occurred_at);
        assert_eq!(doc.tags, back.tags);
        assert_eq!(doc.source, back.source);
        assert!((doc.confidence - back.confidence).abs() < f64::EPSILON);
    }

    #[test]
    fn generate_id_formats_correctly() {
        let id = Document::generate_id("project", "Alpha Project", 1);
        assert_eq!(id, "proj-alpha-project-001");

        let id = Document::generate_id("meeting", "Sprint Review Q4", 42);
        assert_eq!(id, "meet-sprint-review-q4-042");
    }
}
