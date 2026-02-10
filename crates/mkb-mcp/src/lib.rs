//! # mkb-mcp
//!
//! MCP (Model Context Protocol) server for MKB vault.
//!
//! Exposes read-only vault operations as MCP tools:
//! - `mkb_query`: Execute MKQL queries
//! - `mkb_search`: Full-text search (FTS5)
//! - `mkb_search_semantic`: Vector similarity search
//! - `mkb_get_document`: Read a document by type + ID
//! - `mkb_list_types`: List available document types
//! - `mkb_vault_status`: Vault health stats
//!
//! Also provides MCP resource templates:
//! - `mkb://vault/{type}/{id}`: Read a document by type and ID
//! - `mkb://query/{mkql}`: Execute an MKQL query (URL-encoded)

pub mod tools;
