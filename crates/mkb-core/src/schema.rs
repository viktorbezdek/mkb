//! Schema definition types for MKB document type contracts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A schema definition that describes the frontmatter contract for a document type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDefinition {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: HashMap<String, FieldDef>,
    #[serde(default)]
    pub validation: Vec<ValidationRule>,
}

fn default_version() -> u32 {
    1
}

/// Definition of a single field in a schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub indexed: bool,
    #[serde(default)]
    pub searchable: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// For enum types: allowed values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    /// For ref types: target document type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Supported field types in MKB schemas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Integer,
    Float,
    Boolean,
    Date,
    Datetime,
    Duration,
    #[serde(rename = "enum")]
    Enum,
    #[serde(rename = "ref")]
    Ref,
    #[serde(rename = "ref[]")]
    RefArray,
    #[serde(rename = "string[]")]
    StringArray,
    Map,
    Json,
}

/// A validation rule defined in a schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    pub rule: String,
    pub message: String,
    #[serde(default = "default_severity")]
    pub severity: ValidationSeverity,
}

/// Severity level for validation rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationSeverity {
    Fatal,
    Warning,
    Info,
}

fn default_severity() -> ValidationSeverity {
    ValidationSeverity::Fatal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_type_enum_roundtrip() {
        let ft = FieldType::StringArray;
        let json = serde_json::to_string(&ft).expect("serialize");
        assert_eq!(json, "\"string[]\"");
        let back: FieldType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, FieldType::StringArray);
    }
}
