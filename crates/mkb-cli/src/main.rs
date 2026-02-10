//! MKB CLI — Markdown Knowledge Base for LLMs
//!
//! Commands: init, add, query

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;

use mkb_core::document::Document;
use mkb_core::temporal::{DecayProfile, RawTemporalInput, TemporalPrecision};
use mkb_index::IndexManager;
use mkb_vault::Vault;

#[derive(Parser)]
#[command(name = "mkb")]
#[command(version)]
#[command(about = "Markdown Knowledge Base for LLMs")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize a new MKB vault
    Init {
        /// Directory to initialize (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Create a new knowledge document
    Add {
        /// Document type (e.g., project, meeting, decision)
        #[arg(long)]
        doc_type: String,

        /// Document title
        #[arg(long)]
        title: String,

        /// When this information was observed (ISO 8601 datetime)
        #[arg(long)]
        observed_at: DateTime<Utc>,

        /// When this information expires (computed from decay profile if omitted)
        #[arg(long)]
        valid_until: Option<DateTime<Utc>>,

        /// Temporal precision (exact, day, week, month, quarter, approximate, inferred)
        #[arg(long, default_value = "day")]
        precision: String,

        /// Document body (markdown content)
        #[arg(long, default_value = "")]
        body: String,

        /// Tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Execute a query and return JSON results
    #[command(alias = "q")]
    Query {
        /// Document type to query (omit for all)
        #[arg(long)]
        doc_type: Option<String>,

        /// Full-text search query
        #[arg(long)]
        search: Option<String>,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Quick full-text or semantic search
    #[command(alias = "s")]
    Search {
        /// Search query
        query: String,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Vault statistics
    Stats {
        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { path }) => cmd_init(&path),
        Some(Commands::Add {
            doc_type,
            title,
            observed_at,
            valid_until,
            precision,
            body,
            tags,
            vault,
        }) => cmd_add(
            &vault,
            &doc_type,
            &title,
            observed_at,
            valid_until,
            &precision,
            &body,
            tags.as_deref(),
        ),
        Some(Commands::Query {
            doc_type,
            search,
            vault,
        }) => cmd_query(&vault, doc_type.as_deref(), search.as_deref()),
        Some(Commands::Search { query, vault }) => cmd_search(&vault, &query),
        Some(Commands::Stats { vault }) => cmd_stats(&vault),
        None => {
            println!(
                "MKB v{} — Markdown Knowledge Base for LLMs",
                env!("CARGO_PKG_VERSION")
            );
            println!("Run `mkb --help` for usage.");
            Ok(())
        }
    }
}

fn cmd_init(path: &Path) -> Result<()> {
    let vault = Vault::init(path).context("Failed to initialize vault")?;
    let index_path = path.join(".mkb").join("index").join("mkb.db");
    let _index = IndexManager::open(&index_path).context("Failed to create index")?;

    println!(
        "Initialized MKB vault at {}",
        vault.root().canonicalize()?.display()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_add(
    vault_path: &Path,
    doc_type: &str,
    title: &str,
    observed_at: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    precision: &str,
    body: &str,
    tags: Option<&str>,
) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;
    let index_path = vault_path.join(".mkb").join("index").join("mkb.db");
    let index = IndexManager::open(&index_path).context("Failed to open index")?;

    let temporal_precision = parse_precision(precision)?;
    let profile = DecayProfile::default_profile();

    // Count existing docs of this type for ID generation
    let existing = index.query_by_type(doc_type).unwrap_or_default();
    let counter = (existing.len() as u32) + 1;
    let id = Document::generate_id(doc_type, title, counter);

    let input = RawTemporalInput {
        observed_at: Some(observed_at),
        valid_until,
        temporal_precision: Some(temporal_precision),
        occurred_at: None,
    };

    let mut doc = Document::new(id, doc_type.to_string(), title.to_string(), input, &profile)
        .context("Temporal gate rejected document")?;

    doc.body = body.to_string();
    if let Some(tags_str) = tags {
        doc.tags = tags_str.split(',').map(|s| s.trim().to_string()).collect();
    }

    let path = vault.create(&doc).context("Failed to create document")?;
    index
        .index_document(&doc)
        .context("Failed to index document")?;

    // Output JSON for programmatic consumption
    let output = serde_json::json!({
        "id": doc.id,
        "type": doc.doc_type,
        "title": doc.title,
        "path": path.display().to_string(),
        "observed_at": doc.temporal.observed_at.to_rfc3339(),
        "valid_until": doc.temporal.valid_until.to_rfc3339(),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

fn cmd_query(vault_path: &Path, doc_type: Option<&str>, search: Option<&str>) -> Result<()> {
    let _vault = Vault::open(vault_path).context("Failed to open vault")?;
    let index_path = vault_path.join(".mkb").join("index").join("mkb.db");
    let index = IndexManager::open(&index_path).context("Failed to open index")?;

    if let Some(query) = search {
        let results = index.search_fts(query).context("FTS search failed")?;
        let json: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "type": r.doc_type,
                    "title": r.title,
                    "rank": r.rank,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else if let Some(dtype) = doc_type {
        let results = index.query_by_type(dtype).context("Query by type failed")?;
        let json: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "type": r.doc_type,
                    "title": r.title,
                    "observed_at": r.observed_at,
                    "valid_until": r.valid_until,
                    "confidence": r.confidence,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        let results = index.query_all().context("Query all failed")?;
        let json: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "type": r.doc_type,
                    "title": r.title,
                    "observed_at": r.observed_at,
                    "valid_until": r.valid_until,
                    "confidence": r.confidence,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    }

    Ok(())
}

fn cmd_search(vault_path: &Path, query: &str) -> Result<()> {
    cmd_query(vault_path, None, Some(query))
}

fn cmd_stats(vault_path: &Path) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;
    let index_path = vault_path.join(".mkb").join("index").join("mkb.db");
    let index = IndexManager::open(&index_path).context("Failed to open index")?;

    let doc_count = index.count().context("Failed to count documents")?;
    let files = vault.list_documents().unwrap_or_default();

    let output = serde_json::json!({
        "vault_root": vault.root().display().to_string(),
        "indexed_documents": doc_count,
        "vault_files": files.len(),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

fn parse_precision(s: &str) -> Result<TemporalPrecision> {
    match s.to_lowercase().as_str() {
        "exact" => Ok(TemporalPrecision::Exact),
        "day" => Ok(TemporalPrecision::Day),
        "week" => Ok(TemporalPrecision::Week),
        "month" => Ok(TemporalPrecision::Month),
        "quarter" => Ok(TemporalPrecision::Quarter),
        "approximate" | "approx" => Ok(TemporalPrecision::Approximate),
        "inferred" => Ok(TemporalPrecision::Inferred),
        other => anyhow::bail!(
            "Unknown precision '{}'. Valid: exact, day, week, month, quarter, approximate, inferred",
            other
        ),
    }
}
