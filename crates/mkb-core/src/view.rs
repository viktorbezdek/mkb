//! Saved view (named MKQL query) types.
//!
//! A saved view is a named MKQL query persisted as a YAML file
//! in `.mkb/views/`. Users can run views by name instead of
//! re-typing queries.

use serde::{Deserialize, Serialize};

/// A saved MKQL query with metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedView {
    /// Unique name (used as filename: `{name}.yaml`)
    pub name: String,
    /// Optional human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The MKQL query string
    pub query: String,
    /// ISO 8601 creation timestamp
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_view_yaml_roundtrip() {
        let view = SavedView {
            name: "active-projects".to_string(),
            description: Some("All currently active projects".to_string()),
            query: "SELECT * FROM project WHERE CURRENT()".to_string(),
            created_at: "2025-02-10T00:00:00Z".to_string(),
        };

        let yaml = serde_yaml::to_string(&view).expect("serialize");
        let back: SavedView = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(view, back);
    }

    #[test]
    fn saved_view_yaml_roundtrip_no_description() {
        let view = SavedView {
            name: "all-meetings".to_string(),
            description: None,
            query: "SELECT * FROM meeting".to_string(),
            created_at: "2025-02-10T00:00:00Z".to_string(),
        };

        let yaml = serde_yaml::to_string(&view).expect("serialize");
        assert!(!yaml.contains("description"));
        let back: SavedView = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(view, back);
    }
}
