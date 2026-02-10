"""CSV ingestion adapter with date column auto-detection.

Parses CSV files, detects date-like columns, maps each row to an MKB
document with title from the first text column, body from remaining
columns, and observed_at from detected/specified date columns.
"""

from __future__ import annotations

import csv
import re
from collections.abc import Sequence
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path

import mkb

from mkb_ai.ingestion.pipeline import IngestResult

# Patterns for detecting date-like values in cells
_ISO_DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2})?")
_SLASH_DATE_RE = re.compile(r"^\d{1,2}/\d{1,2}/\d{4}$")
_WRITTEN_DATE_RE = re.compile(
    r"^(?:January|February|March|April|May|June|July|August|September|October|November|December)"
    r"\s+\d{1,2},?\s+\d{4}$",
    re.IGNORECASE,
)

# Column names that hint at date content
_DATE_COLUMN_HINTS = {
    "date",
    "created",
    "created_at",
    "created_date",
    "updated",
    "updated_at",
    "modified",
    "modified_at",
    "observed_at",
    "timestamp",
    "time",
    "datetime",
    "occurred_at",
    "start_date",
    "end_date",
    "due_date",
}


@dataclass
class CsvColumnMapping:
    """User-specified column-to-field mapping."""

    title_column: str | None = None
    date_column: str | None = None
    body_columns: list[str] = field(default_factory=list)


@dataclass
class CsvAdapter:
    """Ingests CSV files into MKB documents.

    Auto-detects date columns for observed_at, or uses explicit mapping.
    """

    vault_path: str
    doc_type: str = "document"
    mapping: CsvColumnMapping | None = None

    def ingest_csv(self, csv_path: str | Path) -> list[IngestResult]:
        """Ingest all rows from a CSV file as documents.

        Returns a list of IngestResult, one per row.
        """
        path = Path(csv_path)
        with path.open(newline="") as f:
            reader = csv.DictReader(f)
            headers = reader.fieldnames or []
            rows = list(reader)

        if not rows or not headers:
            return []

        # Resolve column mapping
        title_col = self._resolve_title_column(headers)
        date_col = self._resolve_date_column(headers, rows)
        body_cols = self._resolve_body_columns(headers, title_col, date_col)

        results: list[IngestResult] = []
        for row in rows:
            result = self._ingest_row(row, title_col, date_col, body_cols)
            results.append(result)

        return results

    def _resolve_title_column(self, headers: Sequence[str]) -> str:
        """Determine which column to use as title."""
        if self.mapping and self.mapping.title_column:
            return self.mapping.title_column
        # First text-like column (not a date hint)
        for h in headers:
            if h.lower() not in _DATE_COLUMN_HINTS:
                return h
        return headers[0]

    def _resolve_date_column(
        self, headers: Sequence[str], rows: list[dict[str, str]]
    ) -> str | None:
        """Detect which column contains dates."""
        if self.mapping and self.mapping.date_column:
            return self.mapping.date_column

        # Strategy 1: column name hints
        for h in headers:
            if h.lower() in _DATE_COLUMN_HINTS:
                return h

        # Strategy 2: sample cell values from first few rows
        sample = rows[:5]
        for h in headers:
            date_count = sum(1 for r in sample if _looks_like_date(r.get(h, "")))
            if date_count > 0 and date_count >= len(sample) * 0.5:
                return h

        return None

    def _resolve_body_columns(
        self,
        headers: Sequence[str],
        title_col: str,
        date_col: str | None,
    ) -> list[str]:
        """Determine which columns form the body."""
        if self.mapping and self.mapping.body_columns:
            return self.mapping.body_columns
        # All columns except title and date
        excluded = {title_col}
        if date_col:
            excluded.add(date_col)
        return [h for h in headers if h not in excluded]

    def _ingest_row(
        self,
        row: dict[str, str],
        title_col: str,
        date_col: str | None,
        body_cols: list[str],
    ) -> IngestResult:
        """Convert a single CSV row into a document."""
        title = row.get(title_col, "Untitled").strip() or "Untitled"

        # Extract observed_at
        obs_at: str
        if date_col and row.get(date_col, "").strip():
            obs_at = _normalize_date(row[date_col].strip())
        else:
            obs_at = datetime.now(UTC).isoformat()

        # Build body from remaining columns
        body_parts: list[str] = []
        for col in body_cols:
            val = row.get(col, "").strip()
            if val:
                body_parts.append(f"**{col}**: {val}")
        body = "\n\n".join(body_parts) if body_parts else ""

        doc = mkb.create_document(
            self.vault_path,
            self.doc_type,
            title,
            obs_at,
            body=body,
        )

        return IngestResult(
            doc_id=doc["id"],
            title=title,
            observed_at=obs_at,
            confidence=0.8,  # Import source default
        )


def _looks_like_date(value: str) -> bool:
    """Check if a string value looks like a date."""
    v = value.strip()
    if not v:
        return False
    return bool(
        _ISO_DATE_RE.match(v)
        or _SLASH_DATE_RE.match(v)
        or _WRITTEN_DATE_RE.match(v)
    )


def _normalize_date(value: str) -> str:
    """Normalize a date string to full ISO datetime format.

    Handles ISO dates, slash dates (M/D/YYYY), and written dates.
    Always appends T00:00:00Z to date-only strings since the Rust
    core requires a full datetime for observed_at.
    """
    v = value.strip()

    # Already full ISO datetime
    if _ISO_DATE_RE.match(v) and "T" in v:
        return v

    # ISO date only (YYYY-MM-DD) — append time
    if _ISO_DATE_RE.match(v):
        return f"{v}T00:00:00Z"

    # Slash date: M/D/YYYY
    if _SLASH_DATE_RE.match(v):
        parts = v.split("/")
        try:
            month, day, year = int(parts[0]), int(parts[1]), int(parts[2])
            return f"{year:04d}-{month:02d}-{day:02d}T00:00:00Z"
        except (ValueError, IndexError):
            return v

    # Written date: March 5, 2025
    if _WRITTEN_DATE_RE.match(v):
        for fmt in ("%B %d, %Y", "%B %d %Y"):
            try:
                dt = datetime.strptime(v, fmt)  # noqa: DTZ007
                return dt.strftime("%Y-%m-%dT00:00:00Z")
            except ValueError:
                continue

    return v
