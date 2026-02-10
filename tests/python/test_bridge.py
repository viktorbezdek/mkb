"""Tests for the PyO3 bridge (mkb._mkb_core).

Validates GATE-4a and GATE-4b: Python can import and use all Rust functions
including vault CRUD, index operations, temporal gate, and vector search.
"""

from __future__ import annotations

import hashlib
import json
import struct
import tempfile
from pathlib import Path

import mkb

# === T-400.1: Vault CRUD ===


class TestVaultCRUD:
    """Vault create/read/delete operations through PyO3 bridge."""

    def test_init_vault(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            result = mkb.init_vault(d)
            assert Path(result).exists()
            assert (Path(d) / ".mkb").is_dir()
            assert (Path(d) / ".mkb" / "index" / "mkb.db").exists()

    def test_create_document(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            doc = mkb.create_document(
                d, "project", "Alpha Project", "2025-02-10T00:00:00Z"
            )
            assert doc["title"] == "Alpha Project"
            assert doc["type"] == "project"
            assert doc["id"].startswith("proj-")
            assert "observed_at" in doc
            assert "valid_until" in doc

    def test_create_document_with_body_and_tags(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            doc = mkb.create_document(
                d,
                "project",
                "Beta Project",
                "2025-02-10T00:00:00Z",
                body="Project body content",
                tags=["rust", "test"],
            )
            assert doc["body"] == "Project body content"
            assert doc["tags"] == ["rust", "test"]

    def test_read_document(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            created = mkb.create_document(
                d, "project", "Read Test", "2025-02-10T00:00:00Z"
            )
            doc = mkb.read_document(d, "project", created["id"])
            assert doc["id"] == created["id"]
            assert doc["title"] == "Read Test"

    def test_delete_document(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            created = mkb.create_document(
                d, "project", "Delete Me", "2025-02-10T00:00:00Z"
            )
            archive_path = mkb.delete_document(d, "project", created["id"])
            assert "archive" in archive_path


# === T-400.2: Index Operations ===


class TestIndexOperations:
    """Search and query operations through PyO3 bridge."""

    def test_search_fts(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            mkb.create_document(
                d,
                "project",
                "ML Project",
                "2025-02-10T00:00:00Z",
                body="Machine learning and neural networks",
            )
            results = mkb.search_fts(d, "machine learning")
            assert len(results) >= 1
            assert results[0]["title"] == "ML Project"

    def test_query_mkql_json(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            mkb.create_document(
                d, "project", "Alpha", "2025-02-10T00:00:00Z"
            )
            mkb.create_document(
                d, "project", "Beta", "2025-02-10T00:00:00Z"
            )
            result = mkb.query_mkql(d, "SELECT * FROM project")
            parsed = json.loads(result)
            assert len(parsed) == 2

    def test_query_mkql_table_format(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            mkb.create_document(
                d, "project", "Alpha", "2025-02-10T00:00:00Z"
            )
            result = mkb.query_mkql(d, "SELECT * FROM project", format="table")
            assert "---" in result  # Table separator

    def test_query_all(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            mkb.create_document(
                d, "project", "P1", "2025-02-10T00:00:00Z"
            )
            mkb.create_document(
                d, "meeting", "M1", "2025-02-10T00:00:00Z"
            )
            results = mkb.query_all(d)
            assert len(results) == 2

    def test_query_by_type(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            mkb.create_document(
                d, "project", "P1", "2025-02-10T00:00:00Z"
            )
            mkb.create_document(
                d, "meeting", "M1", "2025-02-10T00:00:00Z"
            )
            results = mkb.query_by_type(d, "project")
            assert len(results) == 1
            assert results[0]["type"] == "project"

    def test_document_count(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            assert mkb.document_count(d) == 0
            mkb.create_document(
                d, "project", "P1", "2025-02-10T00:00:00Z"
            )
            assert mkb.document_count(d) == 1


# === T-400.3: Temporal Gate ===


class TestTemporalGate:
    """Temporal validation through PyO3 bridge."""

    def test_validate_temporal_valid(self) -> None:
        result = mkb.validate_temporal(observed_at="2025-02-10T00:00:00Z")
        assert result["valid"] is True
        assert "observed_at" in result
        assert "valid_until" in result

    def test_validate_temporal_no_observed_at(self) -> None:
        result = mkb.validate_temporal()
        assert result["valid"] is False
        assert "error" in result

    def test_validate_temporal_with_precision(self) -> None:
        result = mkb.validate_temporal(
            observed_at="2025-02-10T00:00:00Z", precision="month"
        )
        assert result["valid"] is True
        assert result["temporal_precision"] == "month"

    def test_validate_temporal_invalid_precision(self) -> None:
        try:
            mkb.validate_temporal(
                observed_at="2025-02-10T00:00:00Z", precision="bogus"
            )
            msg = "Should have raised ValueError"
            raise AssertionError(msg)
        except ValueError:
            pass


# === Utility ===


class TestUtility:
    """Vault status and metadata operations."""

    def test_vault_status(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            mkb.create_document(
                d, "project", "P1", "2025-02-10T00:00:00Z"
            )
            status = mkb.vault_status(d)
            assert status["indexed_documents"] == 1
            assert status["vault_files"] == 1
            assert status["index_synced"] is True
            assert status["rejection_count"] == 0

    def test_version_exposed(self) -> None:
        assert mkb.__version__ == "0.2.0"


# === T-410: Embedding / Vector Search ===


def _test_embedding(seed: str) -> list[float]:
    """Generate a deterministic test embedding from a seed string."""
    dim = mkb.embedding_dim()
    vec = []
    for i in range(dim):
        h = hashlib.sha256(f"{seed}-{i}".encode()).digest()
        val = struct.unpack("f", h[:4])[0]
        # Clamp to reasonable range
        val = max(-1.0, min(1.0, val / 1e38))
        vec.append(val)
    # Normalize
    norm = sum(v * v for v in vec) ** 0.5
    if norm > 0:
        vec = [v / norm for v in vec]
    return vec


class TestEmbeddings:
    """Vector embedding storage and semantic search through PyO3 bridge."""

    def test_embedding_dim(self) -> None:
        assert mkb.embedding_dim() == 1536

    def test_store_and_check_embedding(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            doc = mkb.create_document(
                d, "project", "Alpha", "2025-02-10T00:00:00Z"
            )
            doc_id = doc["id"]

            assert not mkb.has_embedding(d, doc_id)

            emb = _test_embedding("alpha")
            mkb.store_embedding(d, doc_id, emb, "test-model")

            assert mkb.has_embedding(d, doc_id)
            assert mkb.embedding_count(d) == 1

    def test_semantic_search(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)

            ids = []
            for name in ["Alpha", "Beta", "Gamma"]:
                doc = mkb.create_document(
                    d, "project", name, "2025-02-10T00:00:00Z"
                )
                doc_id = doc["id"]
                ids.append(doc_id)
                mkb.store_embedding(d, doc_id, _test_embedding(name), "test-model")

            # Search with Alpha's embedding — should return Alpha first
            results = mkb.search_semantic(d, _test_embedding("Alpha"), limit=3)
            assert len(results) == 3
            assert results[0]["id"] == ids[0]
            assert results[0]["distance"] < results[1]["distance"]

    def test_embedding_dimension_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            mkb.init_vault(d)
            doc = mkb.create_document(
                d, "project", "Alpha", "2025-02-10T00:00:00Z"
            )
            try:
                mkb.store_embedding(d, doc["id"], [0.0] * 768, "bad-model")
                msg = "Should have raised ValueError"
                raise AssertionError(msg)
            except ValueError:
                pass
