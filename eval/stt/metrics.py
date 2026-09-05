"""Small, dependency-free CER/WER scorer; normalisation is part of the metric."""

import unicodedata

NORMALIZATION = "nfkc-punctuation-v1"


def tokens(text: str, language: str) -> list[str]:
    text = unicodedata.normalize("NFKC", text)
    if language == "ja":
        # Remove spacing and Unicode punctuation, but retain letters such as the
        # Japanese long-vowel mark, numbers, and meaningful symbols.
        return [c for c in text if not c.isspace() and not unicodedata.category(c).startswith("P")]
    if language == "en":
        # Apostrophes are removed within words; other punctuation separates words.
        text = "".join(
            "" if c in "'’" else " " if unicodedata.category(c).startswith("P") else c
            for c in text.lower()
        )
        return text.split()
    raise ValueError(f"unsupported scoring language: {language}")


def edit_distance(reference: list[str], hypothesis: list[str]) -> int:
    """Levenshtein substitutions + deletions + insertions, with linear memory."""
    previous = list(range(len(hypothesis) + 1))
    for i, expected in enumerate(reference, 1):
        current = [i]
        for j, actual in enumerate(hypothesis, 1):
            current.append(min(current[-1] + 1, previous[j] + 1,
                               previous[j - 1] + (expected != actual)))
        previous = current
    return previous[-1]


def score(reference: str, hypothesis: str, language: str) -> dict:
    expected, actual = tokens(reference, language), tokens(hypothesis, language)
    if not expected:
        raise ValueError("reference is empty after normalisation")
    errors = edit_distance(expected, actual)
    return {"errors": errors, "reference_units": len(expected), "rate": errors / len(expected)}
