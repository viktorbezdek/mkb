//! MKB CLI — Markdown Knowledge Base for LLMs
//!
//! Commands: init, add, query, search, edit, rm, link, schema, gc, stats, status, ingest

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;

use mkb_core::document::Document;
use mkb_core::frontmatter;
use mkb_core::link::Link;
use mkb_core::schema;
use mkb_core::temporal::{DecayProfile, RawTemporalInput, TemporalPrecision};
use mkb_index::IndexManager;
use mkb_query::{compile, execute, format_results, OutputFormat};
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

        /// Read content from a markdown file with frontmatter
        #[arg(long)]
        from_file: Option<PathBuf>,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Execute an MKQL query
    #[command(alias = "q")]
    Query {
        /// MKQL query string (e.g., "SELECT * FROM project WHERE CURRENT()")
        mkql: Option<String>,

        /// Document type to query (used when no MKQL string given)
        #[arg(long)]
        doc_type: Option<String>,

        /// Full-text search query (used when no MKQL string given)
        #[arg(long)]
        search: Option<String>,

        /// Output format: json, table, markdown, context
        #[arg(long, short, default_value = "json")]
        format: String,

        /// Save this query as a named view
        #[arg(long)]
        save: Option<String>,

        /// Run a saved view by name (instead of an MKQL string)
        #[arg(long)]
        view: Option<String>,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Quick full-text search
    #[command(alias = "s")]
    Search {
        /// Search query (text for FTS/semantic, or omit with --embedding)
        query: Option<String>,

        /// Output format: json, table, markdown
        #[arg(long, short, default_value = "json")]
        format: String,

        /// Use semantic (vector) search instead of full-text search
        #[arg(long)]
        semantic: bool,

        /// Pre-computed embedding vector as JSON array (e.g., '[0.1, 0.2, ...]')
        #[arg(long)]
        embedding: Option<String>,

        /// Maximum results to return
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Edit a document's fields
    Edit {
        /// Document ID (e.g., proj-alpha-001)
        id: String,

        /// Fields to set as key=value pairs
        #[arg(long, short = 's', num_args = 1..)]
        set: Vec<String>,

        /// New title
        #[arg(long)]
        title: Option<String>,

        /// New body content
        #[arg(long)]
        body: Option<String>,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Remove a document (soft delete to archive)
    Rm {
        /// Document ID (e.g., proj-alpha-001)
        id: String,

        /// Document type
        #[arg(long)]
        doc_type: String,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Manage links between documents
    Link {
        #[command(subcommand)]
        action: LinkAction,
    },

    /// Manage document schemas
    Schema {
        #[command(subcommand)]
        action: SchemaAction,
    },

    /// Garbage collect: sweep stale documents
    Gc {
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

    /// Vault health status (rejection count, index health)
    Status {
        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Manage saved views (named MKQL queries)
    View {
        #[command(subcommand)]
        action: ViewAction,
    },

    /// Ingest files into the vault
    Ingest {
        /// File or directory to ingest
        path: PathBuf,

        /// Document type for ingested documents
        #[arg(long, default_value = "document")]
        doc_type: String,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },
}

#[derive(clap::Subcommand)]
enum LinkAction {
    /// Create a link between two documents
    Create {
        /// Source document ID
        #[arg(long)]
        source: String,

        /// Relationship type (e.g., owner, blocked_by, depends_on)
        #[arg(long)]
        rel: String,

        /// Target document ID
        #[arg(long)]
        target: String,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// List links for a document
    List {
        /// Document ID
        id: String,

        /// Show reverse links (pointing to this document)
        #[arg(long)]
        reverse: bool,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },
}

#[derive(clap::Subcommand)]
enum SchemaAction {
    /// List all available schemas
    List,

    /// Validate a document against its schema
    Validate {
        /// Document ID
        id: String,

        /// Document type
        #[arg(long)]
        doc_type: String,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },
}

#[derive(clap::Subcommand)]
enum ViewAction {
    /// Save an MKQL query as a named view
    Save {
        /// View name
        name: String,

        /// MKQL query string
        mkql: String,

        /// Optional description
        #[arg(long)]
        description: Option<String>,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// List all saved views
    List {
        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Run a saved view
    Run {
        /// View name
        name: String,

        /// Output format: json, table, markdown
        #[arg(long, short, default_value = "json")]
        format: String,

        /// Vault directory (defaults to current directory)
        #[arg(long, default_value = ".")]
        vault: PathBuf,
    },

    /// Delete a saved view
    Delete {
        /// View name
        name: String,

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
            from_file,
            vault,
        }) => {
            if let Some(file_path) = from_file {
                cmd_add_from_file(&vault, &file_path)
            } else {
                cmd_add(
                    &vault,
                    &doc_type,
                    &title,
                    observed_at,
                    valid_until,
                    &precision,
                    &body,
                    tags.as_deref(),
                )
            }
        }
        Some(Commands::Query {
            mkql,
            doc_type,
            search,
            format,
            vault,
            save,
            view,
        }) => {
            // --view flag: load saved view and run it
            if let Some(view_name) = view {
                let v = Vault::open(&vault).context("Failed to open vault")?;
                let saved = v
                    .load_view(&view_name)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                return cmd_query(&vault, Some(&saved.query), None, None, &format);
            }
            // --save flag: save the query as a view, then run it
            if let Some(save_name) = save {
                if let Some(ref mkql_str) = mkql {
                    let v = Vault::open(&vault).context("Failed to open vault")?;
                    let saved_view = mkb_core::view::SavedView {
                        name: save_name,
                        description: None,
                        query: mkql_str.clone(),
                        created_at: Utc::now().to_rfc3339(),
                    };
                    v.save_view(&saved_view)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    eprintln!("View '{}' saved.", saved_view.name);
                } else {
                    anyhow::bail!("--save requires an MKQL query string");
                }
            }
            cmd_query(
                &vault,
                mkql.as_deref(),
                doc_type.as_deref(),
                search.as_deref(),
                &format,
            )
        }
        Some(Commands::Search {
            query,
            format,
            semantic,
            embedding,
            limit,
            vault,
        }) => {
            if semantic || embedding.is_some() {
                cmd_search_semantic(&vault, query.as_deref(), embedding.as_deref(), limit, &format)
            } else {
                let q = query.as_deref().unwrap_or("");
                cmd_search(&vault, q, &format)
            }
        }
        Some(Commands::Edit {
            id,
            set,
            title,
            body,
            vault,
        }) => cmd_edit(&vault, &id, &set, title.as_deref(), body.as_deref()),
        Some(Commands::Rm {
            id,
            doc_type,
            vault,
        }) => cmd_rm(&vault, &doc_type, &id),
        Some(Commands::Link { action }) => match action {
            LinkAction::Create {
                source,
                rel,
                target,
                vault,
            } => cmd_link_create(&vault, &source, &rel, &target),
            LinkAction::List { id, reverse, vault } => cmd_link_list(&vault, &id, reverse),
        },
        Some(Commands::Schema { action }) => match action {
            SchemaAction::List => cmd_schema_list(),
            SchemaAction::Validate {
                id,
                doc_type,
                vault,
            } => cmd_schema_validate(&vault, &doc_type, &id),
        },
        Some(Commands::View { action }) => match action {
            ViewAction::Save {
                name,
                mkql,
                description,
                vault,
            } => cmd_view_save(&vault, &name, &mkql, description.as_deref()),
            ViewAction::List { vault } => cmd_view_list(&vault),
            ViewAction::Run {
                name,
                format,
                vault,
            } => cmd_view_run(&vault, &name, &format),
            ViewAction::Delete { name, vault } => cmd_view_delete(&vault, &name),
        },
        Some(Commands::Gc { vault }) => cmd_gc(&vault),
        Some(Commands::Stats { vault }) => cmd_stats(&vault),
        Some(Commands::Status { vault }) => cmd_status(&vault),
        Some(Commands::Ingest {
            path,
            doc_type,
            vault,
        }) => cmd_ingest(&vault, &path, &doc_type),
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

// === Init ===

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

// === Add ===

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
    let index = open_index(vault_path)?;

    let temporal_precision = parse_precision(precision)?;
    let profile = DecayProfile::default_profile();

    let counter = mkb_vault::next_counter(vault_path, doc_type, &mkb_vault::slugify(title));
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

fn cmd_add_from_file(vault_path: &Path, file_path: &Path) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;
    let index = open_index(vault_path)?;

    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    let doc = frontmatter::parse_document(&content).context("Failed to parse frontmatter")?;

    let path = vault.create(&doc).context("Failed to create document")?;
    index
        .index_document(&doc)
        .context("Failed to index document")?;

    let output = serde_json::json!({
        "id": doc.id,
        "type": doc.doc_type,
        "title": doc.title,
        "path": path.display().to_string(),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// === Query ===

fn cmd_query(
    vault_path: &Path,
    mkql: Option<&str>,
    doc_type: Option<&str>,
    search: Option<&str>,
    format: &str,
) -> Result<()> {
    let index = open_index(vault_path)?;

    if let Some(mkql_str) = mkql {
        // Full MKQL query execution
        let ast =
            mkb_parser::parse_mkql(mkql_str).map_err(|e| anyhow::anyhow!("Parse error: {e}"))?;
        let compiled = compile(&ast).map_err(|e| anyhow::anyhow!("Compile error: {e}"))?;
        let result =
            execute(&index, &compiled).map_err(|e| anyhow::anyhow!("Execution error: {e}"))?;

        let output_format = parse_format(format)?;
        println!("{}", format_results(&result, output_format));
    } else if let Some(query) = search {
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
        print_indexed_docs(&results)?;
    } else {
        let results = index.query_all().context("Query all failed")?;
        print_indexed_docs(&results)?;
    }

    Ok(())
}

// === Search ===

fn cmd_search(vault_path: &Path, query: &str, format: &str) -> Result<()> {
    let index = open_index(vault_path)?;

    let results = index.search_fts(query).context("FTS search failed")?;

    match format {
        "json" => {
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
        }
        "table" => {
            if results.is_empty() {
                println!("(no results)");
            } else {
                println!("{:<30} {:<15} {:<30} {:>8}", "ID", "TYPE", "TITLE", "RANK");
                println!("{}", "-".repeat(86));
                for r in &results {
                    println!(
                        "{:<30} {:<15} {:<30} {:>8.2}",
                        r.id, r.doc_type, r.title, r.rank
                    );
                }
            }
        }
        _ => {
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
        }
    }
    Ok(())
}

// === Semantic Search ===

fn cmd_search_semantic(
    vault_path: &Path,
    query: Option<&str>,
    embedding_json: Option<&str>,
    limit: usize,
    format: &str,
) -> Result<()> {
    let index = open_index(vault_path)?;

    let embedding: Vec<f32> = if let Some(json_str) = embedding_json {
        serde_json::from_str(json_str).context("Invalid embedding JSON (expected array of floats)")?
    } else if let Some(q) = query {
        mkb_index::mock_embedding(q)
    } else {
        anyhow::bail!("Semantic search requires either a query string or --embedding vector");
    };

    let results = index
        .search_semantic(&embedding, limit)
        .context("Semantic search failed")?;

    match format {
        "json" => {
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
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        "table" => {
            if results.is_empty() {
                println!("(no results)");
            } else {
                println!("{:<30} {:<15} {:<30} {:>10}", "ID", "TYPE", "TITLE", "DISTANCE");
                println!("{}", "-".repeat(88));
                for r in &results {
                    println!(
                        "{:<30} {:<15} {:<30} {:>10.4}",
                        r.id, r.doc_type, r.title, r.distance
                    );
                }
            }
        }
        _ => {
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
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
    }
    Ok(())
}

// === Edit ===

fn cmd_edit(
    vault_path: &Path,
    id: &str,
    set_fields: &[String],
    new_title: Option<&str>,
    new_body: Option<&str>,
) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;
    let index = open_index(vault_path)?;

    // Find the document type by searching the index
    let all = index.query_all().context("Failed to query index")?;
    let indexed = all
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| anyhow::anyhow!("Document not found: {id}"))?;

    let mut doc = vault
        .read(&indexed.doc_type, id)
        .context("Failed to read document")?;

    if let Some(title) = new_title {
        doc.title = title.to_string();
    }
    if let Some(body) = new_body {
        doc.body = body.to_string();
    }

    // Parse key=value fields
    for field in set_fields {
        if let Some((key, value)) = field.split_once('=') {
            doc.fields.insert(key.to_string(), serde_json::json!(value));
        } else {
            anyhow::bail!("Invalid field format: '{}'. Expected key=value", field);
        }
    }

    let path = vault
        .update(&mut doc)
        .context("Failed to update document")?;
    index
        .index_document(&doc)
        .context("Failed to re-index document")?;

    let output = serde_json::json!({
        "id": doc.id,
        "title": doc.title,
        "path": path.display().to_string(),
        "modified_at": doc.modified_at.to_rfc3339(),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// === Rm ===

fn cmd_rm(vault_path: &Path, doc_type: &str, id: &str) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;
    let index = open_index(vault_path)?;

    let archive_path = vault
        .delete(doc_type, id)
        .context("Failed to delete document")?;
    index
        .remove_document(id)
        .context("Failed to remove from index")?;

    let output = serde_json::json!({
        "id": id,
        "archived_to": archive_path.display().to_string(),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// === Link ===

fn cmd_link_create(vault_path: &Path, source: &str, rel: &str, target: &str) -> Result<()> {
    let index = open_index(vault_path)?;

    let link = Link {
        rel: rel.to_string(),
        target: target.to_string(),
        observed_at: Utc::now(),
        metadata: None,
    };

    // Get existing links and append the new one
    let mut existing = index
        .query_forward_links(source)
        .context("Failed to query existing links")?;

    let new_links: Vec<Link> = existing
        .drain(..)
        .map(|l| Link {
            rel: l.rel,
            target: l.target_id,
            observed_at: chrono::DateTime::parse_from_rfc3339(&l.observed_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            metadata: None,
        })
        .chain(std::iter::once(link))
        .collect();

    index
        .store_links(source, &new_links)
        .context("Failed to store link")?;

    let output = serde_json::json!({
        "source": source,
        "rel": rel,
        "target": target,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn cmd_link_list(vault_path: &Path, id: &str, reverse: bool) -> Result<()> {
    let index = open_index(vault_path)?;

    if reverse {
        let links = index
            .query_reverse_links(id)
            .context("Failed to query reverse links")?;
        let json: Vec<serde_json::Value> = links
            .iter()
            .map(|l| {
                serde_json::json!({
                    "source": l.source_id,
                    "rel": l.rel,
                    "target": l.target_id,
                    "observed_at": l.observed_at,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        let links = index
            .query_forward_links(id)
            .context("Failed to query forward links")?;
        let json: Vec<serde_json::Value> = links
            .iter()
            .map(|l| {
                serde_json::json!({
                    "source": l.source_id,
                    "rel": l.rel,
                    "target": l.target_id,
                    "observed_at": l.observed_at,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    }

    Ok(())
}

// === Schema ===

fn cmd_schema_list() -> Result<()> {
    let schemas = schema::built_in_schemas();
    let json: Vec<serde_json::Value> = schemas
        .iter()
        .map(|s| {
            let field_names: Vec<&str> = s.fields.keys().map(|k| k.as_str()).collect();
            serde_json::json!({
                "name": s.name,
                "version": s.version,
                "description": s.description,
                "fields": field_names,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

fn cmd_schema_validate(vault_path: &Path, doc_type: &str, id: &str) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;

    let doc = vault
        .read(doc_type, id)
        .context("Failed to read document")?;

    let schemas = schema::built_in_schemas();
    let matching = schemas.iter().find(|s| s.name == doc_type);

    if let Some(schema_def) = matching {
        let result = schema_def.validate(doc_type, &doc.fields);
        let output = serde_json::json!({
            "id": id,
            "doc_type": doc_type,
            "valid": result.errors.is_empty(),
            "errors": result.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
            "warnings": result.warnings,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let output = serde_json::json!({
            "id": id,
            "doc_type": doc_type,
            "valid": true,
            "message": format!("No schema defined for type '{doc_type}', skipping validation"),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    }

    Ok(())
}

// === GC ===

fn cmd_gc(vault_path: &Path) -> Result<()> {
    let index = open_index(vault_path)?;

    let now = Utc::now().to_rfc3339();
    let stale_ids = index
        .staleness_sweep(&now)
        .context("Failed to run staleness sweep")?;

    let output = serde_json::json!({
        "swept_at": now,
        "stale_count": stale_ids.len(),
        "stale_ids": stale_ids,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// === Stats ===

fn cmd_stats(vault_path: &Path) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;
    let index = open_index(vault_path)?;

    let doc_count = index.count().context("Failed to count documents")?;
    let files = vault.list_documents().unwrap_or_default();

    // Count by type
    let all_docs = index.query_all().unwrap_or_default();
    let mut type_counts: HashMap<String, usize> = HashMap::new();
    for doc in &all_docs {
        *type_counts.entry(doc.doc_type.clone()).or_insert(0) += 1;
    }

    let output = serde_json::json!({
        "vault_root": vault.root().display().to_string(),
        "indexed_documents": doc_count,
        "vault_files": files.len(),
        "by_type": type_counts,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// === Status ===

fn cmd_status(vault_path: &Path) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;
    let index = open_index(vault_path)?;

    let doc_count = index.count().context("Failed to count documents")?;
    let rejection_count = vault.rejection_count().unwrap_or(0);
    let files = vault.list_documents().unwrap_or_default();

    // Index health: compare file count with indexed count
    let index_synced = files.len() as u64 == doc_count;

    let now = Utc::now().to_rfc3339();
    let stale_count = index.staleness_sweep(&now).unwrap_or_default().len();

    let output = serde_json::json!({
        "vault_root": vault.root().display().to_string(),
        "indexed_documents": doc_count,
        "vault_files": files.len(),
        "index_synced": index_synced,
        "rejection_count": rejection_count,
        "stale_documents": stale_count,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// === Ingest ===

fn cmd_ingest(vault_path: &Path, input_path: &Path, doc_type: &str) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;
    let index = open_index(vault_path)?;

    let paths: Vec<PathBuf> = if input_path.is_dir() {
        // Collect all .md files from directory
        fs::read_dir(input_path)
            .context("Failed to read directory")?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect()
    } else {
        vec![input_path.to_path_buf()]
    };

    let mut ingested = Vec::new();
    let mut rejected = Vec::new();

    for file_path in &paths {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                rejected.push(serde_json::json!({
                    "file": file_path.display().to_string(),
                    "error": e.to_string(),
                }));
                continue;
            }
        };

        match ingest_single_file(&vault, &index, vault_path, &content, doc_type) {
            Ok(doc_id) => {
                ingested.push(serde_json::json!({
                    "file": file_path.display().to_string(),
                    "id": doc_id,
                }));
            }
            Err(e) => {
                // Write to rejection log
                let filename = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                let _ = vault.write_rejection(filename, &content, &e.to_string(), &[]);
                rejected.push(serde_json::json!({
                    "file": file_path.display().to_string(),
                    "error": e.to_string(),
                }));
            }
        }
    }

    let output = serde_json::json!({
        "ingested": ingested.len(),
        "rejected": rejected.len(),
        "files": ingested,
        "errors": rejected,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn ingest_single_file(
    vault: &Vault,
    index: &IndexManager,
    vault_path: &Path,
    content: &str,
    default_doc_type: &str,
) -> Result<String> {
    // Try to parse as frontmatter document first
    if let Ok(doc) = frontmatter::parse_document(content) {
        let doc_id = doc.id.clone();
        vault.create(&doc).context("Failed to create document")?;
        index
            .index_document(&doc)
            .context("Failed to index document")?;
        return Ok(doc_id);
    }

    // Fall back to creating a new document with the content as body
    // Extract title from first heading or filename
    let title = content
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    let profile = DecayProfile::default_profile();
    let counter =
        mkb_vault::next_counter(vault_path, default_doc_type, &mkb_vault::slugify(&title));
    let id = Document::generate_id(default_doc_type, &title, counter);

    let input = RawTemporalInput {
        observed_at: Some(Utc::now()),
        valid_until: None,
        temporal_precision: Some(TemporalPrecision::Day),
        occurred_at: None,
    };

    let mut doc = Document::new(id, default_doc_type.to_string(), title, input, &profile)
        .context("Temporal gate rejected document")?;
    doc.body = content.to_string();

    let doc_id = doc.id.clone();
    vault.create(&doc).context("Failed to create document")?;
    index
        .index_document(&doc)
        .context("Failed to index document")?;

    Ok(doc_id)
}

// === View ===

fn cmd_view_save(
    vault_path: &Path,
    name: &str,
    mkql: &str,
    description: Option<&str>,
) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;

    // Validate the query parses
    mkb_parser::parse_mkql(mkql).map_err(|e| anyhow::anyhow!("Invalid MKQL: {e}"))?;

    let view = mkb_core::view::SavedView {
        name: name.to_string(),
        description: description.map(|s| s.to_string()),
        query: mkql.to_string(),
        created_at: Utc::now().to_rfc3339(),
    };

    let path = vault
        .save_view(&view)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let output = serde_json::json!({
        "name": name,
        "query": mkql,
        "path": path.display().to_string(),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn cmd_view_list(vault_path: &Path) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;

    let names = vault
        .list_views()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut views = Vec::new();
    for name in &names {
        if let Ok(view) = vault.load_view(name) {
            views.push(serde_json::json!({
                "name": view.name,
                "query": view.query,
                "description": view.description,
                "created_at": view.created_at,
            }));
        }
    }

    println!("{}", serde_json::to_string_pretty(&views)?);
    Ok(())
}

fn cmd_view_run(vault_path: &Path, name: &str, format: &str) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;

    let view = vault
        .load_view(name)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    cmd_query(vault_path, Some(&view.query), None, None, format)
}

fn cmd_view_delete(vault_path: &Path, name: &str) -> Result<()> {
    let vault = Vault::open(vault_path).context("Failed to open vault")?;

    vault
        .delete_view(name)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let output = serde_json::json!({
        "name": name,
        "deleted": true,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// === Helpers ===

fn open_index(vault_path: &Path) -> Result<IndexManager> {
    let index_path = vault_path.join(".mkb").join("index").join("mkb.db");
    IndexManager::open(&index_path).context("Failed to open index")
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

fn parse_format(s: &str) -> Result<OutputFormat> {
    match s.to_lowercase().as_str() {
        "json" => Ok(OutputFormat::Json),
        "table" => Ok(OutputFormat::Table),
        "markdown" | "md" => Ok(OutputFormat::Markdown),
        other => anyhow::bail!("Unknown format '{}'. Valid: json, table, markdown", other),
    }
}

fn print_indexed_docs(results: &[mkb_index::IndexedDocument]) -> Result<()> {
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
    Ok(())
}
