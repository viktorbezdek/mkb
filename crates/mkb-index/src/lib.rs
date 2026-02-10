//! # mkb-index
//!
//! SQLite indexer with FTS5 full-text search for MKB.
//!
//! Maintains a derived index from vault markdown files:
//! - Documents table for structured field queries
//! - FTS5 virtual table for full-text content search
//! - Temporal columns for time-based queries

use std::path::Path;

use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::{params, types::Value as SqlValue, Connection};
use sqlite_vec::sqlite3_vec_init;
use zerocopy::AsBytes;

use mkb_core::document::Document;
use mkb_core::error::MkbError;

/// Embedding dimension for text-embedding-3-small (OpenAI).
pub const EMBEDDING_DIM: usize = 1536;

/// Register sqlite-vec extension globally. Safe to call multiple times.
fn ensure_vec_extension() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    });
}

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
        ensure_vec_extension();
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
        ensure_vec_extension();
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

            CREATE TABLE IF NOT EXISTS links (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                rel TEXT NOT NULL,
                observed_at TEXT NOT NULL,
                metadata TEXT,
                FOREIGN KEY (source_id) REFERENCES documents(id)
            );

            CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_id);
            CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_id);
            CREATE INDEX IF NOT EXISTS idx_links_rel ON links(rel);

            CREATE TABLE IF NOT EXISTS document_embeddings (
                id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                model TEXT NOT NULL DEFAULT 'text-embedding-3-small',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (id) REFERENCES documents(id) ON DELETE CASCADE
            );
            ",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        // Create virtual vec0 table for vector search (sqlite-vec).
        // This is idempotent — sqlite-vec handles IF NOT EXISTS internally.
        self.conn
            .execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS vec_documents USING vec0(
                    id TEXT PRIMARY KEY,
                    embedding float[{EMBEDDING_DIM}]
                );"
            ))
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

    /// Store links for a document. Replaces any existing links for the source.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the insert fails.
    pub fn store_links(
        &self,
        source_id: &str,
        links: &[mkb_core::link::Link],
    ) -> Result<(), MkbError> {
        // Remove existing links for this source
        self.conn
            .execute("DELETE FROM links WHERE source_id = ?1", params![source_id])
            .map_err(|e| MkbError::Index(e.to_string()))?;

        for link in links {
            self.conn
                .execute(
                    "INSERT INTO links (source_id, target_id, rel, observed_at, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        source_id,
                        link.target,
                        link.rel,
                        link.observed_at.to_rfc3339(),
                        link.metadata
                            .as_ref()
                            .map(|m| serde_json::to_string(m).unwrap_or_default()),
                    ],
                )
                .map_err(|e| MkbError::Index(e.to_string()))?;
        }
        Ok(())
    }

    /// Query forward links from a source document.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn query_forward_links(&self, source_id: &str) -> Result<Vec<IndexedLink>, MkbError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT source_id, target_id, rel, observed_at FROM links
                 WHERE source_id = ?1
                 ORDER BY rel, observed_at",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        let results = stmt
            .query_map(params![source_id], |row| {
                Ok(IndexedLink {
                    source_id: row.get(0)?,
                    target_id: row.get(1)?,
                    rel: row.get(2)?,
                    observed_at: row.get(3)?,
                })
            })
            .map_err(|e| MkbError::Index(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| MkbError::Index(e.to_string()))?;

        Ok(results)
    }

    /// Query reverse links pointing to a target document.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn query_reverse_links(&self, target_id: &str) -> Result<Vec<IndexedLink>, MkbError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT source_id, target_id, rel, observed_at FROM links
                 WHERE target_id = ?1
                 ORDER BY rel, observed_at",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        let results = stmt
            .query_map(params![target_id], |row| {
                Ok(IndexedLink {
                    source_id: row.get(0)?,
                    target_id: row.get(1)?,
                    rel: row.get(2)?,
                    observed_at: row.get(3)?,
                })
            })
            .map_err(|e| MkbError::Index(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| MkbError::Index(e.to_string()))?;

        Ok(results)
    }

    /// Query documents by observed_at range.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn query_by_observed_at_range(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<IndexedDocument>, MkbError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, doc_type, title, observed_at, valid_until, confidence
                 FROM documents
                 WHERE observed_at >= ?1 AND observed_at <= ?2
                 ORDER BY observed_at DESC",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        let results = stmt
            .query_map(params![from, to], |row| {
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

    /// Query current documents: not superseded and not expired at the given time.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn query_current_documents(&self, at_time: &str) -> Result<Vec<IndexedDocument>, MkbError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, doc_type, title, observed_at, valid_until, confidence
                 FROM documents
                 WHERE superseded_by IS NULL
                   AND valid_until >= ?1
                 ORDER BY observed_at DESC",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        let results = stmt
            .query_map(params![at_time], |row| {
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

    /// Mark expired documents by returning their IDs.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn staleness_sweep(&self, at_time: &str) -> Result<Vec<String>, MkbError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id FROM documents
                 WHERE valid_until < ?1
                   AND superseded_by IS NULL
                 ORDER BY valid_until ASC",
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;

        let results = stmt
            .query_map(params![at_time], |row| row.get(0))
            .map_err(|e| MkbError::Index(e.to_string()))?
            .collect::<std::result::Result<Vec<String>, _>>()
            .map_err(|e| MkbError::Index(e.to_string()))?;

        Ok(results)
    }

    /// Execute a raw SQL query with parameters, returning rows as JSON-like maps.
    ///
    /// Used by the query engine to execute compiled MKQL queries.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn execute_sql(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>, MkbError> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| MkbError::Index(format!("SQL prepare error: {e}")))?;

        let column_count = stmt.column_count();
        let column_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let mut map = std::collections::HashMap::new();
                for (i, name) in column_names.iter().enumerate() {
                    let value: SqlValue = row.get(i)?;
                    let json_val = match value {
                        SqlValue::Null => serde_json::Value::Null,
                        SqlValue::Integer(n) => serde_json::json!(n),
                        SqlValue::Real(f) => serde_json::json!(f),
                        SqlValue::Text(s) => serde_json::json!(s),
                        SqlValue::Blob(b) => {
                            serde_json::json!(format!("<blob:{} bytes>", b.len()))
                        }
                    };
                    map.insert(name.clone(), json_val);
                }
                Ok(map)
            })
            .map_err(|e| MkbError::Index(format!("SQL query error: {e}")))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| MkbError::Index(format!("SQL row error: {e}")))?;

        Ok(rows)
    }

    // === Vector / Embedding Operations ===

    /// Store an embedding vector for a document.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the insert fails.
    pub fn store_embedding(
        &self,
        doc_id: &str,
        embedding: &[f32],
        model: &str,
    ) -> Result<(), MkbError> {
        if embedding.len() != EMBEDDING_DIM {
            return Err(MkbError::Index(format!(
                "Embedding dimension mismatch: expected {EMBEDDING_DIM}, got {}",
                embedding.len()
            )));
        }

        let blob = embedding.as_bytes();

        // Store raw embedding in document_embeddings table
        self.conn
            .execute(
                "INSERT OR REPLACE INTO document_embeddings (id, embedding, model)
                 VALUES (?1, ?2, ?3)",
                params![doc_id, blob, model],
            )
            .map_err(|e| MkbError::Index(format!("Store embedding failed: {e}")))?;

        // Insert into vec0 virtual table for vector search
        self.conn
            .execute(
                "INSERT OR REPLACE INTO vec_documents (id, embedding)
                 VALUES (?1, ?2)",
                params![doc_id, blob],
            )
            .map_err(|e| MkbError::Index(format!("Vec index insert failed: {e}")))?;

        Ok(())
    }

    /// Search for similar documents using vector similarity (KNN).
    ///
    /// Returns document IDs with their distance scores, ordered by similarity.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn search_semantic(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorSearchResult>, MkbError> {
        if query_embedding.len() != EMBEDDING_DIM {
            return Err(MkbError::Index(format!(
                "Query embedding dimension mismatch: expected {EMBEDDING_DIM}, got {}",
                query_embedding.len()
            )));
        }

        let blob = query_embedding.as_bytes();

        let mut stmt = self
            .conn
            .prepare(
                "SELECT v.id, v.distance, d.title, d.doc_type
                 FROM vec_documents v
                 JOIN documents d ON d.id = v.id
                 WHERE v.embedding MATCH ?1
                   AND k = ?2
                 ORDER BY v.distance",
            )
            .map_err(|e| MkbError::Index(format!("Vec search prepare failed: {e}")))?;

        let results = stmt
            .query_map(params![blob, limit as i64], |row| {
                Ok(VectorSearchResult {
                    id: row.get(0)?,
                    distance: row.get(1)?,
                    title: row.get(2)?,
                    doc_type: row.get(3)?,
                })
            })
            .map_err(|e| MkbError::Index(format!("Vec search query failed: {e}")))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| MkbError::Index(format!("Vec search row failed: {e}")))?;

        Ok(results)
    }

    /// Check if a document has an embedding stored.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn has_embedding(&self, doc_id: &str) -> Result<bool, MkbError> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM document_embeddings WHERE id = ?1",
                params![doc_id],
                |row| row.get(0),
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;
        Ok(count > 0)
    }

    /// Remove embedding for a document.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the delete fails.
    pub fn remove_embedding(&self, doc_id: &str) -> Result<(), MkbError> {
        self.conn
            .execute(
                "DELETE FROM document_embeddings WHERE id = ?1",
                params![doc_id],
            )
            .map_err(|e| MkbError::Index(e.to_string()))?;
        self.conn
            .execute("DELETE FROM vec_documents WHERE id = ?1", params![doc_id])
            .map_err(|e| MkbError::Index(e.to_string()))?;
        Ok(())
    }

    /// Count documents with embeddings.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Index`] if the query fails.
    pub fn embedding_count(&self) -> Result<u64, MkbError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM document_embeddings", [], |row| {
                row.get(0)
            })
            .map_err(|e| MkbError::Index(e.to_string()))?;
        Ok(count as u64)
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

/// A link as stored in the index.
#[derive(Debug, Clone)]
pub struct IndexedLink {
    pub source_id: String,
    pub target_id: String,
    pub rel: String,
    pub observed_at: String,
}

/// A vector search result with distance score.
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    pub id: String,
    pub distance: f64,
    pub title: String,
    pub doc_type: String,
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

    // === T-110.3 tests: link indexing ===

    #[test]
    fn link_creation_with_timestamp() {
        let link = mkb_core::link::Link {
            rel: "owner".to_string(),
            target: "people/jane-smith".to_string(),
            observed_at: utc(2025, 2, 10),
            metadata: None,
        };
        assert_eq!(link.rel, "owner");
        assert_eq!(link.observed_at, utc(2025, 2, 10));
    }

    #[test]
    fn store_and_retrieve_links() {
        let mgr = IndexManager::in_memory().unwrap();
        let doc = make_doc("proj-alpha-001", "project", "Alpha", "body");
        mgr.index_document(&doc).unwrap();

        let links = vec![
            mkb_core::link::Link {
                rel: "owner".to_string(),
                target: "people/jane-smith".to_string(),
                observed_at: utc(2025, 2, 10),
                metadata: None,
            },
            mkb_core::link::Link {
                rel: "blocked_by".to_string(),
                target: "proj-beta-001".to_string(),
                observed_at: utc(2025, 2, 10),
                metadata: None,
            },
        ];

        mgr.store_links("proj-alpha-001", &links).unwrap();
        let forward = mgr.query_forward_links("proj-alpha-001").unwrap();
        assert_eq!(forward.len(), 2);
    }

    #[test]
    fn query_forward_links() {
        let mgr = IndexManager::in_memory().unwrap();
        let doc = make_doc("proj-alpha-001", "project", "Alpha", "body");
        mgr.index_document(&doc).unwrap();

        let links = vec![mkb_core::link::Link {
            rel: "owner".to_string(),
            target: "people/jane-smith".to_string(),
            observed_at: utc(2025, 2, 10),
            metadata: None,
        }];
        mgr.store_links("proj-alpha-001", &links).unwrap();

        let forward = mgr.query_forward_links("proj-alpha-001").unwrap();
        assert_eq!(forward.len(), 1);
        assert_eq!(forward[0].target_id, "people/jane-smith");
        assert_eq!(forward[0].rel, "owner");
    }

    #[test]
    fn query_reverse_links() {
        let mgr = IndexManager::in_memory().unwrap();

        let doc1 = make_doc("proj-alpha-001", "project", "Alpha", "body");
        mgr.index_document(&doc1).unwrap();
        let doc2 = make_doc("proj-beta-001", "project", "Beta", "body");
        mgr.index_document(&doc2).unwrap();

        // Both projects link to same person
        mgr.store_links(
            "proj-alpha-001",
            &[mkb_core::link::Link {
                rel: "owner".to_string(),
                target: "people/jane-smith".to_string(),
                observed_at: utc(2025, 2, 10),
                metadata: None,
            }],
        )
        .unwrap();
        mgr.store_links(
            "proj-beta-001",
            &[mkb_core::link::Link {
                rel: "owner".to_string(),
                target: "people/jane-smith".to_string(),
                observed_at: utc(2025, 2, 10),
                metadata: None,
            }],
        )
        .unwrap();

        let reverse = mgr.query_reverse_links("people/jane-smith").unwrap();
        assert_eq!(reverse.len(), 2);
        let sources: Vec<&str> = reverse.iter().map(|l| l.source_id.as_str()).collect();
        assert!(sources.contains(&"proj-alpha-001"));
        assert!(sources.contains(&"proj-beta-001"));
    }

    // === T-110.4 tests: temporal queries ===

    #[test]
    fn query_by_observed_at_range() {
        let mgr = IndexManager::in_memory().unwrap();

        // Doc observed in January
        let d1 = make_doc("d1", "project", "January Doc", "body1");
        mgr.index_document(&d1).unwrap();

        // Doc observed in March (create with different observed_at)
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 3, 15)),
            valid_until: Some(utc(2025, 9, 15)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();
        let mut d2 = Document::new(
            "d2".into(),
            "project".into(),
            "March Doc".into(),
            input,
            &profile,
        )
        .unwrap();
        d2.body = "body2".into();
        mgr.index_document(&d2).unwrap();

        // Query range that only includes February (from Feb 1 to Feb 28)
        let results = mgr
            .query_by_observed_at_range("2025-02-01T00:00:00+00:00", "2025-02-28T23:59:59+00:00")
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "d1");

        // Query range that includes both
        let results = mgr
            .query_by_observed_at_range("2025-01-01T00:00:00+00:00", "2025-12-31T23:59:59+00:00")
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_current_documents() {
        let mgr = IndexManager::in_memory().unwrap();

        // Active document (valid until Aug 2025)
        let d1 = make_doc("d1", "project", "Active", "body1");
        mgr.index_document(&d1).unwrap();

        // Expired document (valid until Jan 2025, before our query time)
        let input = RawTemporalInput {
            observed_at: Some(utc(2024, 6, 1)),
            valid_until: Some(utc(2025, 1, 1)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();
        let mut d2 = Document::new(
            "d2".into(),
            "project".into(),
            "Expired".into(),
            input,
            &profile,
        )
        .unwrap();
        d2.body = "body2".into();
        mgr.index_document(&d2).unwrap();

        // Superseded document
        let mut d3 = make_doc("d3", "project", "Superseded", "body3");
        d3.superseded_by = Some("d1".to_string());
        mgr.index_document(&d3).unwrap();

        // Query current at Feb 2025: should only return d1
        let current = mgr
            .query_current_documents("2025-02-15T00:00:00+00:00")
            .unwrap();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].id, "d1");
    }

    #[test]
    fn query_with_effective_confidence() {
        let mgr = IndexManager::in_memory().unwrap();

        // High-confidence recent doc
        let mut d1 = make_doc("d1", "project", "Recent", "body1");
        d1.confidence = 0.95;
        mgr.index_document(&d1).unwrap();

        // Low-confidence old doc
        let input = RawTemporalInput {
            observed_at: Some(utc(2024, 1, 1)),
            valid_until: Some(utc(2026, 1, 1)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();
        let mut d2 =
            Document::new("d2".into(), "project".into(), "Old".into(), input, &profile).unwrap();
        d2.body = "body2".into();
        d2.confidence = 0.5;
        mgr.index_document(&d2).unwrap();

        // Query all and check confidence values are retrievable
        let all = mgr.query_all().unwrap();
        assert_eq!(all.len(), 2);

        let recent = all.iter().find(|d| d.id == "d1").unwrap();
        assert!((recent.confidence - 0.95).abs() < f64::EPSILON);

        let old = all.iter().find(|d| d.id == "d2").unwrap();
        assert!((old.confidence - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn staleness_sweep_marks_expired() {
        let mgr = IndexManager::in_memory().unwrap();

        // Doc valid until June 2025
        let d1 = make_doc("d1", "project", "Valid", "body1");
        mgr.index_document(&d1).unwrap();

        // Doc valid until Jan 2025 (expired)
        let input = RawTemporalInput {
            observed_at: Some(utc(2024, 6, 1)),
            valid_until: Some(utc(2025, 1, 1)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();
        let mut d2 = Document::new(
            "d2".into(),
            "project".into(),
            "Expired".into(),
            input,
            &profile,
        )
        .unwrap();
        d2.body = "body2".into();
        mgr.index_document(&d2).unwrap();

        // Sweep at Feb 2025
        let stale = mgr.staleness_sweep("2025-02-15T00:00:00+00:00").unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0], "d2");
    }

    // === T-410.2 tests: sqlite-vec vector operations ===

    /// Generate a deterministic test embedding from a seed string.
    fn test_embedding(seed: &str) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut vec = vec![0.0f32; EMBEDDING_DIM];
        for (i, v) in vec.iter_mut().enumerate() {
            let mut h = DefaultHasher::new();
            seed.hash(&mut h);
            i.hash(&mut h);
            *v = (h.finish() as f32 / u64::MAX as f32) * 2.0 - 1.0;
        }
        // Normalize to unit vector
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        for v in &mut vec {
            *v /= norm;
        }
        vec
    }

    #[test]
    fn store_and_query_embedding() {
        let mgr = IndexManager::in_memory().unwrap();

        let doc = make_doc("d1", "project", "Alpha", "body");
        mgr.index_document(&doc).unwrap();

        let emb = test_embedding("alpha");
        mgr.store_embedding("d1", &emb, "test-model").unwrap();

        assert!(mgr.has_embedding("d1").unwrap());
        assert!(!mgr.has_embedding("d2").unwrap());
        assert_eq!(mgr.embedding_count().unwrap(), 1);
    }

    #[test]
    fn semantic_search_returns_similar_documents() {
        let mgr = IndexManager::in_memory().unwrap();

        // Create 3 documents with different embeddings
        for (id, doc_type, title) in &[
            ("d1", "project", "Alpha Project"),
            ("d2", "project", "Beta Project"),
            ("d3", "meeting", "Standup Meeting"),
        ] {
            let doc = make_doc(id, doc_type, title, "body");
            mgr.index_document(&doc).unwrap();
            mgr.store_embedding(id, &test_embedding(id), "test-model")
                .unwrap();
        }

        // Query with the same embedding as d1 — should return d1 first
        let results = mgr.search_semantic(&test_embedding("d1"), 3).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].id, "d1");
        assert!(results[0].distance < results[1].distance);
    }

    #[test]
    fn embedding_dimension_mismatch_rejected() {
        let mgr = IndexManager::in_memory().unwrap();

        let doc = make_doc("d1", "project", "Alpha", "body");
        mgr.index_document(&doc).unwrap();

        let wrong_dim = vec![0.0f32; 768]; // Wrong dimension
        let result = mgr.store_embedding("d1", &wrong_dim, "test-model");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("dimension mismatch"));
    }

    #[test]
    fn remove_embedding_works() {
        let mgr = IndexManager::in_memory().unwrap();

        let doc = make_doc("d1", "project", "Alpha", "body");
        mgr.index_document(&doc).unwrap();
        mgr.store_embedding("d1", &test_embedding("d1"), "test-model")
            .unwrap();

        assert!(mgr.has_embedding("d1").unwrap());
        mgr.remove_embedding("d1").unwrap();
        assert!(!mgr.has_embedding("d1").unwrap());
        assert_eq!(mgr.embedding_count().unwrap(), 0);
    }

    #[test]
    fn persist_and_reload_index() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");

        // Create and populate
        {
            let mgr = IndexManager::open(&db_path).unwrap();
            let doc = make_doc("d1", "project", "Alpha", "body");
            mgr.index_document(&doc).unwrap();
            mgr.store_embedding("d1", &test_embedding("d1"), "test-model")
                .unwrap();
        }

        // Reopen and verify
        {
            let mgr = IndexManager::open(&db_path).unwrap();
            assert_eq!(mgr.count().unwrap(), 1);
            assert!(mgr.has_embedding("d1").unwrap());

            let results = mgr.search_semantic(&test_embedding("d1"), 1).unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "d1");
        }
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
