//! MCP tool definitions for MKB vault operations (read-only).

use std::path::PathBuf;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, ListResourceTemplatesResult, PaginatedRequestParams, RawResourceTemplate,
        ReadResourceRequestParams, ReadResourceResult, ResourceContents, ServerCapabilities,
        ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler,
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

    fn handle_read_resource(&self, uri: &str) -> Result<ReadResourceResult, ErrorData> {
        // Parse mkb://vault/{type}/{id}
        if let Some(rest) = uri.strip_prefix("mkb://vault/") {
            let parts: Vec<&str> = rest.splitn(2, '/').collect();
            if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
                return Err(ErrorData::invalid_params(
                    "Invalid vault URI: expected mkb://vault/{type}/{id}",
                    None,
                ));
            }
            let doc_type = parts[0];
            let doc_id = parts[1];
            let vault = self
                .open_vault()
                .map_err(|e| ErrorData::internal_error(e, None))?;
            let doc = vault
                .read(doc_type, doc_id)
                .map_err(|e| ErrorData::internal_error(format!("Document not found: {e}"), None))?;
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
            let text = serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".to_string());
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(text, uri)],
            });
        }

        // Parse mkb://query/{mkql}
        if let Some(mkql_encoded) = uri.strip_prefix("mkb://query/") {
            let mkql = urlencoding::decode(mkql_encoded)
                .map_err(|e| ErrorData::invalid_params(format!("Invalid URI encoding: {e}"), None))?
                .into_owned();
            let index = self
                .open_index()
                .map_err(|e| ErrorData::internal_error(e, None))?;
            let ast = mkb_parser::parse_mkql(&mkql)
                .map_err(|e| ErrorData::invalid_params(format!("Parse error: {e}"), None))?;
            let compiled = mkb_query::compile(&ast)
                .map_err(|e| ErrorData::internal_error(format!("Compile error: {e}"), None))?;
            let result = mkb_query::execute(&index, &compiled)
                .map_err(|e| ErrorData::internal_error(format!("Execution error: {e}"), None))?;
            let text = mkb_query::format_results(&result, mkb_query::OutputFormat::Json);
            return Ok(ReadResourceResult {
                contents: vec![ResourceContents::text(text, uri)],
            });
        }

        Err(ErrorData::invalid_params(
            format!("Unknown resource URI scheme: {uri}"),
            None,
        ))
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
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            ..Default::default()
        }
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, ErrorData>> + Send + '_
    {
        let templates = vec![
            RawResourceTemplate {
                uri_template: "mkb://vault/{type}/{id}".to_string(),
                name: "Document".to_string(),
                title: Some("MKB Document".to_string()),
                description: Some("Read a document from the vault by type and ID".to_string()),
                mime_type: Some("application/json".to_string()),
                icons: None,
            }
            .no_annotation(),
            RawResourceTemplate {
                uri_template: "mkb://query/{mkql}".to_string(),
                name: "Query".to_string(),
                title: Some("MKQL Query Results".to_string()),
                description: Some("Execute an MKQL query and return results as JSON".to_string()),
                mime_type: Some("application/json".to_string()),
                icons: None,
            }
            .no_annotation(),
        ];
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(templates)))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, ErrorData>> + Send + '_ {
        let result = self.handle_read_resource(&request.uri);
        std::future::ready(result)
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

    fn setup_vault_with_doc() -> (PathBuf, MkbMcpService, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().to_path_buf();
        let vault = mkb_vault::Vault::init(&vault_path).unwrap();

        let input = mkb_core::temporal::RawTemporalInput {
            observed_at: Some(chrono::Utc::now()),
            ..Default::default()
        };
        let profile = mkb_core::temporal::DecayProfile::new(chrono::Duration::days(14));
        let mut doc = mkb_core::Document::new(
            "proj-alpha-001".to_string(),
            "project".to_string(),
            "Alpha Project".to_string(),
            input,
            &profile,
        )
        .unwrap();
        doc.body = "# Alpha\n\nProject details here.".to_string();
        vault.create(&doc).unwrap();

        let index_path = vault_path.join(".mkb").join("index").join("mkb.db");
        let index = mkb_index::IndexManager::open(&index_path).unwrap();
        index.index_document(&doc).unwrap();

        let service = MkbMcpService::new(vault_path.clone());
        (vault_path, service, dir)
    }

    #[test]
    fn read_resource_vault_document() {
        let (_vault_path, service, _dir) = setup_vault_with_doc();
        let result = service
            .handle_read_resource("mkb://vault/project/proj-alpha-001")
            .unwrap();
        assert_eq!(result.contents.len(), 1);
        match &result.contents[0] {
            ResourceContents::TextResourceContents { text, uri, .. } => {
                assert!(text.contains("Alpha Project"));
                assert!(text.contains("proj-alpha-001"));
                assert_eq!(uri, "mkb://vault/project/proj-alpha-001");
            }
            _ => panic!("Expected TextResourceContents"),
        }
    }

    #[test]
    fn read_resource_query() {
        let (_vault_path, service, _dir) = setup_vault_with_doc();
        let mkql = urlencoding::encode("SELECT * FROM project");
        let uri = format!("mkb://query/{mkql}");
        let result = service.handle_read_resource(&uri).unwrap();
        assert_eq!(result.contents.len(), 1);
        match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                assert!(text.contains("proj-alpha-001"));
            }
            _ => panic!("Expected TextResourceContents"),
        }
    }

    #[test]
    fn read_resource_invalid_vault_uri() {
        let service = MkbMcpService::new(PathBuf::from("/tmp/nonexistent"));
        let result = service.handle_read_resource("mkb://vault/project/");
        assert!(result.is_err());
    }

    #[test]
    fn read_resource_unknown_scheme() {
        let service = MkbMcpService::new(PathBuf::from("/tmp/test"));
        let result = service.handle_read_resource("https://example.com");
        assert!(result.is_err());
    }
}
