//! # mkb-parser
//!
//! MKQL (Markdown Knowledge Query Language) parser using pest PEG grammar.
//!
//! Parses MKQL query strings into an AST that can be compiled to
//! SQLite SQL + vector index queries by the mkb-query crate.

pub mod ast;

/// Placeholder for the MKQL pest grammar parser.
/// The grammar file will be at `src/mkql.pest`.
///
/// Implementation will be added in Phase 3 (T-300.x).

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        // Parser tests will be added in Phase 3 (T-300.1+)
    }
}
