"""Ingestion pipeline with AI enrichment.

Processes raw text/files into MKB documents with:
- Date extraction for observed_at inference
- Entity extraction for tags and links
- Confidence scoring
- Optional embedding generation
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

import mkb
from mkb_ai.confidence.scorer import ConfidenceScorer
from mkb_ai.embeddings.generator import EmbeddingBackend, EmbeddingGenerator
from mkb_ai.extraction.dates import DateExtractor
from mkb_ai.extraction.entities import EntityExtractor


@dataclass
class IngestResult:
    """Result of ingesting one item."""

    doc_id: str
    title: str
    observed_at: str
    confidence: float
    extracted_dates: int = 0
    extracted_entities: int = 0
    embedded: bool = False


@dataclass
class IngestPipeline:
    """Enriched ingestion pipeline for MKB vaults.

    Extracts dates, entities, and confidence scores from raw text,
    then creates documents with enriched metadata.
    """

    vault_path: str
    doc_type: str = "document"
    embed: bool = False
    embedding_backend: EmbeddingBackend | None = None
    _date_extractor: DateExtractor = field(default_factory=DateExtractor)
    _entity_extractor: EntityExtractor = field(default_factory=EntityExtractor)
    _scorer: ConfidenceScorer = field(default_factory=ConfidenceScorer)

    def ingest_text(
        self,
        text: str,
        title: str | None = None,
        observed_at: str | None = None,
        source: str = "import",
    ) -> IngestResult:
        """Ingest raw text with AI enrichment.

        If observed_at is not provided, attempts to extract a date from the text.
        """
        # Extract title from first heading if not provided
        if title is None:
            title = _extract_title(text)

        # Extract dates
        dates = self._date_extractor.extract(text)
        extracted_dates = len(dates)

        # Determine observed_at
        if observed_at is not None:
            obs_at = observed_at
        elif dates:
            # Use the first extracted date
            obs_at = dates[0].value.isoformat()
        else:
            # Fall back to now
            obs_at = datetime.now(timezone.utc).isoformat()

        # Extract entities
        entities = self._entity_extractor.extract(text)
        extracted_entities = len(entities)

        # Build tags from entities
        tags = _entities_to_tags(entities)

        # Score confidence
        breakdown = self._scorer.score(
            source=source,
            precision="day",
            has_body=bool(text.strip()),
            has_tags=bool(tags),
            has_links=False,
        )

        # Create the document
        doc = mkb.create_document(
            self.vault_path,
            self.doc_type,
            title,
            obs_at,
            body=text,
            tags=tags if tags else None,
        )

        doc_id = doc["id"]
        embedded = False

        # Optionally generate embedding
        if self.embed:
            gen = EmbeddingGenerator(self.vault_path, self.embedding_backend)
            gen.embed_document(doc_id)
            embedded = True

        return IngestResult(
            doc_id=doc_id,
            title=title,
            observed_at=obs_at,
            confidence=breakdown.final_score,
            extracted_dates=extracted_dates,
            extracted_entities=extracted_entities,
            embedded=embedded,
        )

    def ingest_file(self, file_path: str | Path) -> IngestResult:
        """Ingest a file with AI enrichment."""
        path = Path(file_path)
        text = path.read_text()
        title = _extract_title(text) or path.stem
        return self.ingest_text(text, title=title, source="import")

    def ingest_directory(
        self, dir_path: str | Path, pattern: str = "*.md"
    ) -> list[IngestResult]:
        """Ingest all matching files in a directory."""
        path = Path(dir_path)
        results: list[IngestResult] = []
        for file_path in sorted(path.glob(pattern)):
            result = self.ingest_file(file_path)
            results.append(result)
        return results

    def dry_run(self, text: str, title: str | None = None) -> dict[str, object]:
        """Preview what would be extracted without creating a document."""
        if title is None:
            title = _extract_title(text)

        dates = self._date_extractor.extract(text)
        entities = self._entity_extractor.extract(text)
        tags = _entities_to_tags(entities)

        breakdown = self._scorer.score(
            source="import",
            precision="day",
            has_body=bool(text.strip()),
            has_tags=bool(tags),
        )

        return {
            "title": title,
            "dates": [
                {"value": d.value.isoformat(), "text": d.original_text, "relative": d.is_relative}
                for d in dates
            ],
            "entities": [
                {"kind": e.kind, "value": e.value}
                for e in entities
            ],
            "tags": tags,
            "confidence": breakdown.final_score,
        }


def _extract_title(text: str) -> str:
    """Extract title from first markdown heading or first line."""
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("# "):
            return stripped[2:].strip()
    # Fall back to first non-empty line
    for line in text.splitlines():
        if line.strip():
            return line.strip()[:80]
    return "Untitled"


def _entities_to_tags(entities: list[object]) -> list[str]:
    """Convert extracted entities to tags."""
    tags: list[str] = []
    seen: set[str] = set()
    for e in entities:
        entity = e  # type: ignore[assignment]
        tag = f"{entity.kind}:{entity.value}"  # type: ignore[union-attr]
        if tag not in seen:
            tags.append(tag)
            seen.add(tag)
    return tags
