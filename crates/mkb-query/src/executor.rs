//! Query executor: bridges compiled MKQL queries to SQLite execution.
//!
//! Takes a `CompiledQuery` and an `IndexManager`, executes the SQL,
//! and returns a `QueryResult`.

use mkb_index::IndexManager;
use rusqlite::types::Value as SqlValue;

use crate::compiler::{CompiledQuery, SqlParam};
use crate::formatter::{QueryResult, ResultRow};

/// Execute a compiled query against the index.
///
/// For queries with `NEAR()` predicate, uses a two-phase approach:
/// 1. Generate mock embedding, run KNN search to get candidate IDs
/// 2. Filter by distance threshold, inject matching IDs into SQL
///
/// # Errors
///
/// Returns a string error if execution fails.
pub fn execute(index: &IndexManager, compiled: &CompiledQuery) -> Result<QueryResult, String> {
    let mut sql = compiled.sql.clone();

    // Phase 1: If NEAR() is used, resolve semantic candidates first
    if compiled.uses_semantic {
        if let Some((ref query_text, threshold)) = compiled.near_params {
            let embedding = mkb_index::mock_embedding(query_text);
            // Fetch a generous number of candidates (100)
            let candidates = index
                .search_semantic(&embedding, 100)
                .map_err(|e| format!("Semantic search failed: {e}"))?;

            // Filter by distance threshold (lower distance = more similar)
            let matching_ids: Vec<String> = candidates
                .into_iter()
                .filter(|r| r.distance <= (1.0 - threshold as f64))
                .map(|r| r.id)
                .collect();

            if matching_ids.is_empty() {
                return Ok(QueryResult {
                    rows: Vec::new(),
                    total: 0,
                });
            }

            // Replace the NEAR placeholder with an ID filter
            let id_list = matching_ids
                .iter()
                .map(|id| format!("'{id}'"))
                .collect::<Vec<_>>()
                .join(", ");
            sql = sql.replace(
                "1=1 /* NEAR placeholder */",
                &format!("d.id IN ({id_list})"),
            );
        }
    }

    let sql_params: Vec<SqlValue> = compiled
        .params
        .iter()
        .map(|p| match p {
            SqlParam::Text(s) => SqlValue::Text(s.clone()),
            SqlParam::Integer(i) => SqlValue::Integer(*i),
            SqlParam::Float(f) => SqlValue::Real(*f),
            SqlParam::Null => SqlValue::Null,
        })
        .collect();

    let rows = index
        .execute_sql(&sql, &sql_params)
        .map_err(|e| format!("Query execution failed: {e}"))?;

    let total = rows.len();
    let result_rows: Vec<ResultRow> = rows
        .into_iter()
        .map(|fields| ResultRow { fields })
        .collect();

    Ok(QueryResult {
        rows: result_rows,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::compile;
    use chrono::{TimeZone, Utc};
    use mkb_core::document::Document;
    use mkb_core::temporal::{DecayProfile, RawTemporalInput, TemporalPrecision};

    fn utc(y: i32, m: u32, d: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    fn make_doc(id: &str, doc_type: &str, title: &str, body: &str) -> Document {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 2, 10)),
            valid_until: Some(utc(2025, 8, 10)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();
        let mut doc = Document::new(
            id.to_string(),
            doc_type.to_string(),
            title.to_string(),
            input,
            &profile,
        )
        .unwrap();
        doc.body = body.to_string();
        doc
    }

    fn setup_index() -> IndexManager {
        let index = IndexManager::in_memory().unwrap();
        index
            .index_document(&make_doc(
                "proj-alpha-001",
                "project",
                "Alpha Project",
                "Rust systems programming",
            ))
            .unwrap();

        let mut beta = make_doc(
            "proj-beta-001",
            "project",
            "Beta Project",
            "Python data pipeline",
        );
        beta.confidence = 0.8;
        index.index_document(&beta).unwrap();

        index
            .index_document(&make_doc(
                "meet-standup-001",
                "meeting",
                "Daily Standup",
                "Sprint review notes",
            ))
            .unwrap();
        index
    }

    #[test]
    fn execute_select_star_returns_all_type_docs() {
        let index = setup_index();
        let query = mkb_parser::parse_mkql("SELECT * FROM project").unwrap();
        let compiled = compile(&query).unwrap();
        let result = execute(&index, &compiled).unwrap();

        assert_eq!(result.total, 2);
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn execute_select_specific_fields() {
        let index = setup_index();
        let query = mkb_parser::parse_mkql("SELECT title, confidence FROM project").unwrap();
        let compiled = compile(&query).unwrap();
        let result = execute(&index, &compiled).unwrap();

        assert_eq!(result.total, 2);
        // Check that title field is present
        let titles: Vec<&str> = result
            .rows
            .iter()
            .filter_map(|r| r.fields.get("title").and_then(|v| v.as_str()))
            .collect();
        assert!(titles.contains(&"Alpha Project"));
        assert!(titles.contains(&"Beta Project"));
    }

    #[test]
    fn execute_with_where_clause() {
        let index = setup_index();
        let query =
            mkb_parser::parse_mkql("SELECT * FROM project WHERE title = 'Alpha Project'").unwrap();
        let compiled = compile(&query).unwrap();
        let result = execute(&index, &compiled).unwrap();

        assert_eq!(result.total, 1);
        let title = result.rows[0]
            .fields
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(title, "Alpha Project");
    }

    #[test]
    fn execute_fts_body_contains() {
        let index = setup_index();
        let query =
            mkb_parser::parse_mkql("SELECT * FROM project WHERE BODY CONTAINS 'Rust'").unwrap();
        let compiled = compile(&query).unwrap();
        let result = execute(&index, &compiled).unwrap();

        assert_eq!(result.total, 1);
    }

    #[test]
    fn execute_with_limit() {
        let index = setup_index();
        let query = mkb_parser::parse_mkql("SELECT * FROM project LIMIT 1").unwrap();
        let compiled = compile(&query).unwrap();
        let result = execute(&index, &compiled).unwrap();

        assert_eq!(result.total, 1);
    }

    #[test]
    fn execute_near_returns_results() {
        let index = setup_index();
        // Store embeddings for the test documents
        let emb1 = mkb_index::mock_embedding("Rust systems programming");
        index
            .store_embedding("proj-alpha-001", &emb1, "mock")
            .unwrap();

        let emb2 = mkb_index::mock_embedding("Python data pipeline");
        index
            .store_embedding("proj-beta-001", &emb2, "mock")
            .unwrap();

        // Query with NEAR - should find documents semantically
        let query = mkb_parser::parse_mkql(
            "SELECT * FROM project WHERE NEAR('Rust systems programming', 0.0)",
        )
        .unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.uses_semantic);

        let result = execute(&index, &compiled).unwrap();
        // With threshold 0.0, both should match (very permissive)
        assert!(result.total >= 1);
    }

    #[test]
    fn execute_near_with_no_embeddings_returns_empty() {
        let index = setup_index();
        // Don't store any embeddings — NEAR should return empty
        let query = mkb_parser::parse_mkql(
            "SELECT * FROM project WHERE NEAR('machine learning', 0.9)",
        )
        .unwrap();
        let compiled = compile(&query).unwrap();
        let result = execute(&index, &compiled).unwrap();
        assert_eq!(result.total, 0);
    }

    #[test]
    fn execute_no_results_for_missing_type() {
        let index = setup_index();
        let query = mkb_parser::parse_mkql("SELECT * FROM decision").unwrap();
        let compiled = compile(&query).unwrap();
        let result = execute(&index, &compiled).unwrap();

        assert_eq!(result.total, 0);
    }
}
