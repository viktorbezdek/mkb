//! Result formatting: JSON, Table, and Markdown output.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Output format for query results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Table,
    Markdown,
}

/// A single row in a query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultRow {
    pub fields: HashMap<String, serde_json::Value>,
}

/// A complete query result set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub rows: Vec<ResultRow>,
    pub total: usize,
}

/// Format query results in the specified output format.
#[must_use]
pub fn format_results(result: &QueryResult, format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => format_json(result),
        OutputFormat::Table => format_table(result),
        OutputFormat::Markdown => format_markdown(result),
    }
}

fn format_json(result: &QueryResult) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|_| "[]".to_string())
}

fn format_table(result: &QueryResult) -> String {
    if result.rows.is_empty() {
        return "(no results)".to_string();
    }

    // Collect all column names from the first row
    let columns: Vec<String> = {
        let mut cols: Vec<String> = result.rows[0].fields.keys().cloned().collect();
        cols.sort();
        cols
    };

    // Calculate column widths
    let mut widths: Vec<usize> = columns.iter().map(|c| c.len()).collect();
    for row in &result.rows {
        for (i, col) in columns.iter().enumerate() {
            let val_len = row.fields.get(col).map(value_display_len).unwrap_or(4); // "null"
            widths[i] = widths[i].max(val_len);
        }
    }

    let mut output = String::new();

    // Header
    let header: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{:width$}", c, width = widths[i]))
        .collect();
    output.push_str(&header.join(" | "));
    output.push('\n');

    // Separator
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    output.push_str(&sep.join("-+-"));
    output.push('\n');

    // Rows
    for row in &result.rows {
        let vals: Vec<String> = columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let val = row
                    .fields
                    .get(col)
                    .map(value_to_display)
                    .unwrap_or_else(|| "null".to_string());
                format!("{:width$}", val, width = widths[i])
            })
            .collect();
        output.push_str(&vals.join(" | "));
        output.push('\n');
    }

    output
}

fn format_markdown(result: &QueryResult) -> String {
    if result.rows.is_empty() {
        return "*No results*\n".to_string();
    }

    let columns: Vec<String> = {
        let mut cols: Vec<String> = result.rows[0].fields.keys().cloned().collect();
        cols.sort();
        cols
    };

    let mut output = String::new();

    // Header
    output.push_str("| ");
    output.push_str(&columns.join(" | "));
    output.push_str(" |\n");

    // Separator
    output.push_str("| ");
    let seps: Vec<&str> = columns.iter().map(|_| "---").collect();
    output.push_str(&seps.join(" | "));
    output.push_str(" |\n");

    // Rows
    for row in &result.rows {
        output.push_str("| ");
        let vals: Vec<String> = columns
            .iter()
            .map(|col| {
                row.fields
                    .get(col)
                    .map(value_to_display)
                    .unwrap_or_else(|| "null".to_string())
            })
            .collect();
        output.push_str(&vals.join(" | "));
        output.push_str(" |\n");
    }

    output
}

fn value_to_display(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn value_display_len(v: &serde_json::Value) -> usize {
    value_to_display(v).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> QueryResult {
        let mut row1 = HashMap::new();
        row1.insert("id".to_string(), serde_json::json!("proj-alpha-001"));
        row1.insert("title".to_string(), serde_json::json!("Alpha Project"));
        row1.insert("status".to_string(), serde_json::json!("active"));

        let mut row2 = HashMap::new();
        row2.insert("id".to_string(), serde_json::json!("proj-beta-001"));
        row2.insert("title".to_string(), serde_json::json!("Beta Project"));
        row2.insert("status".to_string(), serde_json::json!("paused"));

        QueryResult {
            rows: vec![ResultRow { fields: row1 }, ResultRow { fields: row2 }],
            total: 2,
        }
    }

    #[test]
    fn format_as_json() {
        let result = sample_result();
        let output = format_results(&result, OutputFormat::Json);
        assert!(output.contains("proj-alpha-001"));
        assert!(output.contains("Alpha Project"));
        assert!(output.contains("proj-beta-001"));
        // Valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["total"], 2);
    }

    #[test]
    fn format_as_table() {
        let result = sample_result();
        let output = format_results(&result, OutputFormat::Table);
        assert!(output.contains("id"));
        assert!(output.contains("title"));
        assert!(output.contains("status"));
        assert!(output.contains("proj-alpha-001"));
        assert!(output.contains("---"));
    }

    #[test]
    fn format_as_markdown() {
        let result = sample_result();
        let output = format_results(&result, OutputFormat::Markdown);
        assert!(output.contains("| id"));
        assert!(output.contains("| ---"));
        assert!(output.contains("proj-alpha-001"));
        assert!(output.contains("|\n"));
    }

    #[test]
    fn format_empty_result() {
        let result = QueryResult {
            rows: vec![],
            total: 0,
        };
        assert_eq!(format_results(&result, OutputFormat::Table), "(no results)");
        assert_eq!(
            format_results(&result, OutputFormat::Markdown),
            "*No results*\n"
        );
    }
}
