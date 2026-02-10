"""Tests for the AI-enriched ingestion pipeline.

Validates T-420.4: ingest with AI enrichment, directory batch, dry run.
"""

from __future__ import annotations

import tempfile
from pathlib import Path

import mkb
from mkb_ai.ingestion import IngestPipeline


def _setup_vault() -> str:
    """Create an empty vault, return path."""
    d = tempfile.mkdtemp()
    mkb.init_vault(d)
    return d


class TestIngestText:
    """Ingest raw text with extraction enrichment."""

    def test_ingest_with_explicit_date(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d, doc_type="document")
        result = pipeline.ingest_text(
            "Some meeting notes about the project.",
            title="Meeting Notes",
            observed_at="2025-02-10T00:00:00Z",
        )
        assert result.doc_id.startswith("docu-")
        assert result.title == "Meeting Notes"
        assert result.observed_at == "2025-02-10T00:00:00Z"
        assert mkb.document_count(d) == 1

    def test_ingest_extracts_dates(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d)
        result = pipeline.ingest_text(
            "Sprint review on 2025-03-15 was productive."
        )
        assert result.extracted_dates >= 1

    def test_ingest_extracts_entities(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d)
        result = pipeline.ingest_text(
            "Fixed PROJ-123 by @jane, see https://example.com"
        )
        assert result.extracted_entities >= 3  # ticket, mention, url

    def test_ingest_uses_extracted_date_as_observed_at(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d)
        result = pipeline.ingest_text(
            "Decision made on 2025-01-20 to use Rust.",
            title="Decision Record",
        )
        # Should use extracted date as observed_at
        assert "2025-01-20" in result.observed_at

    def test_ingest_extracts_title_from_heading(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d)
        result = pipeline.ingest_text(
            "# Sprint Retrospective\n\nWent well: everything.",
            observed_at="2025-02-10T00:00:00Z",
        )
        assert result.title == "Sprint Retrospective"

    def test_ingest_with_embedding(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d, embed=True)
        result = pipeline.ingest_text(
            "Machine learning pipeline for text classification.",
            title="ML Pipeline",
            observed_at="2025-02-10T00:00:00Z",
        )
        assert result.embedded
        assert mkb.has_embedding(d, result.doc_id)


class TestIngestFile:
    """Ingest files from disk."""

    def test_ingest_single_file(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d)

        # Create a test file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".md", delete=False
        ) as f:
            f.write("# Project Plan\n\nTimeline starts 2025-03-01.\n")
            file_path = f.name

        result = pipeline.ingest_file(file_path)
        assert result.title == "Project Plan"
        assert result.extracted_dates >= 1

    def test_ingest_directory(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d)

        # Create a temp directory with markdown files
        with tempfile.TemporaryDirectory() as td:
            for name in ["alpha.md", "beta.md", "gamma.md"]:
                Path(td, name).write_text(
                    f"# {name[:-3].title()}\n\nContent for {name}.\n"
                )
            Path(td, "readme.txt").write_text("Not a markdown file.\n")

            results = pipeline.ingest_directory(td)
            assert len(results) == 3
            assert mkb.document_count(d) == 3


class TestDryRun:
    """Preview extraction without creating documents."""

    def test_dry_run_extracts_without_creating(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d)
        preview = pipeline.dry_run(
            "Meeting with @alice about PROJ-456 on 2025-04-01."
        )
        assert preview["title"] is not None
        assert len(preview["dates"]) >= 1  # type: ignore[arg-type]
        assert len(preview["entities"]) >= 2  # type: ignore[arg-type]
        assert preview["confidence"] > 0
        # Nothing should be created
        assert mkb.document_count(d) == 0

    def test_dry_run_shows_tags(self) -> None:
        d = _setup_vault()
        pipeline = IngestPipeline(vault_path=d)
        preview = pipeline.dry_run(
            "Assigned JIRA-789 to @bob at https://example.com"
        )
        tags = preview["tags"]
        assert isinstance(tags, list)
        assert len(tags) >= 2
