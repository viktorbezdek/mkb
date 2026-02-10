//! Link type — typed relationships between documents.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A typed, timestamped relationship between two documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// Relationship type (e.g., "owner", "blocked_by", "has_signal").
    pub rel: String,

    /// Target document reference (e.g., "people/jane-smith").
    pub target: String,

    /// When this relationship was observed. Links carry their own timestamp.
    pub observed_at: DateTime<Utc>,

    /// Optional metadata for the relationship.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_serialization_roundtrip() {
        let link = Link {
            rel: "owner".to_string(),
            target: "people/jane-smith".to_string(),
            observed_at: Utc::now(),
            metadata: None,
        };

        let json = serde_json::to_string(&link).expect("serialize");
        let deserialized: Link = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(link.rel, deserialized.rel);
        assert_eq!(link.target, deserialized.target);
    }
}
