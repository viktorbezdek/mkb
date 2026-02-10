"""Confidence scoring for documents.

Assigns a confidence score (0.0–1.0) based on:
- Source type (human-authored > AI-generated > inferred)
- Temporal precision (exact > approximate)
- Completeness (has body, has tags, has links)
- Corroboration (multiple sources mentioning same entity)
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass
class ScoreBreakdown:
    """Detailed breakdown of how confidence was calculated."""

    source_score: float = 0.0
    precision_score: float = 0.0
    completeness_score: float = 0.0
    corroboration_bonus: float = 0.0
    final_score: float = 0.0


# Source type weights
_SOURCE_WEIGHTS: dict[str, float] = {
    "human": 1.0,
    "manual": 1.0,
    "import": 0.9,
    "api": 0.85,
    "ai": 0.7,
    "ai-generated": 0.7,
    "inferred": 0.5,
    "unknown": 0.6,
}

# Precision weights
_PRECISION_WEIGHTS: dict[str, float] = {
    "exact": 1.0,
    "day": 0.95,
    "week": 0.85,
    "month": 0.75,
    "quarter": 0.65,
    "approximate": 0.5,
    "inferred": 0.4,
}


@dataclass
class ConfidenceScorer:
    """Calculate confidence scores for documents.

    Weights:
    - source_weight: importance of source type (0.0–1.0), default 0.3
    - precision_weight: importance of temporal precision, default 0.2
    - completeness_weight: importance of field completeness, default 0.3
    - corroboration_weight: bonus for corroborated info, default 0.2
    """

    source_weight: float = 0.3
    precision_weight: float = 0.2
    completeness_weight: float = 0.3
    corroboration_weight: float = 0.2

    def score(
        self,
        source: str | None = None,
        precision: str = "day",
        has_body: bool = False,
        has_tags: bool = False,
        has_links: bool = False,
        has_observed_at: bool = True,
        corroboration_count: int = 0,
    ) -> ScoreBreakdown:
        """Calculate confidence score with full breakdown."""
        # Source score
        src = (source or "unknown").lower()
        source_score = _SOURCE_WEIGHTS.get(src, 0.6)

        # Precision score
        precision_score = _PRECISION_WEIGHTS.get(precision.lower(), 0.5)

        # Completeness score (0-1 based on filled fields)
        completeness_checks: list[bool] = [
            has_observed_at,
            has_body,
            has_tags,
            has_links,
        ]
        completeness_score = sum(completeness_checks) / len(completeness_checks)

        # Corroboration bonus (diminishing returns)
        corroboration_bonus = min(1.0, corroboration_count * 0.2)

        # Weighted combination
        final = (
            self.source_weight * source_score
            + self.precision_weight * precision_score
            + self.completeness_weight * completeness_score
            + self.corroboration_weight * corroboration_bonus
        )

        return ScoreBreakdown(
            source_score=round(source_score, 3),
            precision_score=round(precision_score, 3),
            completeness_score=round(completeness_score, 3),
            corroboration_bonus=round(corroboration_bonus, 3),
            final_score=round(min(1.0, final), 3),
        )


def score_document(
    doc: dict[str, object],
    corroboration_count: int = 0,
) -> float:
    """Convenience function to score a document dict.

    Returns the final confidence score (0.0–1.0).
    """
    scorer = ConfidenceScorer()
    breakdown = scorer.score(
        source=str(doc.get("source", "unknown")),
        precision=str(doc.get("temporal_precision", "day")),
        has_body=bool(doc.get("body")),
        has_tags=bool(doc.get("tags")),
        has_links=False,  # Would need link query
        has_observed_at="observed_at" in doc,
        corroboration_count=corroboration_count,
    )
    return breakdown.final_score
