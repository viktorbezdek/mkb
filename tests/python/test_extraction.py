"""Tests for rule-based extraction pipeline.

Validates T-420.1 (dates), T-420.2 (entities), T-420.3 (confidence).
"""

from __future__ import annotations

from datetime import datetime, timezone

from mkb_ai.confidence import ConfidenceScorer, score_document
from mkb_ai.extraction import DateExtractor, EntityExtractor


# === T-420.1: Date/time extraction ===


class TestDateExtraction:
    """Extract dates from unstructured text."""

    def test_extract_iso_datetime(self) -> None:
        ext = DateExtractor()
        results = ext.extract("Meeting was on 2025-02-10T14:30:00Z.")
        assert len(results) == 1
        assert results[0].value.year == 2025
        assert results[0].value.month == 2
        assert results[0].value.day == 10

    def test_extract_iso_date(self) -> None:
        ext = DateExtractor()
        results = ext.extract("Deadline is 2025-06-15.")
        assert len(results) == 1
        assert results[0].value.day == 15

    def test_extract_written_date(self) -> None:
        ext = DateExtractor()
        results = ext.extract("Started on January 15, 2025.")
        assert len(results) == 1
        assert results[0].value.month == 1
        assert results[0].value.day == 15

    def test_extract_slash_date(self) -> None:
        ext = DateExtractor()
        results = ext.extract("Due by 3/15/2025.")
        assert len(results) == 1
        assert results[0].value.month == 3

    def test_extract_relative_yesterday(self) -> None:
        ref = datetime(2025, 2, 10, tzinfo=timezone.utc)
        ext = DateExtractor(reference_time=ref)
        results = ext.extract("Updated yesterday.")
        assert len(results) == 1
        assert results[0].is_relative
        assert results[0].value.day == 9

    def test_extract_relative_n_days_ago(self) -> None:
        ref = datetime(2025, 2, 10, tzinfo=timezone.utc)
        ext = DateExtractor(reference_time=ref)
        results = ext.extract("Discussed 5 days ago.")
        assert len(results) == 1
        assert results[0].value.day == 5

    def test_extract_multiple_dates(self) -> None:
        ext = DateExtractor()
        text = "Started 2025-01-01 and finishes 2025-12-31."
        results = ext.extract(text)
        assert len(results) == 2
        # Should be sorted by position
        assert results[0].value.month == 1
        assert results[1].value.month == 12

    def test_extract_no_dates(self) -> None:
        ext = DateExtractor()
        results = ext.extract("No dates here, just text.")
        assert len(results) == 0


# === T-420.2: Entity extraction ===


class TestEntityExtraction:
    """Extract entities from unstructured text."""

    def test_extract_jira_ticket(self) -> None:
        ext = EntityExtractor()
        results = ext.extract("Fixed in PROJ-123 and related to TEAM-456.")
        tickets = [e for e in results if e.kind == "jira_ticket"]
        assert len(tickets) == 2
        assert tickets[0].value == "PROJ-123"
        assert tickets[1].value == "TEAM-456"

    def test_extract_at_mentions(self) -> None:
        ext = EntityExtractor()
        results = ext.extract("Assigned to @jane.smith and reviewed by @john.")
        people = [e for e in results if e.kind == "person"]
        assert len(people) == 2
        assert people[0].value == "jane.smith"
        assert people[1].value == "john"

    def test_extract_person_with_title(self) -> None:
        ext = EntityExtractor()
        results = ext.extract("Approved by Jane Smith (VP).")
        people = [e for e in results if e.kind == "person"]
        assert len(people) == 1
        assert people[0].value == "Jane Smith"

    def test_extract_urls(self) -> None:
        ext = EntityExtractor()
        results = ext.extract("See https://example.com/docs for details.")
        urls = [e for e in results if e.kind == "url"]
        assert len(urls) == 1
        assert urls[0].value == "https://example.com/docs"

    def test_extract_emails(self) -> None:
        ext = EntityExtractor()
        results = ext.extract("Contact jane@example.com for questions.")
        emails = [e for e in results if e.kind == "email"]
        assert len(emails) == 1
        assert emails[0].value == "jane@example.com"

    def test_extract_by_kind(self) -> None:
        ext = EntityExtractor()
        text = "PROJ-123 assigned to @jane with docs at https://example.com"
        tickets = ext.extract_by_kind(text, "jira_ticket")
        assert len(tickets) == 1
        assert tickets[0].value == "PROJ-123"

    def test_extract_no_entities(self) -> None:
        ext = EntityExtractor()
        results = ext.extract("Just plain text with no special entities.")
        # Filter out false positives from title-case words
        meaningful = [e for e in results if e.kind in ("jira_ticket", "url", "email")]
        assert len(meaningful) == 0


# === T-420.3: Confidence scoring ===


class TestConfidenceScoring:
    """Confidence scoring based on source, precision, completeness."""

    def test_human_authored_high_confidence(self) -> None:
        scorer = ConfidenceScorer()
        result = scorer.score(
            source="human",
            precision="exact",
            has_body=True,
            has_tags=True,
            has_links=True,
            corroboration_count=5,
        )
        assert result.final_score >= 0.9

    def test_ai_generated_lower_confidence(self) -> None:
        scorer = ConfidenceScorer()
        result = scorer.score(
            source="ai",
            precision="approximate",
            has_body=True,
            has_tags=False,
            has_links=False,
        )
        assert result.final_score < 0.7

    def test_corroboration_boosts_score(self) -> None:
        scorer = ConfidenceScorer()
        base = scorer.score(source="manual", precision="day", has_body=True)
        boosted = scorer.score(
            source="manual", precision="day", has_body=True, corroboration_count=3
        )
        assert boosted.final_score > base.final_score
        assert boosted.corroboration_bonus > 0

    def test_score_never_exceeds_one(self) -> None:
        scorer = ConfidenceScorer()
        result = scorer.score(
            source="human",
            precision="exact",
            has_body=True,
            has_tags=True,
            has_links=True,
            corroboration_count=100,
        )
        assert result.final_score <= 1.0

    def test_score_document_convenience(self) -> None:
        doc = {
            "source": "manual",
            "temporal_precision": "day",
            "body": "Some content",
            "tags": ["test"],
            "observed_at": "2025-02-10T00:00:00Z",
        }
        score = score_document(doc)
        assert 0.0 < score <= 1.0

    def test_unknown_source_gets_default(self) -> None:
        scorer = ConfidenceScorer()
        result = scorer.score(source="something_new", precision="day")
        assert result.source_score == 0.6  # Default for unknown
