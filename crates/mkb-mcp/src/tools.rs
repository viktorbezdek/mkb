//! MCP tool definitions for MKB vault operations (read-only).

use std::path::PathBuf;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;

use mkb_index::IndexManager;
use mkb_vault::Vault;

/// MKB MCP Server exposing read-only vault operations.
#[derive(Debug, Clone)]
pub struct MkbMcpService {
    /// Path to the vault directory.
    pub vault_path: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl MkbMcpService {
    /// Create a new MKB MCP server for the given vault path.
    pub fn new(vault_path: PathBuf) -> Self {
        Self {
            vault_path,
            tool_router: Self::tool_router(),
        }
    }

    fn open_index(&self) -> Result<IndexManager, String> {
        let index_path = self.vault_path.join(".mkb").join("index").join("mkb.db");
        IndexManager::open(&index_path).map_err(|e| format!("Failed to open index: {e}"))
    }

    fn open_vault(&self) -> Result<Vault, String> {
        Vault::open(&self.vault_path).map_err(|e| format!("Failed to open vault: {e}"))
    }
}

// === Tool request types ===

/// Request for MKQL query execution.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryRequest {
    /// MKQL query string (e.g., "SELECT * FROM project WHERE CURRENT()")
    pub mkql: String,
}

/// Request for full-text search.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    /// Full-text search query
    pub query: String,
    /// Maximum results to return (default: 10)
    pub limit: Option<usize>,
}

/// Request for semantic (vector) search.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SemanticSearchRequest {
    /// Text query for semantic similarity search
    pub query: String,
    /// Maximum results to return (default: 10)
    pub limit: Option<usize>,
}

/// Request to read a specific document.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetDocumentRequest {
    /// Document type (e.g., project, meeting)
    pub doc_type: String,
    /// Document ID (e.g., proj-alpha-001)
    pub id: String,
}

#[tool_router]
impl MkbMcpService {
    /// Execute an MKQL query and return JSON results.
    #[tool(
        description = "Execute an MKQL (Markdown Knowledge Query Language) query and return JSON results"
    )]
    fn mkb_query(&self, Parameters(req): Parameters<QueryRequest>) -> String {
        let index = match self.open_index() {
            Ok(i) => i,
            Err(e) => return format!("{{\"error\": \"{e}\"}}"),
        };
        let ast = match mkb_parser::parse_mkql(&req.mkql) {
            Ok(a) => a,
            Err(e) => return format!("{{\"error\": \"Parse error: {e}\"}}"),
        };
        let compiled = match mkb_query::compile(&ast) {
            Ok(c) => c,
            Err(e) => return format!("{{\"error\": \"Compile error: {e}\"}}"),
        };
        match mkb_query::execute(&index, &compiled) {
            Ok(result) => mkb_query::format_results(&result, mkb_query::OutputFormat::Json),
            Err(e) => format!("{{\"error\": \"Execution error: {e}\"}}"),
        }
    }

    /// Full-text search across all documents.
    #[tool(description = "Full-text search across all documents using FTS5")]
    fn mkb_search(&self, Parameters(req): Parameters<SearchRequest>) -> String {
        let index = match self.open_index() {
            Ok(i) => i,
            Err(e) => return format!("{{\"error\": \"{e}\"}}"),
        };
        let results = match index.search_fts(&req.query) {
            Ok(r) => r,
            Err(e) => return format!("{{\"error\": \"Search failed: {e}\"}}"),
        };
        let limit = req.limit.unwrap_or(10);
        let json: Vec<serde_json::Value> = results
            .iter()
            .take(limit)
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "type": r.doc_type,
                    "title": r.title,
                    "rank": r.rank,
                })
            })
            .collect();
        serde_json::to_string_pretty(&json).unwrap_or_else(|_| "[]".to_string())
    }

    /// Vector similarity search using embeddings.
    #[tool(description = "Vector similarity search using embeddings")]
    fn mkb_search_semantic(&self, Parameters(req): Parameters<SemanticSearchRequest>) -> String {
        let index = match self.open_index() {
            Ok(i) => i,
            Err(e) => return format!("{{\"error\": \"{e}\"}}"),
        };
        let embedding = mkb_index::mock_embedding(&req.query);
        let limit = req.limit.unwrap_or(10);
        let results = match index.search_semantic(&embedding, limit) {
            Ok(r) => r,
            Err(e) => return format!("{{\"error\": \"Semantic search failed: {e}\"}}"),
        };
        let json: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "type": r.doc_type,
                    "title": r.title,
                    "distance": r.distance,
                })
            })
            .collect();
        serde_json::to_string_pretty(&json).unwrap_or_else(|_| "[]".to_string())
    }

    /// Read a specific document by type and ID.
    #[tool(description = "Read a specific document by type and ID, returning its full content")]
    fn mkb_get_document(&self, Parameters(req): Parameters<GetDocumentRequest>) -> String {
        let vault = match self.open_vault() {
            Ok(v) => v,
            Err(e) => return format!("{{\"error\": \"{e}\"}}"),
        };
        let doc = match vault.read(&req.doc_type, &req.id) {
            Ok(d) => d,
            Err(e) => return format!("{{\"error\": \"Document not found: {e}\"}}"),
        };
        let json = serde_json::json!({
            "id": doc.id,
            "type": doc.doc_type,
            "title": doc.title,
            "body": doc.body,
            "tags": doc.tags,
            "observed_at": doc.temporal.observed_at.to_rfc3339(),
            "valid_until": doc.temporal.valid_until.to_rfc3339(),
            "confidence": doc.confidence,
            "source": doc.source,
            "fields": doc.fields,
        });
        serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".to_string())
    }

    /// List all document types that have indexed documents.
    #[tool(description = "List all document types that have indexed documents")]
    fn mkb_list_types(&self) -> String {
        let index = match self.open_index() {
            Ok(i) => i,
            Err(e) => return format!("{{\"error\": \"{e}\"}}"),
        };
        let all = match index.query_all() {
            Ok(a) => a,
            Err(e) => return format!("{{\"error\": \"Query failed: {e}\"}}"),
        };
        let mut types: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for doc in &all {
            *types.entry(doc.doc_type.clone()).or_insert(0) += 1;
        }
        let json: Vec<serde_json::Value> = types
            .iter()
            .map(|(t, count)| serde_json::json!({"type": t, "count": count}))
            .collect();
        serde_json::to_string_pretty(&json).unwrap_or_else(|_| "[]".to_string())
    }

    /// Get vault health status.
    #[tool(
        description = "Get vault health status including document count, index sync, and stale documents"
    )]
    fn mkb_vault_status(&self) -> String {
        let vault = match self.open_vault() {
            Ok(v) => v,
            Err(e) => return format!("{{\"error\": \"{e}\"}}"),
        };
        let index = match self.open_index() {
            Ok(i) => i,
            Err(e) => return format!("{{\"error\": \"{e}\"}}"),
        };
        let doc_count = index.count().unwrap_or(0);
        let files = vault.list_documents().unwrap_or_default();
        let rejection_count = vault.rejection_count().unwrap_or(0);
        let index_synced = files.len() as u64 == doc_count;
        let now = chrono::Utc::now().to_rfc3339();
        let stale_count = index.staleness_sweep(&now).unwrap_or_default().len();

        let json = serde_json::json!({
            "vault_root": vault.root().display().to_string(),
            "indexed_documents": doc_count,
            "vault_files": files.len(),
            "index_synced": index_synced,
            "rejection_count": rejection_count,
            "stale_documents": stale_count,
        });
        serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".to_string())
    }
}

#[tool_handler]
impl ServerHandler for MkbMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "MKB (Markdown Knowledge Base) server. Query documents with MKQL, \
                 search full-text or semantically, read documents, and check vault status."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_service_creation() {
        let service = MkbMcpService::new(PathBuf::from("/tmp/test-vault"));
        assert_eq!(service.vault_path, PathBuf::from("/tmp/test-vault"));
    }
}
