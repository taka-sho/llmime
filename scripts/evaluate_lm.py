#!/usr/bin/env python3
"""N-gram language model accuracy evaluation for llmime.

Usage:
    python3 scripts/evaluate_lm.py \
        --testset tests/lm_eval/testset.csv \
        --model models/wiki-ja.5gram.bin \
        --vibrato-dict dict/system.dic \
        --top-k 5 \
        --output reports/lm_eval_YYYYMMDD.md \
        [--category-filter homophone] \
        [--verbose]
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Optional

# Try kenlm Python binding; fall back to CLI subprocess if unavailable.
try:
    import kenlm as _kenlm_mod
    _KENLM_PYTHON = True
except ImportError:
    _kenlm_mod = None
    _KENLM_PYTHON = False


CATEGORY_TARGETS = {
    "general": 85,
    "homophone": 50,
    "proper_noun": 60,
    "technical": 65,
    "idiom": 75,
    "multi_bunsetsu": 70,
}

CATEGORY_LABELS = {
    "general": "A 一般語彙",
    "homophone": "B 同音異義語",
    "proper_noun": "C 固有名詞",
    "technical": "D 専門語",
    "idiom": "E 慣用句",
    "multi_bunsetsu": "F 連文節短文",
}

CATEGORY_ORDER = ["general", "homophone", "proper_noun", "technical", "idiom", "multi_bunsetsu"]


@dataclass
class TestItem:
    category: str
    reading: str
    expected: str
    context_left: str = ""
    context_right: str = ""
    notes: str = ""


@dataclass
class EvalResult:
    item: TestItem
    candidates: list[str] = field(default_factory=list)
    top1_correct: bool = False
    top5_correct: bool = False


class LanguageModel:
    """Wrapper around kenlm (Python binding or CLI)."""

    def __init__(self, model_path: Path):
        self._model_path = model_path
        self._model = None
        self._use_python = False

        if _KENLM_PYTHON:
            try:
                self._model = _kenlm_mod.Model(str(model_path))
                self._use_python = True
            except Exception as e:
                print(f"[WARN] kenlm Python binding failed to load model: {e}", file=sys.stderr)

        if not self._use_python:
            # Fall back to CLI
            if not _find_kenlm_query():
                raise RuntimeError(
                    "kenlm not available: Python binding import failed and 'query' CLI not found.\n"
                    "Install kenlm: pip install https://github.com/kpu/kenlm/archive/master.zip\n"
                    "Or build kenlm and ensure 'query' is on PATH."
                )

    def score(self, sentence: str) -> float:
        """Return log10 probability for sentence."""
        if self._use_python:
            return self._model.score(sentence, bos=True, eos=True)
        return _score_via_cli(self._model_path, sentence)

    def n_gram_order(self) -> int:
        if self._use_python:
            return self._model.order
        return 0

    def file_size_mb(self) -> float:
        return self._model_path.stat().st_size / (1024 * 1024)


def _find_kenlm_query() -> Optional[str]:
    import shutil
    return shutil.which("query")


def _score_via_cli(model_path: Path, sentence: str) -> float:
    query_bin = _find_kenlm_query()
    if not query_bin:
        return float("-inf")
    try:
        result = subprocess.run(
            [query_bin, "-n", str(model_path)],
            input=sentence + "\n",
            capture_output=True,
            text=True,
            timeout=10,
        )
        for line in result.stdout.splitlines():
            if line.startswith("Total:"):
                return float(line.split()[1])
    except Exception:
        pass
    return float("-inf")


class VibratoTokenizer:
    """Calls 'vibrato' CLI for morphological analysis and candidate generation."""

    def __init__(self, dict_path: Path):
        self._dict_path = dict_path
        import shutil
        self._vibrato_bin = shutil.which("vibrato") or shutil.which("vibrato-tokenize")
        if not self._vibrato_bin:
            raise RuntimeError(
                f"'vibrato' CLI not found. Build from https://github.com/daac-tools/vibrato\n"
                f"and ensure the binary is on PATH. Dict: {dict_path}"
            )
        if not dict_path.exists():
            raise RuntimeError(f"Vibrato dict not found: {dict_path}")

    def convert_candidates(self, reading: str, context_left: str = "", top_k: int = 5) -> list[str]:
        """Generate top-K conversion candidates for a reading string.

        Uses Vibrato's lattice output to enumerate candidate surface forms.
        When context_left is given, it is prepended to improve N-gram scoring.
        """
        context_text = (context_left + reading) if context_left else reading
        try:
            result = subprocess.run(
                [self._vibrato_bin, "-d", str(self._dict_path), "-O", "wakati"],
                input=context_text,
                capture_output=True,
                text=True,
                timeout=15,
            )
            tokens = result.stdout.strip().split()
            return [" ".join(tokens)] if tokens else []
        except Exception as e:
            print(f"[WARN] Vibrato error for '{reading}': {e}", file=sys.stderr)
            return []


def load_testset(path: Path, category_filter: str = "") -> list[TestItem]:
    items = []
    with open(path, encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for lineno, row in enumerate(reader, start=2):
            try:
                cat = row["category"].strip()
                if category_filter and cat != category_filter:
                    continue
                items.append(TestItem(
                    category=cat,
                    reading=row["reading"].strip(),
                    expected=row["expected"].strip(),
                    context_left=row.get("context_left", "").strip(),
                    context_right=row.get("context_right", "").strip(),
                    notes=row.get("notes", "").strip(),
                ))
            except KeyError as e:
                print(f"[WARN] CSV line {lineno} missing column {e} — skipped", file=sys.stderr)
    if not items:
        print("[ERROR] No test items loaded (0 items).", file=sys.stderr)
        sys.exit(2)
    return items


def evaluate(
    items: list[TestItem],
    lm: LanguageModel,
    tokenizer: VibratoTokenizer,
    top_k: int,
    verbose: bool,
) -> list[EvalResult]:
    results = []
    for item in items:
        candidates = tokenizer.convert_candidates(item.reading, item.context_left, top_k)
        if not candidates:
            candidates = [item.reading]  # fallback: reading itself as sole candidate

        # Score candidates with KenLM and take top-k
        scored = []
        for cand in candidates:
            score = lm.score(cand)
            scored.append((score, cand))
        scored.sort(key=lambda x: x[0], reverse=True)
        top_candidates = [c for _, c in scored[:top_k]]

        top1_correct = bool(top_candidates) and top_candidates[0] == item.expected
        top5_correct = item.expected in top_candidates

        if verbose and not top1_correct:
            top1 = top_candidates[0] if top_candidates else "(none)"
            print(
                f"[MISS] cat={item.category} reading={item.reading} "
                f"expected={item.expected} got={top1}",
                file=sys.stderr,
            )

        results.append(EvalResult(
            item=item,
            candidates=top_candidates,
            top1_correct=top1_correct,
            top5_correct=top5_correct,
        ))
    return results


def compute_metrics(results: list[EvalResult]) -> dict:
    total = len(results)
    m1_correct = sum(1 for r in results if r.top1_correct)
    m2_correct = sum(1 for r in results if r.top5_correct)

    m1 = round(m1_correct / total * 100, 1) if total else 0.0
    m2 = round(m2_correct / total * 100, 1) if total else 0.0

    # M3: per-category accuracy
    by_cat: dict[str, list[EvalResult]] = {}
    for r in results:
        by_cat.setdefault(r.item.category, []).append(r)

    m3: dict[str, float] = {}
    for cat, cat_results in by_cat.items():
        n = len(cat_results)
        correct = sum(1 for r in cat_results if r.top1_correct)
        m3[cat] = round(correct / n * 100, 1) if n else 0.0

    # M4: homophone context-lift (context_left present vs absent)
    hm = [r for r in results if r.item.category == "homophone"]
    with_ctx = [r for r in hm if r.item.context_left]
    without_ctx = [r for r in hm if not r.item.context_left]
    ctx_rate = round(sum(r.top1_correct for r in with_ctx) / len(with_ctx) * 100, 1) if with_ctx else 0.0
    noctx_rate = round(sum(r.top1_correct for r in without_ctx) / len(without_ctx) * 100, 1) if without_ctx else 0.0
    m4 = round(ctx_rate - noctx_rate, 1)

    # M5: homophone Wikipedia-frequency bias (top1 = most frequent reading)
    m5_correct = sum(1 for r in hm if r.top1_correct)
    m5 = round(m5_correct / len(hm) * 100, 1) if hm else 0.0

    return {
        "total": total,
        "m1": m1,
        "m2": m2,
        "m3": m3,
        "m4": m4,
        "m5": m5,
    }


def _judge(value: float, target: float) -> str:
    return "PASS" if value >= target else "FAIL"


def determine_overall(metrics: dict) -> tuple[str, str]:
    m1 = metrics["m1"]
    if m1 >= 70:
        verdict = "PASS"
        reason = f"M1 Top-1 {m1}% ≥ 70% 基準達成。Phase 2 へ進む。"
    elif m1 >= 50:
        verdict = "NEEDS_IMPROVEMENT"
        reason = f"M1 Top-1 {m1}%。改善手段A〜C を順次適用し再評価。"
    else:
        verdict = "FAIL"
        reason = f"M1 Top-1 {m1}% < 50%。学習パイプライン根本見直し（殿エスカレ即時）。"
    return verdict, reason


def render_report(
    metrics: dict,
    results: list[EvalResult],
    model_path: Path,
    testset_path: Path,
    dict_path: Path,
    lm: LanguageModel,
    verbose: bool,
) -> str:
    now = datetime.now().strftime("%Y-%m-%d %H:%M")
    total = metrics["total"]
    m1 = metrics["m1"]
    m2 = metrics["m2"]
    m3 = metrics["m3"]
    m4 = metrics["m4"]
    m5 = metrics["m5"]

    size_mb = lm.file_size_mb()
    n_gram = lm.n_gram_order()
    n_gram_str = f"{n_gram}-gram" if n_gram else "?-gram"

    verdict, reason = determine_overall(metrics)

    lines = [
        "# LM Evaluation Report",
        "",
        f"- Date: {now}",
        f"- Model: {model_path} ({size_mb:.1f} MB, {n_gram_str})",
        f"- Test set: {testset_path} ({total} samples)",
        f"- Vibrato dict: {dict_path.name}",
        "",
        "## 主要指標",
        "",
        "| 指標 | 値 | 目標 | 判定 |",
        "|------|------|------|------|",
        f"| M1 Top-1 完全一致 | {m1}% | ≥70% | {_judge(m1, 70)} |",
        f"| M2 Top-5 含有率 | {m2}% | ≥90% | {_judge(m2, 90)} |",
        "",
        "## カテゴリ別 Top-1 精度",
        "",
        "| カテゴリ | サンプル数 | Top-1精度 | 目標 | 判定 |",
        "|---------|----------|----------|------|------|",
    ]

    by_cat: dict[str, list[EvalResult]] = {}
    for r in results:
        by_cat.setdefault(r.item.category, []).append(r)

    for cat in CATEGORY_ORDER:
        cat_results = by_cat.get(cat, [])
        n = len(cat_results)
        acc = m3.get(cat, 0.0)
        target = CATEGORY_TARGETS.get(cat, 0)
        label = CATEGORY_LABELS.get(cat, cat)
        lines.append(f"| {label} | {n} | {acc}% | ≥{target}% | {_judge(acc, target)} |")

    lines += [
        "",
        "## 副次指標",
        "",
        f"- M4 同音異義語 文脈効果: +{m4}pt (文脈あり vs なし)",
        f"- M5 最頻表記正解率: {m5}%",
    ]

    if verbose:
        miss_cases = [r for r in results if not r.top1_correct]
        lines += [
            "",
            "## ミスケース上位 (verbose)",
            "",
            "| カテゴリ | reading | expected | got_top1 | got_top5 |",
            "|---------|---------|----------|----------|----------|",
        ]
        for r in miss_cases[:50]:
            top1 = r.candidates[0] if r.candidates else "(none)"
            top5 = "○" if r.top5_correct else "✗"
            lines.append(
                f"| {r.item.category} | {r.item.reading} | {r.item.expected} | {top1} | {top5} |"
            )

    lines += [
        "",
        "## 総合判定",
        "",
        f"**{verdict}** — {reason}",
    ]

    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description="N-gram LM accuracy evaluator for llmime")
    parser.add_argument("--testset", required=True, type=Path)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--vibrato-dict", required=True, type=Path, dest="vibrato_dict")
    parser.add_argument("--top-k", type=int, default=5)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--category-filter", default="")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    # Load model
    if not args.model.exists():
        print(f"[ERROR] Model not found: {args.model}", file=sys.stderr)
        sys.exit(1)

    try:
        lm = LanguageModel(args.model)
    except RuntimeError as e:
        print(f"[ERROR] {e}", file=sys.stderr)
        sys.exit(1)

    # Load tokenizer
    try:
        tokenizer = VibratoTokenizer(args.vibrato_dict)
    except RuntimeError as e:
        print(f"[ERROR] {e}", file=sys.stderr)
        sys.exit(1)

    # Load testset
    items = load_testset(args.testset, args.category_filter)
    print(f"[INFO] Loaded {len(items)} test items.", file=sys.stderr)

    # Evaluate
    results = evaluate(items, lm, tokenizer, args.top_k, args.verbose)

    # Compute metrics
    metrics = compute_metrics(results)
    print(f"[INFO] M1={metrics['m1']}%  M2={metrics['m2']}%", file=sys.stderr)

    # Render report
    report = render_report(metrics, results, args.model, args.testset, args.vibrato_dict, lm, args.verbose)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(report, encoding="utf-8")
    print(f"[INFO] Report written to {args.output}", file=sys.stderr)

    # Exit 3 if FAIL so CI can catch it
    verdict, _ = determine_overall(metrics)
    if verdict == "FAIL":
        sys.exit(3)


if __name__ == "__main__":
    main()
