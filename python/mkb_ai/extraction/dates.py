"""Date and time extraction from unstructured text.

Rule-based extraction using regex patterns — no LLM required.
Handles ISO dates, common date formats, and relative date references.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone


@dataclass
class ExtractedDate:
    """A date found in text with its source context."""

    value: datetime
    original_text: str
    start: int
    end: int
    is_relative: bool = False


# ISO 8601 patterns
_ISO_DATETIME = re.compile(
    r"\b(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)\b"
)
_ISO_DATE = re.compile(r"\b(\d{4}-\d{2}-\d{2})\b")

# Common date formats
_SLASH_DATE_MDY = re.compile(r"\b(\d{1,2}/\d{1,2}/\d{4})\b")
_WRITTEN_DATE = re.compile(
    r"\b((?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*"
    r"\s+\d{1,2},?\s+\d{4})\b",
    re.IGNORECASE,
)

# Relative references
_RELATIVE_PATTERNS: list[tuple[re.Pattern[str], str]] = [
    (re.compile(r"\byesterday\b", re.IGNORECASE), "yesterday"),
    (re.compile(r"\btoday\b", re.IGNORECASE), "today"),
    (re.compile(r"\btomorrow\b", re.IGNORECASE), "tomorrow"),
    (re.compile(r"\blast\s+week\b", re.IGNORECASE), "last_week"),
    (re.compile(r"\bnext\s+week\b", re.IGNORECASE), "next_week"),
    (re.compile(r"\blast\s+month\b", re.IGNORECASE), "last_month"),
    (re.compile(r"\b(\d+)\s+days?\s+ago\b", re.IGNORECASE), "n_days_ago"),
    (re.compile(r"\b(\d+)\s+weeks?\s+ago\b", re.IGNORECASE), "n_weeks_ago"),
]

_MONTH_NAMES = {
    "jan": 1, "january": 1,
    "feb": 2, "february": 2,
    "mar": 3, "march": 3,
    "apr": 4, "april": 4,
    "may": 5,
    "jun": 6, "june": 6,
    "jul": 7, "july": 7,
    "aug": 8, "august": 8,
    "sep": 9, "september": 9,
    "oct": 10, "october": 10,
    "nov": 11, "november": 11,
    "dec": 12, "december": 12,
}


@dataclass
class DateExtractor:
    """Extract dates and time references from text.

    Finds ISO dates, written dates, and relative references like "yesterday".
    """

    reference_time: datetime = field(
        default_factory=lambda: datetime.now(timezone.utc)
    )

    def extract(self, text: str) -> list[ExtractedDate]:
        """Extract all date references from text."""
        results: list[ExtractedDate] = []

        # ISO datetime (most specific first)
        for m in _ISO_DATETIME.finditer(text):
            dt = _parse_iso_datetime(m.group(1))
            if dt is not None:
                results.append(
                    ExtractedDate(
                        value=dt,
                        original_text=m.group(0),
                        start=m.start(),
                        end=m.end(),
                    )
                )

        # ISO date only (skip if already matched as datetime)
        seen_ranges = {(r.start, r.end) for r in results}
        for m in _ISO_DATE.finditer(text):
            if any(s <= m.start() and m.end() <= e for s, e in seen_ranges):
                continue
            dt = _parse_iso_date(m.group(1))
            if dt is not None:
                results.append(
                    ExtractedDate(
                        value=dt,
                        original_text=m.group(0),
                        start=m.start(),
                        end=m.end(),
                    )
                )

        # Written dates (e.g., "January 15, 2025")
        for m in _WRITTEN_DATE.finditer(text):
            dt = _parse_written_date(m.group(1))
            if dt is not None:
                results.append(
                    ExtractedDate(
                        value=dt,
                        original_text=m.group(0),
                        start=m.start(),
                        end=m.end(),
                    )
                )

        # Slash dates (M/D/YYYY)
        for m in _SLASH_DATE_MDY.finditer(text):
            dt = _parse_slash_date(m.group(1))
            if dt is not None:
                results.append(
                    ExtractedDate(
                        value=dt,
                        original_text=m.group(0),
                        start=m.start(),
                        end=m.end(),
                    )
                )

        # Relative dates
        for pattern, kind in _RELATIVE_PATTERNS:
            for m in pattern.finditer(text):
                dt = self._resolve_relative(kind, m)
                if dt is not None:
                    results.append(
                        ExtractedDate(
                            value=dt,
                            original_text=m.group(0),
                            start=m.start(),
                            end=m.end(),
                            is_relative=True,
                        )
                    )

        # Sort by position in text
        results.sort(key=lambda r: r.start)
        return results

    def _resolve_relative(
        self, kind: str, match: re.Match[str]
    ) -> datetime | None:
        ref = self.reference_time
        if kind == "yesterday":
            return ref - timedelta(days=1)
        if kind == "today":
            return ref
        if kind == "tomorrow":
            return ref + timedelta(days=1)
        if kind == "last_week":
            return ref - timedelta(weeks=1)
        if kind == "next_week":
            return ref + timedelta(weeks=1)
        if kind == "last_month":
            return ref.replace(
                month=ref.month - 1 if ref.month > 1 else 12,
                year=ref.year if ref.month > 1 else ref.year - 1,
            )
        if kind == "n_days_ago":
            n = int(match.group(1))
            return ref - timedelta(days=n)
        if kind == "n_weeks_ago":
            n = int(match.group(1))
            return ref - timedelta(weeks=n)
        return None


def _parse_iso_datetime(s: str) -> datetime | None:
    """Parse an ISO 8601 datetime string."""
    try:
        return datetime.fromisoformat(s.replace("Z", "+00:00"))
    except ValueError:
        return None


def _parse_iso_date(s: str) -> datetime | None:
    """Parse an ISO 8601 date-only string."""
    try:
        return datetime.fromisoformat(s).replace(tzinfo=timezone.utc)
    except ValueError:
        return None


def _parse_written_date(s: str) -> datetime | None:
    """Parse a written date like 'January 15, 2025'."""
    parts = re.split(r"[\s,]+", s.strip())
    if len(parts) < 3:
        return None
    month_str = parts[0].lower()
    month = _MONTH_NAMES.get(month_str)
    if month is None:
        return None
    try:
        day = int(parts[1])
        year = int(parts[2])
        return datetime(year, month, day, tzinfo=timezone.utc)
    except (ValueError, IndexError):
        return None


def _parse_slash_date(s: str) -> datetime | None:
    """Parse M/D/YYYY format."""
    parts = s.split("/")
    if len(parts) != 3:
        return None
    try:
        month, day, year = int(parts[0]), int(parts[1]), int(parts[2])
        return datetime(year, month, day, tzinfo=timezone.utc)
    except ValueError:
        return None
