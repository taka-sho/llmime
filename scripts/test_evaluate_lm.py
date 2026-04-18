"""pytest unit tests for evaluate_lm.py.

Tests use dummy CSV and mock LM/tokenizer — no real kenlm or vibrato needed.
"""

from __future__ import annotations

import csv
import textwrap
from pathlib import Path
from typing import Optional

import pytest

# Import the module under test
import importlib.util, sys

spec = importlib.util.spec_from_file_location(
    "evaluate_lm",
    Path(__file__).parent / "evaluate_lm.py",
)
mod = importlib.util.module_from_spec(spec)
sys.modules["evaluate_lm"] = mod
spec.loader.exec_module(mod)

TestItem = mod.TestItem
EvalResult = mod.EvalResult
compute_metrics = mod.compute_metrics
determine_overall = mod.determine_overall
load_testset = mod.load_testset
render_report = mod.render_report


# ── Helpers ──────────────────────────────────────────────────────────────────

DUMMY_CSV_ROWS = [
    ("general", "たべる", "食べる", "", "", "動詞"),
    ("general", "はしる", "走る", "", "", "動詞"),
    ("homophone", "こうえん", "公園", "週末に", "へ行く", "同音異義"),
    ("homophone", "こうえん", "講演", "大学教授の", "を聞く", "同音異義"),
    ("proper_noun", "とうきょう", "東京", "", "", "地名"),
    ("technical", "あるごりずむ", "アルゴリズム", "", "", "IT"),
    ("idiom", "いっせきにちょう", "一石二鳥", "", "", "四字熟語"),
    ("multi_bunsetsu", "きょうのかいぎはちゅうしになりました", "今日の会議は中止になりました", "", "", "日常文"),
    ("general", "みず", "水", "", "", "一般名詞"),
    ("general", "てんき", "天気", "", "", "一般名詞"),
]


def _write_dummy_csv(tmp_path: Path, rows=None) -> Path:
    p = tmp_path / "dummy.csv"
    if rows is None:
        rows = DUMMY_CSV_ROWS
    with open(p, "w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["category", "reading", "expected", "context_left", "context_right", "notes"])
        writer.writerows(rows)
    return p


def _make_result(category: str, reading: str, expected: str, top1_ok: bool, top5_ok: bool) -> EvalResult:
    item = TestItem(category=category, reading=reading, expected=expected)
    return EvalResult(
        item=item,
        candidates=[expected] if top1_ok else (["候補A", expected] if top5_ok else ["候補A"]),
        top1_correct=top1_ok,
        top5_correct=top5_ok,
    )


# ── load_testset ─────────────────────────────────────────────────────────────

class TestLoadTestset:
    def test_basic_load(self, tmp_path):
        p = _write_dummy_csv(tmp_path)
        items = load_testset(p)
        assert len(items) == len(DUMMY_CSV_ROWS)

    def test_category_filter(self, tmp_path):
        p = _write_dummy_csv(tmp_path)
        items = load_testset(p, category_filter="homophone")
        assert all(i.category == "homophone" for i in items)
        assert len(items) == 2

    def test_fields_parsed(self, tmp_path):
        p = _write_dummy_csv(tmp_path)
        items = load_testset(p)
        first = items[0]
        assert first.category == "general"
        assert first.reading == "たべる"
        assert first.expected == "食べる"

    def test_exit2_on_empty_filter(self, tmp_path):
        p = _write_dummy_csv(tmp_path)
        with pytest.raises(SystemExit) as exc:
            load_testset(p, category_filter="nonexistent_cat")
        assert exc.value.code == 2


# ── compute_metrics ───────────────────────────────────────────────────────────

class TestComputeMetrics:
    def _all_correct(self, n: int) -> list[EvalResult]:
        return [_make_result("general", "r", "e", True, True) for _ in range(n)]

    def _all_wrong(self, n: int) -> list[EvalResult]:
        return [_make_result("general", "r", "e", False, False) for _ in range(n)]

    def test_all_correct_m1_100(self):
        results = self._all_correct(10)
        m = compute_metrics(results)
        assert m["m1"] == 100.0
        assert m["m2"] == 100.0

    def test_all_wrong_m1_0(self):
        results = self._all_wrong(10)
        m = compute_metrics(results)
        assert m["m1"] == 0.0
        assert m["m2"] == 0.0

    def test_half_correct_m1_50(self):
        results = self._all_correct(5) + self._all_wrong(5)
        m = compute_metrics(results)
        assert m["m1"] == 50.0

    def test_top5_only(self):
        results = [_make_result("general", "r", "e", False, True) for _ in range(10)]
        m = compute_metrics(results)
        assert m["m1"] == 0.0
        assert m["m2"] == 100.0

    def test_category_accuracy(self):
        results = [
            _make_result("general", "r", "e", True, True),
            _make_result("general", "r", "e", False, False),
            _make_result("homophone", "r", "e", True, True),
        ]
        m = compute_metrics(results)
        assert m["m3"]["general"] == 50.0
        assert m["m3"]["homophone"] == 100.0

    def test_m4_context_lift(self):
        results = [
            EvalResult(
                item=TestItem("homophone", "r", "e", context_left="前文"),
                candidates=["e"],
                top1_correct=True,
                top5_correct=True,
            ),
            EvalResult(
                item=TestItem("homophone", "r", "e", context_left=""),
                candidates=["x"],
                top1_correct=False,
                top5_correct=False,
            ),
        ]
        m = compute_metrics(results)
        assert m["m4"] == 100.0  # 100% - 0%

    def test_empty_category_returns_zero(self):
        results = [_make_result("general", "r", "e", True, True)]
        m = compute_metrics(results)
        assert "homophone" not in m["m3"]


# ── determine_overall ─────────────────────────────────────────────────────────

class TestDetermineOverall:
    def test_pass_at_70(self):
        verdict, _ = determine_overall({"m1": 70.0})
        assert verdict == "PASS"

    def test_pass_above_70(self):
        verdict, _ = determine_overall({"m1": 85.0})
        assert verdict == "PASS"

    def test_needs_improvement_at_65(self):
        verdict, _ = determine_overall({"m1": 65.0})
        assert verdict == "NEEDS_IMPROVEMENT"

    def test_needs_improvement_at_50(self):
        verdict, _ = determine_overall({"m1": 50.0})
        assert verdict == "NEEDS_IMPROVEMENT"

    def test_fail_below_50(self):
        verdict, _ = determine_overall({"m1": 49.9})
        assert verdict == "FAIL"

    def test_fail_at_0(self):
        verdict, _ = determine_overall({"m1": 0.0})
        assert verdict == "FAIL"


# ── render_report ─────────────────────────────────────────────────────────────

class TestRenderReport:
    def _make_lm_stub(self, tmp_path):
        """Return an object that satisfies render_report's lm interface."""
        model_file = tmp_path / "dummy.bin"
        model_file.write_bytes(b"\x00" * 1024)

        class _LMStub:
            def file_size_mb(self): return 0.001
            def n_gram_order(self): return 3

        return _LMStub(), model_file

    def test_report_contains_verdict(self, tmp_path):
        lm, model_file = self._make_lm_stub(tmp_path)
        testset_path = tmp_path / "t.csv"
        testset_path.touch()
        dict_path = tmp_path / "system.dic"
        metrics = {"total": 10, "m1": 75.0, "m2": 95.0, "m3": {"general": 80.0}, "m4": 5.0, "m5": 60.0}
        results = [_make_result("general", "r", "e", True, True) for _ in range(10)]
        report = render_report(metrics, results, model_file, testset_path, dict_path, lm, verbose=False)
        assert "PASS" in report
        assert "M1" in report
        assert "M2" in report

    def test_report_has_category_table(self, tmp_path):
        lm, model_file = self._make_lm_stub(tmp_path)
        testset_path = tmp_path / "t.csv"
        testset_path.touch()
        dict_path = tmp_path / "system.dic"
        metrics = {"total": 10, "m1": 30.0, "m2": 40.0, "m3": {}, "m4": 0.0, "m5": 0.0}
        results = [_make_result("general", "r", "e", False, False) for _ in range(10)]
        report = render_report(metrics, results, model_file, testset_path, dict_path, lm, verbose=False)
        assert "カテゴリ別" in report
        assert "FAIL" in report
