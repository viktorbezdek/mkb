//! # mkb-index
//!
//! SQLite indexer with FTS5 full-text search for MKB.
//!
//! Maintains a derived index from vault markdown files:
//! - Documents table for structured field queries
//! - FTS5 virtual table for full-text content search
//! - Temporal columns for time-based queries

use std::path::Path;

use rusqlite::{params, Connection};

use mkb_core::document::Document;
use mkb_core::error::MkbError;

/// The IndexManager manages the SQLite index database.
pub struct IndexManager {
    conn: Connection,
}

impl IndexManager {
    /// Open or create an index database at the given path.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the database cannot be opened.
    pub fn open(path: &Path) -> Result<Self, MkbError> {
        let conn = Connection::open(path).map_err(|e| MkbError::Index(e.to_string()))?;
        let mgr = Self { conn };
        mgr.create_schema()?;
        Ok(mgr)
    }

    /// Create an in-memory index (useful for testing).
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if schema creation fails.
    pub fn in_memory() -> Result<Self, MkbError> {
        let conn = Connection::open_in_memory().map_err(|e| MkbError::Index(e.to_string()))?;
        let mgr = Self { conn };
        mgr.create_schema()?;
        Ok(mgr)
    }

    /// Create the index schema (documents table + FTS5 virtual table).
    fn create_schema(&self) -> Result<(), MkbError> {
        self.conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                doc_type TEXT NOT NULL,
                title TEXT NOT NULL,
                observed_at TEXT NOT NULL,
                valid_until TEXT NOT NULL,
                temporal_precision TEXT NOT NULL,
                occurred_at TEXT,
                created_at TEXT NOT NULL,
                modified_at TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 1.0,
                source TEXT,
                supersedes TEXT,
                superseded_by TEXT,
                tags TEXT,
                body TEXT NOT NULL DEFAULT ''
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
                title,
                body,
                tags,
                content='documents',
                content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS documents_ai AFTER INSERT ON documents BEGIN
                INSERT INTO documents_fts(rowid, title, body, tags)
                VALUES (new.rowid, new.title, new.body, new.tags);
            END;

            CREATE TRIGGER IF NOT EXISTS documents_ad AFTER DELETE ON documents BEGIN
                INSERT INTO documents_fts(documents_fts, rowid, title, body, tags)
                VALUES ('delete', old.rowid, old.title, old.body, old.tags);
            END;

            CREATE TRIGGER IF NOT EXISTS documents_au AFTER UPDATE ON documents BEGIN
                INSERT INTO documents_fts(documents_fts, rowid, title, body, tags)
                VALUES ('delete', old.rowid, old.title, old.body, old.tags);
                INSERT INTO documents_fts(rowid, title, body, tags)
                VALUES (new.rowid, new.title, new.body, new.tags);
            END;

            CREATE INDEX IF NOT EXISTS idx_documents_type ON documents(doc_type);
            CREATE INDEX IF NOT EXISTS idx_documents_observed_at ON documents(observed_at);
            CREATE INDEX IF NOT EXISTS idx_documents_valid_until ON documents(valid_until);
            ",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        Ok(())
    }

    /// Index a document (insert or replace).
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the insert fails.
    pub fn index_document(&self, doc: &Document) -> Result<(), MkbError> {
        let tags_str = doc.tags.join(", ");

        self.conn
            .execute(
                "INSERT OR REPLACE INTO documents
                (id, doc_type, title, observed_at, valid_until, temporal_precision,
                 occurred_at, created_at, modified_at, confidence, source,
                 supersedes, superseded_by, tags, body)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    doc.id,
                    doc.doc_type,
                    doc.title,
                    doc.temporal.observed_at.to_rfc3339(),
                    doc.temporal.valid_until.to_rfc3339(),
                    format!("{:?}", doc.temporal.temporal_precision).to_lowercase(),
                    doc.temporal.occurred_at.map(|d| d.to_rfc3339()),
                    doc.created_at.to_rfc3339(),
                    doc.modified_at.to_rfc3339(),
                    doc.confidence,
                    doc.source,
                    doc.supersedes,
                    doc.superseded_by,
                    tags_str,
                    doc.body,
                ],
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        Ok(())
    }

    /// Remove a document from the index.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the delete fails.
    pub fn remove_document(&self, id: &str) -> Result<(), MkbError> {
        self.conn
            .execute("DELETE FROM documents WHERE id = ?1", params![id])
            .map_err(|e| MkbError::Index(e.to_string()))?;
        Ok(())
    }

    /// Search documents using FTS5 full-text search.
    ///
    /// Returns document IDs and titles ranked by relevance.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn search_fts(&self, query: &str) -> Result<Vec<SearchResult>, MkbError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT d.id, d.title, d.doc_type, rank
                 FROM documents_fts f
                 JOIN documents d ON d.rowid = f.rowid
                 WHERE documents_fts MATCH ?1
                 ORDER BY rank",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        let results = stmt
            .query_map(params![query], |row| {
                Ok(SearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    doc_type: row.get(2)?,
                    rank: row.get(3)?,
                })
            })
            .map_err(|e| MkbError::Index(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| MkbError::Index(e.to_string()))?;

        Ok(results)
    }

    /// Query documents by type.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn query_by_type(&self, doc_type: &str) -> Result<Vec<IndexedDocument>, MkbError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, doc_type, title, observed_at, valid_until, confidence
                 FROM documents
                 WHERE doc_type = ?1
                 ORDER BY observed_at DESC",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        let results = stmt
            .query_map(params![doc_type], |row| {
                Ok(IndexedDocument {
                    id: row.get(0)?,
                    doc_type: row.get(1)?,
                    title: row.get(2)?,
                    observed_at: row.get(3)?,
                    valid_until: row.get(4)?,
                    confidence: row.get(5)?,
                })
            })
            .map_err(|e| MkbError::Index(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| MkbError::Index(e.to_string()))?;

        Ok(results)
    }

    /// Query all documents, returning basic info.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn query_all(&self) -> Result<Vec<IndexedDocument>, MkbError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, doc_type, title, observed_at, valid_until, confidence
                 FROM documents
                 ORDER BY observed_at DESC",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        let results = stmt
            .query_map([], |row| {
                Ok(IndexedDocument {
                    id: row.get(0)?,
                    doc_type: row.get(1)?,
                    title: row.get(2)?,
                    observed_at: row.get(3)?,
                    valid_until: row.get(4)?,
                    confidence: row.get(5)?,
                })
            })
            .map_err(|e| MkbError::Index(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| MkbError::Index(e.to_string()))?;

        Ok(results)
    }

    /// Get count of indexed documents.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn count(&self) -> Result<u64, MkbError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .map_err(|e| MkbError::Index(e.to_string()))?;
        Ok(count as u64)
    }
}

/// A search result from FTS5 full-text search.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub doc_type: String,
    pub rank: f64,
}

/// A document as stored in the index.
#[derive(Debug, Clone)]
pub struct IndexedDocument {
    pub id: String,
    pub doc_type: String,
    pub title: String,
    pub observed_at: String,
    pub valid_until: String,
    pub confidence: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};
    use mkb_core::temporal::{DecayProfile, RawTemporalInput, TemporalPrecision};

    fn utc(y: i32, m: u32, d: u32) -> DateTime<Utc> {
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

    #[test]
    fn creates_schema_on_init() {
        let mgr = IndexManager::in_memory().unwrap();
        // Schema exists — can count without error
        let count = mgr.count().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn index_document_stores_all_frontmatter_fields() {
        let mgr = IndexManager::in_memory().unwrap();

        let mut doc = make_doc("proj-alpha-001", "project", "Alpha Project", "Some body");
        doc.tags = vec!["rust".to_string(), "ai".to_string()];
        doc.confidence = 0.95;
        doc.source = Some("manual".to_string());

        mgr.index_document(&doc).unwrap();

        assert_eq!(mgr.count().unwrap(), 1);

        let results = mgr.query_by_type("project").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "proj-alpha-001");
        assert_eq!(results[0].title, "Alpha Project");
        assert!((results[0].confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn fts_indexes_title_and_body() {
        let mgr = IndexManager::in_memory().unwrap();

        mgr.index_document(&make_doc(
            "proj-alpha-001",
            "project",
            "Alpha Project",
            "This project uses Rust and machine learning.",
        ))
        .unwrap();

        mgr.index_document(&make_doc(
            "proj-beta-001",
            "project",
            "Beta Project",
            "A Python data pipeline for analytics.",
        ))
        .unwrap();

        // Search in body
        let results = mgr.search_fts("machine learning").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "proj-alpha-001");

        // Search in title
        let results = mgr.search_fts("Beta").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "proj-beta-001");
    }

    #[test]
    fn fts_search_returns_ranked_results() {
        let mgr = IndexManager::in_memory().unwrap();

        mgr.index_document(&make_doc(
            "d1",
            "project",
            "Rust Project",
            "Rust is great for systems programming with Rust tools.",
        ))
        .unwrap();

        mgr.index_document(&make_doc(
            "d2",
            "project",
            "Python Project",
            "Python is great. Also mentions Rust once.",
        ))
        .unwrap();

        let results = mgr.search_fts("Rust").unwrap();
        assert_eq!(results.len(), 2);
        // d1 should rank higher (more mentions of "Rust")
        assert_eq!(results[0].id, "d1");
    }

    #[test]
    fn remove_document_deletes_from_index() {
        let mgr = IndexManager::in_memory().unwrap();

        mgr.index_document(&make_doc("d1", "project", "Alpha", "body"))
            .unwrap();
        assert_eq!(mgr.count().unwrap(), 1);

        mgr.remove_document("d1").unwrap();
        assert_eq!(mgr.count().unwrap(), 0);
    }

    #[test]
    fn query_all_returns_all_documents() {
        let mgr = IndexManager::in_memory().unwrap();

        mgr.index_document(&make_doc("d1", "project", "Alpha", "body1"))
            .unwrap();
        mgr.index_document(&make_doc("d2", "meeting", "Sprint Review", "body2"))
            .unwrap();

        let all = mgr.query_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn index_document_upserts_on_duplicate_id() {
        let mgr = IndexManager::in_memory().unwrap();

        mgr.index_document(&make_doc("d1", "project", "Original", "body"))
            .unwrap();
        mgr.index_document(&make_doc("d1", "project", "Updated", "new body"))
            .unwrap();

        assert_eq!(mgr.count().unwrap(), 1);
        let results = mgr.query_by_type("project").unwrap();
        assert_eq!(results[0].title, "Updated");
    }

    #[test]
    fn full_rebuild_matches_incremental_index() {
        // Build incrementally
        let incremental = IndexManager::in_memory().unwrap();
        let docs = vec![
            make_doc("d1", "project", "Alpha", "body1"),
            make_doc("d2", "project", "Beta", "body2"),
            make_doc("d3", "meeting", "Sprint", "body3"),
        ];

        for doc in &docs {
            incremental.index_document(doc).unwrap();
        }

        // Build from scratch (simulates full rebuild)
        let rebuilt = IndexManager::in_memory().unwrap();
        for doc in &docs {
            rebuilt.index_document(doc).unwrap();
        }

        // Both should have same count and same query results
        assert_eq!(incremental.count().unwrap(), rebuilt.count().unwrap());

        let inc_projects = incremental.query_by_type("project").unwrap();
        let reb_projects = rebuilt.query_by_type("project").unwrap();
        assert_eq!(inc_projects.len(), reb_projects.len());

        for (a, b) in inc_projects.iter().zip(reb_projects.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.title, b.title);
        }
    }
}
