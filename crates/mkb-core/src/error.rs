//! Error types for MKB.

use thiserror::Error;

/// Top-level result type for MKB operations.
pub type Result<T> = std::result::Result<T, MkbError>;

/// Top-level error type for MKB.
#[derive(Debug, Error)]
pub enum MkbError {
    #[error("temporal error: {0}")]
    Temporal(#[from] TemporalError),

    #[error("schema error: {0}")]
    Schema(#[from] SchemaError),

    #[error("vault error: {0}")]
    Vault(String),

    #[error("index error: {0}")]
    Index(String),

    #[error("query error: {0}")]
    Query(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Errors related to temporal grounding and decay.
#[derive(Debug, Error)]
pub enum TemporalError {
    #[error("REJECTED: No temporal grounding. Every document must have observed_at.")]
    MissingObservedAt,

    #[error("REJECTED: No valid_until. Every document must have an expiry.")]
    MissingValidUntil,

    #[error("REJECTED: No temporal_precision. Every document must declare precision.")]
    MissingPrecision,

    #[error("valid_until ({valid_until}) cannot be before observed_at ({observed_at})")]
    ValidUntilBeforeObservedAt {
        observed_at: String,
        valid_until: String,
    },

    #[error("occurred_at ({occurred_at}) should not be after observed_at ({observed_at})")]
    OccurredAtAfterObservedAt {
        observed_at: String,
        occurred_at: String,
    },
}

/// Errors related to schema validation.
#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("unknown schema type: {0}")]
    UnknownType(String),

    #[error("missing required field '{field}' for type '{doc_type}'")]
    MissingRequiredField { doc_type: String, field: String },

    #[error("invalid field type for '{field}': expected {expected}, got {actual}")]
    InvalidFieldType {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("invalid enum value '{value}' for field '{field}': allowed values are {allowed:?}")]
    InvalidEnumValue {
        field: String,
        value: String,
        allowed: Vec<String>,
    },

    #[error("schema parse error: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_display_human_readable_messages() {
        let err = TemporalError::MissingObservedAt;
        let msg = err.to_string();
        assert!(msg.contains("observed_at"));

        let err = SchemaError::MissingRequiredField {
            doc_type: "project".to_string(),
            field: "title".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("title"));
        assert!(msg.contains("project"));
    }
}
