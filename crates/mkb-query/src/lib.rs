//! # mkb-query
//!
//! Query engine for MKB. Compiles MKQL AST into execution plans
//! that combine SQLite queries with vector similarity search.
//!
//! Includes:
//! - MKQL-to-SQL compiler
//! - Result formatter (JSON, Table, Markdown, Context)
//! - Context assembler for LLM token budgets

mod compiler;
mod context;
mod executor;
mod formatter;
pub mod graph;

pub use compiler::{compile, CompiledQuery};
pub use context::{ContextAssembler, ContextOpts};
pub use executor::execute;
pub use formatter::{format_results, OutputFormat, QueryResult, ResultRow};
