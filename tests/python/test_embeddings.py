"""Tests for the embedding generation and semantic search pipeline.

Validates T-410.1 through T-410.4 using mock embeddings (no API calls).
"""

from __future__ import annotations

import tempfile

import mkb
from mkb_ai.embeddings import EmbeddingGenerator, embed_document, embed_query
from mkb_ai.embeddings.generator import MockEmbeddingBackend


def _setup_vault() -> tuple[str, list[str]]:
    """Create a vault with 3 documents, return (path, doc_ids)."""
    d = tempfile.mkdtemp()
    mkb.init_vault(d)
    ids = []
    for name, body in [
        ("ML Pipeline", "Machine learning model training with PyTorch"),
        ("Web Server", "HTTP API server built with Rust and Actix"),
        ("Data Analysis", "Statistical analysis of neural network performance"),
    ]:
        doc = mkb.create_document(d, "project", name, "2025-02-10T00:00:00Z", body=body)
        ids.append(doc["id"])
    return d, ids


# === T-410.1: Embedding generation ===


class TestEmbeddingGeneration:
    """Mock embedding backend produces correct dimensions."""

    def test_mock_generates_correct_dimensions(self) -> None:
        backend = MockEmbeddingBackend()
        emb = backend.generate("test text")
        assert len(emb) == mkb.embedding_dim()
        assert len(emb) == 1536

    def test_mock_is_deterministic(self) -> None:
        backend = MockEmbeddingBackend()
        emb1 = backend.generate("same text")
        emb2 = backend.generate("same text")
        assert emb1 == emb2

    def test_mock_different_text_different_embedding(self) -> None:
        backend = MockEmbeddingBackend()
        emb1 = backend.generate("text one")
        emb2 = backend.generate("text two")
        assert emb1 != emb2

    def test_mock_embeddings_are_normalized(self) -> None:
        backend = MockEmbeddingBackend()
        emb = backend.generate("normalize test")
        norm = sum(v * v for v in emb) ** 0.5
        assert abs(norm - 1.0) < 0.01

    def test_model_name(self) -> None:
        backend = MockEmbeddingBackend()
        assert backend.model_name == "mock-embedding"


# === T-410.2: sqlite-vec index management ===


class TestEmbeddingStorage:
    """Embedding storage and retrieval through the full pipeline."""

    def test_embed_single_document(self) -> None:
        d, ids = _setup_vault()
        gen = EmbeddingGenerator(d)
        gen.embed_document(ids[0])
        assert mkb.has_embedding(d, ids[0])
        assert mkb.embedding_count(d) == 1

    def test_embed_all_documents(self) -> None:
        d, ids = _setup_vault()
        gen = EmbeddingGenerator(d)
        count = gen.embed_all()
        assert count == 3
        for doc_id in ids:
            assert mkb.has_embedding(d, doc_id)

    def test_embed_all_skips_already_embedded(self) -> None:
        d, ids = _setup_vault()
        gen = EmbeddingGenerator(d)
        gen.embed_document(ids[0])
        count = gen.embed_all()
        assert count == 2  # Only 2 new, not 3


# === T-410.3: Semantic search with sqlite-vec ===


class TestSemanticSearch:
    """NEAR() style semantic search returns relevant documents."""

    def test_search_finds_relevant_docs(self) -> None:
        d, ids = _setup_vault()
        gen = EmbeddingGenerator(d)
        gen.embed_all()

        # Search for ML-related content — should return ML Pipeline first
        results = gen.search("machine learning model training")
        assert len(results) >= 1
        assert results[0]["id"] == ids[0]  # ML Pipeline

    def test_search_respects_limit(self) -> None:
        d, _ids = _setup_vault()
        gen = EmbeddingGenerator(d)
        gen.embed_all()

        results = gen.search("project", limit=2)
        assert len(results) == 2

    def test_search_returns_distance_scores(self) -> None:
        d, _ids = _setup_vault()
        gen = EmbeddingGenerator(d)
        gen.embed_all()

        results = gen.search("data analysis statistics")
        assert len(results) >= 1
        assert "distance" in results[0]
        # Distances should be ordered (nearest first)
        if len(results) >= 2:
            assert results[0]["distance"] <= results[1]["distance"]


# === T-410.4: Convenience functions ===


class TestConvenienceFunctions:
    """Top-level embed_document and embed_query functions."""

    def test_embed_document_function(self) -> None:
        d, ids = _setup_vault()
        embed_document(d, ids[0])
        assert mkb.has_embedding(d, ids[0])

    def test_embed_query_function(self) -> None:
        emb = embed_query("test query")
        assert len(emb) == mkb.embedding_dim()
