"""Unit tests for idiom alias matching in evaluate_lm.py."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "scripts"))

from evaluate_lm import _alias_correct, TestItem  # noqa: E402


def _idiom_item(reading: str, expected: str) -> TestItem:
    return TestItem(category="idiom", reading=reading, expected=expected)


def _general_item(reading: str, expected: str) -> TestItem:
    return TestItem(category="general", reading=reading, expected=expected)


ALIASES = {
    "きをつかう": ["気を使う", "気を遣う"],
    "しかたがない": ["仕方がない", "仕方が無い"],
}


def test_alias_match_primary():
    item = _idiom_item("きをつかう", "気を使う")
    assert _alias_correct("気を使う", item, ALIASES)


def test_alias_match_variant():
    item = _idiom_item("きをつかう", "気を使う")
    assert _alias_correct("気を遣う", item, ALIASES)


def test_alias_no_match():
    item = _idiom_item("きをつかう", "気を使う")
    assert not _alias_correct("気をつかう", item, ALIASES)


def test_alias_not_applied_to_non_idiom():
    item = _general_item("きをつかう", "気を使う")
    assert not _alias_correct("気を遣う", item, ALIASES)


def test_alias_correct_general_exact_match():
    item = _general_item("たべる", "食べる")
    assert _alias_correct("食べる", item, {})


def test_alias_correct_general_no_match():
    item = _general_item("たべる", "食べる")
    assert not _alias_correct("たべる", item, {})
