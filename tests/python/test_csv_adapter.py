"""Tests for CSV ingestion adapter.

Validates Phase 6d.2: CSV parsing, date column detection, observed_at extraction,
fallback to now(), and custom column mapping.
"""

from __future__ import annotations

import csv
import tempfile

import mkb
from mkb_ai.ingestion.csv_adapter import CsvAdapter, CsvColumnMapping


def _setup_vault() -> str:
    """Create an empty vault, return path."""
    d = tempfile.mkdtemp()
    mkb.init_vault(d)
    return d


class TestCsvIngestBasic:
    """Basic CSV ingestion into documents."""

    def test_csv_ingest_basic(self) -> None:
        d = _setup_vault()
        adapter = CsvAdapter(vault_path=d, doc_type="document")

        csv_path = _write_csv(
            ["title", "status", "notes"],
            [
                ["Alpha Project", "active", "First project notes"],
                ["Beta Project", "on-hold", "Second project notes"],
            ],
        )

        results = adapter.ingest_csv(csv_path)
        assert len(results) == 2
        assert mkb.document_count(d) == 2
        assert results[0].title == "Alpha Project"
        assert results[1].title == "Beta Project"

    def test_csv_ingest_with_headers(self) -> None:
        d = _setup_vault()
        adapter = CsvAdapter(vault_path=d)

        csv_path = _write_csv(
            ["name", "description", "priority"],
            [
                ["Task A", "Do something", "high"],
                ["Task B", "Do another thing", "low"],
                ["Task C", "One more", "medium"],
            ],
        )

        results = adapter.ingest_csv(csv_path)
        assert len(results) == 3
        for r in results:
            assert r.doc_id.startswith("docu-")


class TestCsvDateColumnDetection:
    """Auto-detect date columns for observed_at."""

    def test_csv_date_column_detection(self) -> None:
        d = _setup_vault()
        adapter = CsvAdapter(vault_path=d)

        csv_path = _write_csv(
            ["title", "created_date", "status"],
            [
                ["Item A", "2025-03-15", "open"],
                ["Item B", "2025-04-01", "closed"],
            ],
        )

        results = adapter.ingest_csv(csv_path)
        assert len(results) == 2
        assert "2025-03-15" in results[0].observed_at
        assert "2025-04-01" in results[1].observed_at

    def test_csv_observed_at_from_date_column(self) -> None:
        d = _setup_vault()
        adapter = CsvAdapter(vault_path=d)

        csv_path = _write_csv(
            ["title", "observed_at", "body"],
            [
                ["Doc One", "2025-01-10T12:00:00Z", "First document body"],
            ],
        )

        results = adapter.ingest_csv(csv_path)
        assert len(results) == 1
        assert "2025-01-10" in results[0].observed_at

    def test_csv_detects_various_date_formats(self) -> None:
        d = _setup_vault()
        adapter = CsvAdapter(vault_path=d)

        csv_path = _write_csv(
            ["title", "date", "value"],
            [
                ["Row 1", "2025-06-15", "100"],
                ["Row 2", "01/20/2025", "200"],
                ["Row 3", "March 5, 2025", "300"],
            ],
        )

        results = adapter.ingest_csv(csv_path)
        assert len(results) == 3
        # All should have dates extracted (not fallback to now)
        for r in results:
            assert "2025" in r.observed_at


class TestCsvNoDateFallback:
    """When no date column exists, fall back to now()."""

    def test_csv_with_no_dates(self) -> None:
        d = _setup_vault()
        adapter = CsvAdapter(vault_path=d)

        csv_path = _write_csv(
            ["name", "category", "value"],
            [
                ["Widget", "hardware", "42"],
                ["Gadget", "software", "99"],
            ],
        )

        results = adapter.ingest_csv(csv_path)
        assert len(results) == 2
        # Should still have observed_at (fallback to now)
        for r in results:
            assert r.observed_at != ""


class TestCsvCustomMapping:
    """User-specified column-to-field mapping."""

    def test_csv_custom_mapping(self) -> None:
        d = _setup_vault()
        mapping = CsvColumnMapping(
            title_column="project_name",
            date_column="start_date",
            body_columns=["description", "notes"],
        )
        adapter = CsvAdapter(vault_path=d, mapping=mapping)

        csv_path = _write_csv(
            ["project_name", "start_date", "description", "notes", "owner"],
            [
                ["Alpha", "2025-05-01", "Main project", "Important notes", "alice"],
            ],
        )

        results = adapter.ingest_csv(csv_path)
        assert len(results) == 1
        assert results[0].title == "Alpha"
        assert "2025-05-01" in results[0].observed_at

    def test_csv_custom_body_columns_concatenated(self) -> None:
        d = _setup_vault()
        mapping = CsvColumnMapping(
            title_column="name",
            body_columns=["summary", "details"],
        )
        adapter = CsvAdapter(vault_path=d, mapping=mapping)

        csv_path = _write_csv(
            ["name", "summary", "details"],
            [
                ["Test Item", "Short summary here", "Longer details section"],
            ],
        )

        results = adapter.ingest_csv(csv_path)
        assert len(results) == 1
        # Verify the body contains both columns by reading the document
        doc = mkb.read_document(d, "document", results[0].doc_id)
        body = doc.get("body", "")
        assert "Short summary here" in body
        assert "Longer details section" in body


# ---- Helpers ----


def _write_csv(headers: list[str], rows: list[list[str]]) -> str:
    """Write a CSV file to a temp path and return the path."""
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".csv", delete=False, newline=""
    ) as f:
        writer = csv.writer(f)
        writer.writerow(headers)
        writer.writerows(rows)
        return f.name
