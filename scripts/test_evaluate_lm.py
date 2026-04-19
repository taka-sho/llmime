"""pytest unit tests for evaluate_lm.py.

Tests use dummy CSV and mock LM/index — no real kenlm or mozc dict needed.
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
MozcReadingIndex = mod.MozcReadingIndex
MozcConverter = mod.MozcConverter


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
        mozc_dir = tmp_path / "mozc_oss"
        metrics = {"total": 10, "m1": 30.0, "m2": 40.0, "m3": {}, "m4": 0.0, "m5": 0.0}
        results = [_make_result("general", "r", "e", False, False) for _ in range(10)]
        report = render_report(metrics, results, model_file, testset_path, mozc_dir, lm, verbose=False)
        assert "カテゴリ別" in report
        assert "FAIL" in report


# ── MozcReadingIndex ──────────────────────────────────────────────────────────

def _write_mozc_dict(tmp_path: Path, entries: list[tuple]) -> Path:
    """Write dummy mozc dictionary00.txt and return the dir path."""
    d = tmp_path / "mozc_oss"
    d.mkdir()
    with open(d / "dictionary00.txt", "w", encoding="utf-8") as f:
        for reading, lid, rid, cost, surface in entries:
            f.write(f"{reading}\t{lid}\t{rid}\t{cost}\t{surface}\n")
    return d


DUMMY_MOZC_ENTRIES = [
    ("こうえん", "1851", "1851", "5000", "公園"),
    ("こうえん", "1851", "1851", "6000", "講演"),
    ("こうえん", "1851", "1851", "7000", "公演"),
    ("たべる", "1851", "1851", "4000", "食べる"),
    ("とうきょう", "1851", "1851", "3000", "東京"),
]


class TestMozcReadingIndex:
    def test_exact_lookup(self, tmp_path):
        d = _write_mozc_dict(tmp_path, DUMMY_MOZC_ENTRIES)
        idx = MozcReadingIndex(d)
        results = idx.lookup("こうえん")
        surfaces = [s for s, _ in results]
        assert "公園" in surfaces
        assert "講演" in surfaces
        assert "公演" in surfaces

    def test_empty_lookup(self, tmp_path):
        d = _write_mozc_dict(tmp_path, DUMMY_MOZC_ENTRIES)
        idx = MozcReadingIndex(d)
        assert idx.lookup("xxxxunknownxxxx") == []

    def test_multi_entry_costs(self, tmp_path):
        d = _write_mozc_dict(tmp_path, DUMMY_MOZC_ENTRIES)
        idx = MozcReadingIndex(d)
        results = idx.lookup("こうえん")
        costs = [c for _, c in results]
        assert len(costs) == 3
        assert all(isinstance(c, int) for c in costs)

    def test_single_entry(self, tmp_path):
        d = _write_mozc_dict(tmp_path, DUMMY_MOZC_ENTRIES)
        idx = MozcReadingIndex(d)
        results = idx.lookup("たべる")
        assert len(results) == 1
        assert results[0][0] == "食べる"

    def test_malformed_line_skipped(self, tmp_path):
        d = tmp_path / "mozc_oss"
        d.mkdir()
        with open(d / "dictionary00.txt", "w") as f:
            f.write("badline\n")
            f.write("こうえん\t1\t1\t5000\t公園\n")
        idx = MozcReadingIndex(d)
        assert idx.lookup("こうえん") == [("公園", 5000)]

    def test_missing_dict_files_skipped(self, tmp_path):
        d = tmp_path / "mozc_oss"
        d.mkdir()
        with open(d / "dictionary00.txt", "w") as f:
            f.write("たべる\t1\t1\t4000\t食べる\n")
        idx = MozcReadingIndex(d)
        assert idx.lookup("たべる") == [("食べる", 4000)]


# ── MozcConverter ─────────────────────────────────────────────────────────────

class _MockLM:
    """Mock LM that scores by length (longer = better) for testing."""
    def score(self, sentence: str) -> float:
        return float(len(sentence))

    def score_batch(self, sentences: list) -> list:
        return [self.score(s) for s in sentences]

    def n_gram_order(self): return 3
    def file_size_mb(self): return 0.0


class TestMozcConverter:
    def _make_converter(self, tmp_path):
        d = _write_mozc_dict(tmp_path, DUMMY_MOZC_ENTRIES)
        lm = _MockLM()
        return MozcConverter(d, lm), d

    def test_known_reading_returns_candidates(self, tmp_path):
        conv, _ = self._make_converter(tmp_path)
        cands = conv.convert_candidates("こうえん", top_k=5)
        assert len(cands) >= 1
        assert all(isinstance(c, str) for c in cands)

    def test_oov_returns_reading_itself(self, tmp_path):
        conv, _ = self._make_converter(tmp_path)
        cands = conv.convert_candidates("xxxxunknownxxxx", top_k=5)
        assert len(cands) >= 1

    def test_topk_limit_respected(self, tmp_path):
        conv, _ = self._make_converter(tmp_path)
        cands = conv.convert_candidates("こうえん", top_k=2)
        assert len(cands) <= 2

    def test_cost_lm_combined_ordering(self, tmp_path):
        """Lower cost + higher LM score should rank higher."""
        entries = [
            ("あ", "1", "1", "100", "A"),   # low cost
            ("あ", "1", "1", "9000", "AA"),  # high cost
        ]
        d = _write_mozc_dict(tmp_path, entries)

        class _ConstLM:
            def score(self, s): return 0.0  # constant LM — cost alone decides
            def score_batch(self, ss): return [0.0] * len(ss)
        conv = MozcConverter(d, _ConstLM())
        cands = conv.convert_candidates("あ", top_k=2)
        assert cands[0] == "A"  # lower cost wins when LM is constant

    def test_context_left_passed_to_lm(self, tmp_path):
        """Candidates with context_left should be scored differently than without."""
        conv, _ = self._make_converter(tmp_path)
        cands_no_ctx = conv.convert_candidates("こうえん", context_left="", top_k=3)
        cands_with_ctx = conv.convert_candidates("こうえん", context_left="大学の", top_k=3)
        assert len(cands_no_ctx) >= 1
        assert len(cands_with_ctx) >= 1
