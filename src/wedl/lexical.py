from __future__ import annotations

import re

from .util import TOKEN_RE

STOPWORDS = {
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "had", "has", "have",
    "he", "her", "hers", "him", "his", "how", "i", "in", "is", "it", "its", "me", "my", "of", "on",
    "or", "our", "she", "that", "the", "their", "them", "they", "this", "to", "was", "we", "what",
    "when", "where", "which", "who", "why", "with", "you", "your",
}

QUOTED_OR_TERM = re.compile(r'"([^"\\]*(?:\\.[^"\\]*)*)"|([\w\'-]+)', re.UNICODE)


def content_tokens(text: str, *, unique: bool = False, maximum: int | None = None) -> list[str]:
    values = [
        match.group(0).casefold()
        for match in TOKEN_RE.finditer(text)
        if len(match.group(0)) > 1
    ]
    filtered = [value for value in values if value not in STOPWORDS] or values
    if unique:
        filtered = list(dict.fromkeys(filtered))
    return filtered[:maximum] if maximum is not None else filtered


def fts_query_parts(text: str) -> tuple[list[str], list[str]]:
    phrases: list[str] = []
    terms: list[str] = []
    for match in QUOTED_OR_TERM.finditer(text):
        if match.group(1) is not None:
            phrase = match.group(1).replace('\\"', '"').strip().casefold()
            if phrase:
                phrases.append(phrase)
        else:
            term = str(match.group(2)).casefold()
            if len(term) > 1 and term not in STOPWORDS:
                terms.append(term)
    if not phrases and not terms:
        terms = content_tokens(text, unique=True)
    return list(dict.fromkeys(phrases)), list(dict.fromkeys(terms))
