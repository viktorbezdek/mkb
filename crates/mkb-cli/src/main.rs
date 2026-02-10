//! MKB CLI — Markdown Knowledge Base for LLMs
//!
//! Commands: init, add, edit, rm, query, search, ingest, link,
//! schema, index, sync, export, gc, serve, repl, stats, report

use clap::Parser;

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
    Init,
    /// Create a new knowledge document
    Add,
    /// Execute an MKQL query
    #[command(alias = "q")]
    Query,
    /// Quick full-text or semantic search
    #[command(alias = "s")]
    Search,
    /// Vault statistics
    Stats,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            println!("mkb init — not yet implemented (Phase 4)");
        }
        Some(Commands::Add) => {
            println!("mkb add — not yet implemented (Phase 4)");
        }
        Some(Commands::Query) => {
            println!("mkb query — not yet implemented (Phase 4)");
        }
        Some(Commands::Search) => {
            println!("mkb search — not yet implemented (Phase 4)");
        }
        Some(Commands::Stats) => {
            println!("mkb stats — not yet implemented (Phase 4)");
        }
        None => {
            println!(
                "MKB v{} — Markdown Knowledge Base for LLMs",
                env!("CARGO_PKG_VERSION")
            );
            println!("Run `mkb --help` for usage.");
        }
    }
}
