//! YAML frontmatter parsing and writing.
//!
//! Handles the `---` delimited YAML frontmatter in markdown files.
//! Format:
//! ```markdown
//! ---
//! id: "proj-alpha-001"
//! type: project
//! title: "Alpha Project"
//! observed_at: 2025-02-10T09:15:00Z
//! ...
//! ---
//!
//! ## Body content here
//! ```

use crate::document::Document;
use crate::error::MkbError;

/// Parse a markdown file into frontmatter YAML and body content.
///
/// Returns `(yaml_str, body)` where `yaml_str` is the raw YAML between
/// `---` delimiters and `body` is everything after the closing `---`.
///
/// # Errors
///
/// Returns [`MkbError::Parse`] if the file does not contain valid frontmatter.
pub fn split_frontmatter(content: &str) -> Result<(&str, &str), MkbError> {
    let content = content.trim_start();

    if !content.starts_with("---") {
        return Err(MkbError::Parse(
            "File must start with '---' frontmatter delimiter".to_string(),
        ));
    }

    // Find the closing ---
    let after_first = &content[3..];
    let after_first = after_first.trim_start_matches(['\r', '\n']);

    let close_pos = after_first.find("\n---").ok_or_else(|| {
        MkbError::Parse("No closing '---' frontmatter delimiter found".to_string())
    })?;

    let yaml = &after_first[..close_pos];
    let rest = &after_first[close_pos + 4..]; // skip \n---

    // Skip the newline after closing ---
    let body = rest.strip_prefix('\n').unwrap_or(rest);
    let body = body.strip_prefix('\r').unwrap_or(body);

    Ok((yaml, body))
}

/// Parse a markdown file with YAML frontmatter into a [`Document`].
///
/// # Errors
///
/// Returns [`MkbError::Parse`] if frontmatter is missing or malformed.
/// Returns [`MkbError::Serialization`] if YAML cannot be deserialized.
pub fn parse_document(content: &str) -> Result<Document, MkbError> {
    let (yaml, body) = split_frontmatter(content)?;

    let mut doc: Document =
        serde_yaml::from_str(yaml).map_err(|e| MkbError::Serialization(e.to_string()))?;
    doc.body = body.to_string();

    Ok(doc)
}

/// Write a [`Document`] as a markdown file with YAML frontmatter.
///
/// # Errors
///
/// Returns [`MkbError::Serialization`] if the document cannot be serialized.
pub fn write_document(doc: &Document) -> Result<String, MkbError> {
    let yaml = serde_yaml::to_string(doc).map_err(|e| MkbError::Serialization(e.to_string()))?;

    let mut output = String::with_capacity(yaml.len() + doc.body.len() + 10);
    output.push_str("---\n");
    output.push_str(&yaml);
    output.push_str("---\n");
    if !doc.body.is_empty() {
        output.push('\n');
        output.push_str(&doc.body);
        if !doc.body.ends_with('\n') {
            output.push('\n');
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temporal::{DecayProfile, RawTemporalInput, TemporalPrecision};
    use chrono::{TimeZone, Utc};

    fn utc(y: i32, m: u32, d: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    #[test]
    fn split_frontmatter_extracts_yaml_and_body() {
        let content = "---\nid: test\ntype: project\n---\n\n## Hello\n";
        let (yaml, body) = split_frontmatter(content).unwrap();
        assert!(yaml.contains("id: test"));
        assert!(yaml.contains("type: project"));
        assert!(body.contains("## Hello"));
    }

    #[test]
    fn split_frontmatter_rejects_missing_opener() {
        let content = "id: test\ntype: project\n---\n";
        let result = split_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn split_frontmatter_rejects_missing_closer() {
        let content = "---\nid: test\ntype: project\n";
        let result = split_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn parse_frontmatter_from_markdown() {
        let content = r#"---
id: "proj-alpha-001"
type: project
title: "Alpha Project"
_created_at: "2025-02-10T00:00:00Z"
_modified_at: "2025-02-10T00:00:00Z"
observed_at: "2025-02-10T00:00:00Z"
valid_until: "2025-08-10T00:00:00Z"
temporal_precision: day
confidence: 0.95
---

## Project Description

This is the Alpha project.
"#;

        let doc = parse_document(content).unwrap();
        assert_eq!(doc.id, "proj-alpha-001");
        assert_eq!(doc.doc_type, "project");
        assert_eq!(doc.title, "Alpha Project");
        assert_eq!(doc.temporal.observed_at, utc(2025, 2, 10));
        assert_eq!(doc.temporal.valid_until, utc(2025, 8, 10));
        assert_eq!(doc.temporal.temporal_precision, TemporalPrecision::Day);
        assert!(doc.body.contains("## Project Description"));
        assert!(doc.body.contains("Alpha project"));
    }

    #[test]
    fn write_frontmatter_to_markdown() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 2, 10)),
            valid_until: Some(utc(2025, 8, 10)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
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
        doc.body = "## Hello World\n".to_string();

        let output = write_document(&doc).unwrap();
        assert!(output.starts_with("---\n"));
        assert!(output.contains("proj-alpha-001"));
        assert!(output.contains("observed_at"));
        assert!(output.contains("## Hello World"));

        // Should have closing ---
        let parts: Vec<&str> = output.split("---\n").collect();
        assert!(parts.len() >= 3, "Should have opening and closing ---");
    }

    #[test]
    fn rejects_yaml_without_observed_at() {
        // YAML missing observed_at should fail deserialization
        let content = r#"---
id: "test-001"
type: project
title: "Test"
_created_at: "2025-02-10T00:00:00Z"
_modified_at: "2025-02-10T00:00:00Z"
valid_until: "2025-08-10T00:00:00Z"
confidence: 1.0
---

Body here.
"#;

        let result = parse_document(content);
        // serde_yaml will fail because observed_at is a required field
        assert!(result.is_err());
    }

    #[test]
    fn frontmatter_roundtrip_preserves_content() {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 2, 10)),
            valid_until: Some(utc(2025, 8, 10)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();

        let mut doc = Document::new(
            "proj-test-001".to_string(),
            "project".to_string(),
            "Test Project".to_string(),
            input,
            &profile,
        )
        .unwrap();
        doc.body = "## Test Body\n\nSome content here.\n".to_string();
        doc.tags = vec!["test".to_string()];

        let written = write_document(&doc).unwrap();
        let parsed = parse_document(&written).unwrap();

        assert_eq!(doc.id, parsed.id);
        assert_eq!(doc.doc_type, parsed.doc_type);
        assert_eq!(doc.title, parsed.title);
        assert_eq!(doc.temporal.observed_at, parsed.temporal.observed_at);
        assert_eq!(doc.temporal.valid_until, parsed.temporal.valid_until);
        assert_eq!(doc.tags, parsed.tags);
        assert!(parsed.body.contains("## Test Body"));
        assert!(parsed.body.contains("Some content here."));
    }
}
