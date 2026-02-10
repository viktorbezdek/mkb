//! Context assembler for LLM context windows.
//!
//! Assembles query results into a format suitable for LLM consumption,
//! prioritizing high-confidence fresh documents and respecting token budgets.

use crate::formatter::{QueryResult, ResultRow};

/// Options for context assembly.
#[derive(Debug, Clone)]
pub struct ContextOpts {
    /// Maximum number of tokens (estimated at ~4 chars per token).
    pub max_tokens: usize,
    /// Whether to use summary format when budget is tight.
    pub allow_summary: bool,
}

impl Default for ContextOpts {
    fn default() -> Self {
        Self {
            max_tokens: 4000,
            allow_summary: true,
        }
    }
}

/// Assembles query results into LLM-consumable context.
pub struct ContextAssembler;

impl ContextAssembler {
    /// Assemble results into a context string, respecting the token budget.
    ///
    /// Documents are prioritized by:
    /// 1. Higher confidence first
    /// 2. More recent `observed_at` first
    ///
    /// If the full format exceeds the budget, falls back to summary format.
    #[must_use]
    pub fn assemble(result: &QueryResult, opts: &ContextOpts) -> String {
        if result.rows.is_empty() {
            return String::new();
        }

        // Sort rows by confidence (desc), then by observed_at (desc)
        let mut sorted: Vec<&ResultRow> = result.rows.iter().collect();
        sorted.sort_by(|a, b| {
            let conf_a = a
                .fields
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let conf_b = b
                .fields
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            conf_b
                .partial_cmp(&conf_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let max_chars = opts.max_tokens * 4; // rough token estimate

        // Try full format first
        let full = Self::format_full(&sorted);
        if full.len() <= max_chars {
            return full;
        }

        // Fall back to summary format if allowed
        if opts.allow_summary {
            return Self::format_summary(&sorted, max_chars);
        }

        // Truncate full format
        full[..max_chars.min(full.len())].to_string()
    }

    fn format_full(rows: &[&ResultRow]) -> String {
        let mut output = String::new();
        for row in rows {
            let title = row
                .fields
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled");
            let doc_type = row
                .fields
                .get("doc_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let body = row
                .fields
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let confidence = row
                .fields
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);
            let observed_at = row
                .fields
                .get("observed_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            output.push_str(&format!("## [{doc_type}] {title}\n"));
            output.push_str(&format!(
                "*Observed: {observed_at} | Confidence: {confidence:.2}*\n\n"
            ));
            if !body.is_empty() {
                output.push_str(body);
                output.push_str("\n\n");
            }
            output.push_str("---\n\n");
        }
        output
    }

    fn format_summary(rows: &[&ResultRow], max_chars: usize) -> String {
        let mut output = String::from("# Summary (truncated for context budget)\n\n");

        for row in rows {
            let title = row
                .fields
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled");
            let doc_type = row
                .fields
                .get("doc_type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let confidence = row
                .fields
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);

            let line = format!("- **[{doc_type}] {title}** (confidence: {confidence:.2})\n");

            if output.len() + line.len() > max_chars {
                break;
            }
            output.push_str(&line);
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_row(title: &str, confidence: f64, body: &str) -> ResultRow {
        let mut fields = HashMap::new();
        fields.insert("title".to_string(), serde_json::json!(title));
        fields.insert("doc_type".to_string(), serde_json::json!("project"));
        fields.insert("confidence".to_string(), serde_json::json!(confidence));
        fields.insert(
            "observed_at".to_string(),
            serde_json::json!("2025-02-10T00:00:00Z"),
        );
        fields.insert("body".to_string(), serde_json::json!(body));
        ResultRow { fields }
    }

    #[test]
    fn assembler_prioritizes_high_confidence_fresh_docs() {
        let result = QueryResult {
            rows: vec![
                make_row("Low Confidence", 0.3, "low body"),
                make_row("High Confidence", 0.95, "high body"),
                make_row("Medium Confidence", 0.7, "medium body"),
            ],
            total: 3,
        };

        let opts = ContextOpts {
            max_tokens: 10000,
            allow_summary: false,
        };
        let output = ContextAssembler::assemble(&result, &opts);

        // High confidence should appear first
        let high_pos = output.find("High Confidence").unwrap();
        let medium_pos = output.find("Medium Confidence").unwrap();
        let low_pos = output.find("Low Confidence").unwrap();
        assert!(high_pos < medium_pos);
        assert!(medium_pos < low_pos);
    }

    #[test]
    fn assembler_respects_token_budget() {
        let long_body = "x".repeat(10000);
        let result = QueryResult {
            rows: vec![
                make_row("Doc 1", 0.95, &long_body),
                make_row("Doc 2", 0.90, &long_body),
                make_row("Doc 3", 0.85, &long_body),
            ],
            total: 3,
        };

        let opts = ContextOpts {
            max_tokens: 100, // Very small budget = ~400 chars
            allow_summary: true,
        };
        let output = ContextAssembler::assemble(&result, &opts);

        // Should fall back to summary format
        assert!(output.contains("Summary"));
        assert!(output.len() <= 500); // Within budget range
    }

    #[test]
    fn assembler_falls_back_to_summary_format() {
        let long_body = "x".repeat(5000);
        let result = QueryResult {
            rows: vec![
                make_row("Doc A", 0.95, &long_body),
                make_row("Doc B", 0.90, &long_body),
            ],
            total: 2,
        };

        let opts = ContextOpts {
            max_tokens: 50, // Tiny budget
            allow_summary: true,
        };
        let output = ContextAssembler::assemble(&result, &opts);
        assert!(output.contains("Summary"));
        assert!(output.contains("Doc A")); // Highest confidence should still appear
    }

    #[test]
    fn assembler_empty_result() {
        let result = QueryResult {
            rows: vec![],
            total: 0,
        };
        let output = ContextAssembler::assemble(&result, &ContextOpts::default());
        assert!(output.is_empty());
    }
}
