"""Entity extraction from unstructured text.

Rule-based extraction using regex patterns — no LLM required.
Extracts Jira ticket IDs, person mentions, URLs, and email addresses.
"""

from __future__ import annotations

import re
from dataclasses import dataclass


@dataclass
class ExtractedEntity:
    """An entity found in text."""

    kind: str  # "jira_ticket", "person", "url", "email"
    value: str
    original_text: str
    start: int
    end: int


# Jira ticket pattern: PROJECT-123
_JIRA_TICKET = re.compile(r"\b([A-Z][A-Z0-9]+-\d+)\b")

# Person mention: @name or Name (Title)
_AT_MENTION = re.compile(r"@(\w[\w.-]*\w|\w)")
_PERSON_TITLE = re.compile(
    r"\b([A-Z][a-z]+(?:\s+[A-Z][a-z]+)+)\s*\("
    r"(?:CEO|CTO|VP|Director|Manager|Lead|Engineer|PM|Designer|Analyst)"
    r"\)"
)

# URL pattern (simplified)
_URL = re.compile(
    r"https?://[^\s<>\"')\]]+",
    re.IGNORECASE,
)

# Email pattern
_EMAIL = re.compile(
    r"\b[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}\b"
)


@dataclass
class EntityExtractor:
    """Extract named entities from text using regex patterns."""

    def extract(self, text: str) -> list[ExtractedEntity]:
        """Extract all entities from text."""
        results: list[ExtractedEntity] = []

        # Jira tickets
        for m in _JIRA_TICKET.finditer(text):
            results.append(
                ExtractedEntity(
                    kind="jira_ticket",
                    value=m.group(1),
                    original_text=m.group(0),
                    start=m.start(),
                    end=m.end(),
                )
            )

        # @mentions
        for m in _AT_MENTION.finditer(text):
            results.append(
                ExtractedEntity(
                    kind="person",
                    value=m.group(1),
                    original_text=m.group(0),
                    start=m.start(),
                    end=m.end(),
                )
            )

        # Person with title
        for m in _PERSON_TITLE.finditer(text):
            results.append(
                ExtractedEntity(
                    kind="person",
                    value=m.group(1),
                    original_text=m.group(0),
                    start=m.start(),
                    end=m.end(),
                )
            )

        # URLs
        for m in _URL.finditer(text):
            results.append(
                ExtractedEntity(
                    kind="url",
                    value=m.group(0),
                    original_text=m.group(0),
                    start=m.start(),
                    end=m.end(),
                )
            )

        # Emails
        for m in _EMAIL.finditer(text):
            results.append(
                ExtractedEntity(
                    kind="email",
                    value=m.group(0),
                    original_text=m.group(0),
                    start=m.start(),
                    end=m.end(),
                )
            )

        results.sort(key=lambda r: r.start)
        return results

    def extract_by_kind(self, text: str, kind: str) -> list[ExtractedEntity]:
        """Extract entities of a specific kind."""
        return [e for e in self.extract(text) if e.kind == kind]
