//! # mkb-index
//!
//! SQLite indexer with FTS5 full-text search for MKB.
//!
//! Maintains a derived index from vault markdown files:
//! - Field index (EAV pattern) for structured queries
//! - FTS5 full-text index for content search
//! - Link index for graph traversal
//! - Temporal indexes for time-based queries

/// Placeholder for index manager implementation.
/// Implementation will be added in Phase 2 (T-210.x).

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        // Index tests will be added in Phase 2 (T-210.1+)
    }
}
