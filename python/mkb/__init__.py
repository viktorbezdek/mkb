"""MKB — Markdown Knowledge Base for LLMs.

Python interface to the Rust-powered MKB core. All vault operations
go through the native `_mkb_core` extension module built via PyO3.
"""

from mkb._mkb_core import (  # type: ignore[import-untyped]
    __version__,
    create_document,
    delete_document,
    document_count,
    embedding_count,
    embedding_dim,
    has_embedding,
    init_vault,
    query_all,
    query_by_type,
    query_mkql,
    read_document,
    search_fts,
    search_semantic,
    store_embedding,
    validate_temporal,
    vault_status,
)

__all__ = [
    "__version__",
    "init_vault",
    "create_document",
    "read_document",
    "delete_document",
    "search_fts",
    "search_semantic",
    "store_embedding",
    "has_embedding",
    "embedding_count",
    "embedding_dim",
    "query_mkql",
    "query_all",
    "query_by_type",
    "validate_temporal",
    "document_count",
    "vault_status",
]
