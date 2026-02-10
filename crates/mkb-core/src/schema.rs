//! Schema definition types and validation engine for MKB document type contracts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::SchemaError;

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

/// A validation result containing all errors/warnings found.
#[derive(Debug, Default)]
pub struct ValidationResult {
    pub errors: Vec<SchemaError>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

impl SchemaDefinition {
    /// Validate a document's fields against this schema.
    ///
    /// Checks:
    /// - All required fields are present
    /// - Field types match (basic type checking on JSON values)
    /// - Enum values are in the allowed set
    pub fn validate(
        &self,
        doc_type: &str,
        fields: &HashMap<String, serde_json::Value>,
    ) -> ValidationResult {
        let mut result = ValidationResult::default();

        for (field_name, field_def) in &self.fields {
            match fields.get(field_name) {
                None if field_def.required => {
                    result.errors.push(SchemaError::MissingRequiredField {
                        doc_type: doc_type.to_string(),
                        field: field_name.clone(),
                    });
                }
                Some(value) => {
                    // Type check
                    if let Some(err) = check_field_type(field_name, &field_def.field_type, value) {
                        result.errors.push(err);
                    }

                    // Enum value check
                    if field_def.field_type == FieldType::Enum {
                        if let Some(ref allowed) = field_def.values {
                            if let Some(s) = value.as_str() {
                                if !allowed.contains(&s.to_string()) {
                                    result.errors.push(SchemaError::InvalidEnumValue {
                                        field: field_name.clone(),
                                        value: s.to_string(),
                                        allowed: allowed.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {} // Optional field missing is fine
            }
        }

        result
    }
}

/// Check if a JSON value matches the expected field type.
fn check_field_type(
    field_name: &str,
    expected: &FieldType,
    value: &serde_json::Value,
) -> Option<SchemaError> {
    let ok = match expected {
        FieldType::String | FieldType::Date | FieldType::Datetime | FieldType::Duration => {
            value.is_string()
        }
        FieldType::Integer => value.is_i64() || value.is_u64(),
        FieldType::Float => value.is_f64() || value.is_i64() || value.is_u64(),
        FieldType::Boolean => value.is_boolean(),
        FieldType::Enum => value.is_string(),
        FieldType::Ref => value.is_string(),
        FieldType::RefArray | FieldType::StringArray => value.is_array(),
        FieldType::Map | FieldType::Json => value.is_object(),
    };

    if ok {
        None
    } else {
        Some(SchemaError::InvalidFieldType {
            field: field_name.to_string(),
            expected: format!("{expected:?}"),
            actual: json_type_name(value).to_string(),
        })
    }
}

fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Built-in schemas for common document types.
pub fn built_in_schemas() -> Vec<SchemaDefinition> {
    vec![
        project_schema(),
        meeting_schema(),
        decision_schema(),
        signal_schema(),
    ]
}

/// Schema for "project" documents.
#[must_use]
pub fn project_schema() -> SchemaDefinition {
    let mut fields = HashMap::new();
    fields.insert(
        "status".to_string(),
        FieldDef {
            field_type: FieldType::Enum,
            required: true,
            indexed: true,
            searchable: false,
            unique: false,
            default: Some(serde_json::json!("active")),
            values: Some(vec![
                "active".to_string(),
                "paused".to_string(),
                "completed".to_string(),
                "cancelled".to_string(),
            ]),
            ref_type: None,
            description: Some("Project status".to_string()),
        },
    );
    fields.insert(
        "owner".to_string(),
        FieldDef {
            field_type: FieldType::Ref,
            required: false,
            indexed: true,
            searchable: false,
            unique: false,
            default: None,
            values: None,
            ref_type: Some("person".to_string()),
            description: Some("Project owner".to_string()),
        },
    );

    SchemaDefinition {
        name: "project".to_string(),
        version: 1,
        extends: None,
        description: Some("A project being tracked".to_string()),
        fields,
        validation: vec![],
    }
}

/// Schema for "meeting" documents.
#[must_use]
pub fn meeting_schema() -> SchemaDefinition {
    let mut fields = HashMap::new();
    fields.insert(
        "attendees".to_string(),
        FieldDef {
            field_type: FieldType::StringArray,
            required: false,
            indexed: false,
            searchable: true,
            unique: false,
            default: None,
            values: None,
            ref_type: None,
            description: Some("Meeting attendees".to_string()),
        },
    );

    SchemaDefinition {
        name: "meeting".to_string(),
        version: 1,
        extends: None,
        description: Some("A meeting or discussion".to_string()),
        fields,
        validation: vec![],
    }
}

/// Schema for "decision" documents.
#[must_use]
pub fn decision_schema() -> SchemaDefinition {
    let mut fields = HashMap::new();
    fields.insert(
        "decision".to_string(),
        FieldDef {
            field_type: FieldType::String,
            required: true,
            indexed: false,
            searchable: true,
            unique: false,
            default: None,
            values: None,
            ref_type: None,
            description: Some("The decision that was made".to_string()),
        },
    );
    fields.insert(
        "rationale".to_string(),
        FieldDef {
            field_type: FieldType::String,
            required: false,
            indexed: false,
            searchable: true,
            unique: false,
            default: None,
            values: None,
            ref_type: None,
            description: Some("Why this decision was made".to_string()),
        },
    );

    SchemaDefinition {
        name: "decision".to_string(),
        version: 1,
        extends: None,
        description: Some("A decision record".to_string()),
        fields,
        validation: vec![],
    }
}

/// Schema for "signal" documents.
#[must_use]
pub fn signal_schema() -> SchemaDefinition {
    let mut fields = HashMap::new();
    fields.insert(
        "sentiment".to_string(),
        FieldDef {
            field_type: FieldType::Enum,
            required: false,
            indexed: true,
            searchable: false,
            unique: false,
            default: None,
            values: Some(vec![
                "positive".to_string(),
                "neutral".to_string(),
                "negative".to_string(),
            ]),
            ref_type: None,
            description: Some("Signal sentiment".to_string()),
        },
    );

    SchemaDefinition {
        name: "signal".to_string(),
        version: 1,
        extends: None,
        description: Some("A signal or observation".to_string()),
        fields,
        validation: vec![],
    }
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

    // === T-110.2 tests ===

    #[test]
    fn validate_project_document_against_schema() {
        let schema = project_schema();
        let mut fields = HashMap::new();
        fields.insert("status".to_string(), serde_json::json!("active"));
        fields.insert("owner".to_string(), serde_json::json!("people/jane-smith"));

        let result = schema.validate("project", &fields);
        assert!(result.is_valid(), "Errors: {:?}", result.errors);
    }

    #[test]
    fn validate_rejects_missing_required_field() {
        let schema = project_schema();
        let fields = HashMap::new(); // no status field!

        let result = schema.validate("project", &fields);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            SchemaError::MissingRequiredField { field, .. } if field == "status"
        )));
    }

    #[test]
    fn validate_rejects_wrong_type() {
        let schema = project_schema();
        let mut fields = HashMap::new();
        fields.insert(
            "status".to_string(),
            serde_json::json!(42), // should be string/enum, not number
        );

        let result = schema.validate("project", &fields);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            SchemaError::InvalidFieldType { field, .. } if field == "status"
        )));
    }

    #[test]
    fn validate_rejects_invalid_enum_value() {
        let schema = project_schema();
        let mut fields = HashMap::new();
        fields.insert(
            "status".to_string(),
            serde_json::json!("invalid_status"), // not in allowed values
        );

        let result = schema.validate("project", &fields);
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            SchemaError::InvalidEnumValue { field, value, .. }
            if field == "status" && value == "invalid_status"
        )));
    }

    #[test]
    fn all_built_in_schemas_parse_successfully() {
        let schemas = built_in_schemas();
        assert!(!schemas.is_empty());

        for schema in &schemas {
            // Each schema should serialize/deserialize correctly
            let yaml = serde_yaml::to_string(schema).expect("serialize");
            let back: SchemaDefinition = serde_yaml::from_str(&yaml).expect("deserialize");
            assert_eq!(schema.name, back.name);
            assert_eq!(schema.version, back.version);
        }

        // Check we have all expected types
        let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"project"));
        assert!(names.contains(&"meeting"));
        assert!(names.contains(&"decision"));
        assert!(names.contains(&"signal"));
    }

    #[test]
    fn validate_decision_schema() {
        let schema = decision_schema();
        let mut fields = HashMap::new();
        fields.insert(
            "decision".to_string(),
            serde_json::json!("Use Rust for the core"),
        );

        let result = schema.validate("decision", &fields);
        assert!(result.is_valid());
    }

    #[test]
    fn validate_decision_rejects_missing_decision_field() {
        let schema = decision_schema();
        let fields = HashMap::new();

        let result = schema.validate("decision", &fields);
        assert!(!result.is_valid());
    }
}
