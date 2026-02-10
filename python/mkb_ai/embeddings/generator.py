"""Embedding generation for MKB documents.

Uses OpenAI text-embedding-3-small (1536 dimensions) by default.
Provides both async and sync interfaces, with a mock backend for testing.
"""

from __future__ import annotations

from typing import Protocol

import mkb


class EmbeddingBackend(Protocol):
    """Protocol for embedding generation backends."""

    def generate(self, text: str) -> list[float]:
        """Generate an embedding vector for the given text."""
        ...

    @property
    def model_name(self) -> str:
        """Return the model name for metadata tracking."""
        ...

    @property
    def dimensions(self) -> int:
        """Return the embedding dimensionality."""
        ...


class OpenAIEmbeddingBackend:
    """OpenAI text-embedding-3-small backend."""

    def __init__(self, api_key: str | None = None) -> None:
        import openai

        self._client = openai.OpenAI(api_key=api_key)
        self._model = "text-embedding-3-small"

    def generate(self, text: str) -> list[float]:
        """Generate embedding using OpenAI API."""
        response = self._client.embeddings.create(
            input=text,
            model=self._model,
        )
        return response.data[0].embedding

    @property
    def model_name(self) -> str:
        return self._model

    @property
    def dimensions(self) -> int:
        return 1536


class MockEmbeddingBackend:
    """Deterministic mock backend for testing (no API calls)."""

    def __init__(self) -> None:
        self._model = "mock-embedding"

    def generate(self, text: str) -> list[float]:
        """Generate a deterministic embedding from text hash."""
        import hashlib
        import struct

        dim = mkb.embedding_dim()
        vec: list[float] = []
        for i in range(dim):
            h = hashlib.sha256(f"{text}-{i}".encode()).digest()
            val = struct.unpack("f", h[:4])[0]
            val = max(-1.0, min(1.0, val / 1e38))
            vec.append(val)
        # Normalize
        norm = sum(v * v for v in vec) ** 0.5
        if norm > 0:
            vec = [v / norm for v in vec]
        return vec

    @property
    def model_name(self) -> str:
        return self._model

    @property
    def dimensions(self) -> int:
        return mkb.embedding_dim()


class EmbeddingGenerator:
    """High-level embedding manager for MKB vaults.

    Generates embeddings for documents and stores them in the vault's
    sqlite-vec index for semantic search.
    """

    def __init__(
        self,
        vault_path: str,
        backend: EmbeddingBackend | None = None,
    ) -> None:
        self.vault_path = vault_path
        self.backend = backend or MockEmbeddingBackend()

    def embed_document(self, doc_id: str) -> None:
        """Generate and store an embedding for a document."""
        doc = mkb.read_document(
            self.vault_path,
            _doc_type_from_id(doc_id),
            doc_id,
        )
        text = _document_to_text(doc)
        embedding = self.backend.generate(text)
        mkb.store_embedding(
            self.vault_path, doc_id, embedding, self.backend.model_name
        )

    def embed_all(self) -> int:
        """Embed all documents that don't have embeddings yet.

        Returns the number of documents embedded.
        """
        docs = mkb.query_all(self.vault_path)
        count = 0
        for doc in docs:
            doc_id = doc["id"]
            if not mkb.has_embedding(self.vault_path, doc_id):
                self.embed_document(doc_id)
                count += 1
        return count

    def search(self, query: str, limit: int = 10) -> list[dict[str, object]]:
        """Semantic search: embed the query and find similar documents."""
        query_embedding = self.backend.generate(query)
        return mkb.search_semantic(self.vault_path, query_embedding, limit=limit)


def _document_to_text(doc: dict[str, object]) -> str:
    """Convert a document dict to text for embedding generation."""
    parts: list[str] = []
    title = doc.get("title", "")
    if title:
        parts.append(f"Title: {title}")
    body = doc.get("body", "")
    if body:
        parts.append(str(body))
    tags = doc.get("tags", [])
    if tags:
        parts.append(f"Tags: {', '.join(str(t) for t in tags)}")  # type: ignore[union-attr]
    return "\n".join(parts)


def _doc_type_from_id(doc_id: str) -> str:
    """Infer document type from ID prefix (e.g., proj-foo-001 -> project)."""
    prefix_map = {
        "proj": "project",
        "meet": "meeting",
        "deci": "decision",
        "sign": "signal",
        "doc": "document",
    }
    prefix = doc_id.split("-")[0] if "-" in doc_id else doc_id
    return prefix_map.get(prefix, prefix)


def embed_document(vault_path: str, doc_id: str, backend: EmbeddingBackend | None = None) -> None:
    """Convenience function to embed a single document."""
    gen = EmbeddingGenerator(vault_path, backend)
    gen.embed_document(doc_id)


def embed_query(query: str, backend: EmbeddingBackend | None = None) -> list[float]:
    """Convenience function to embed a query string."""
    b = backend or MockEmbeddingBackend()
    return b.generate(query)
