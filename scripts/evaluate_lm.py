#!/usr/bin/env python3
"""N-gram language model accuracy evaluation for llmime.

Usage:
    python3 scripts/evaluate_lm.py \
        --testset tests/lm_eval/testset.csv \
        --model models/llmime.klm \
        --mozc-dict vendor/mozc_oss \
        --top-k 5 \
        --output reports/lm_eval_YYYYMMDD.md \
        [--category-filter homophone] \
        [--verbose]

Legacy --vibrato-dict also accepted for backwards compatibility.
"""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Optional

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
    def __init__(self, model_path: Path):
        self._model_path = model_path
        self._model = None
        self._use_python = False

        if _KENLM_PYTHON:
            try:
                self._model = _kenlm_mod.Model(str(model_path))
                self._use_python = True
            except Exception as e:
                print(f"[WARN] kenlm Python binding failed: {e}", file=sys.stderr)

        if not self._use_python and not _find_kenlm_query():
            raise RuntimeError(
                "kenlm not available. Install: pip install kenlm or build kenlm CLI."
            )

    def score_batch(self, sentences: list[str]) -> list[float]:
        if self._use_python:
            return [self._model.score(s, bos=True, eos=True) for s in sentences]
        return _score_batch_via_cli(self._model_path, sentences)

    def n_gram_order(self) -> int:
        return self._model.order if self._use_python else 0

    def file_size_mb(self) -> float:
        return self._model_path.stat().st_size / (1024 * 1024)


def _find_kenlm_query() -> Optional[str]:
    import shutil
    return shutil.which("query")


def _score_batch_via_cli(model_path: Path, sentences: list[str]) -> list[float]:
    query_bin = _find_kenlm_query()
    if not query_bin or not sentences:
        return [float("-inf")] * len(sentences)
    try:
        result = subprocess.run(
            [query_bin, "-n", str(model_path)],
            input="\n".join(sentences) + "\n",
            capture_output=True,
            text=True,
            timeout=120,
        )
        scores = []
        for line in result.stdout.splitlines():
            if "Total:" in line:
                after = line[line.index("Total:") + len("Total:"):].split()
                scores.append(float(after[0]) if after else float("-inf"))
        if len(scores) == len(sentences):
            return scores
    except Exception:
        pass
    return [float("-inf")] * len(sentences)


def _lid_to_pos(lid: int) -> str:
    """Map mozc IPAdic left_id to a POS string (mirrors Rust lid_to_pos)."""
    if 172 <= lid <= 180:
        return "助動詞"
    if lid in (215, 423):
        return "終助詞"
    if lid in (591, 633) or 837 <= lid <= 870:
        return "動詞"
    if lid in (726, 830, 2418, 2459) or 2467 <= lid <= 2476:
        return "形容詞"
    if 168 <= lid <= 720:
        return "助詞"
    if 1280 <= lid <= 2671:
        return "名詞"
    return "名詞"


# POS bigram connection penalty (in mozc cost units, lower = better).
# Mirrors Rust pos_connection::connection_penalty, scaled to match word costs.
_POS_PENALTY: dict[tuple[str, str], int] = {
    # 自然な遷移 (penalty = 0)
    ("名詞", "助詞"): 0, ("名詞", "助動詞"): 0,
    ("形容詞", "名詞"): 0, ("形容詞", "助動詞"): 0, ("形容詞", "助詞"): 500,
    ("動詞", "助詞"): 0, ("動詞", "助動詞"): 0, ("動詞", "終助詞"): 0,
    ("助詞", "名詞"): 0, ("助詞", "動詞"): 0, ("助詞", "形容詞"): 0,
    ("助動詞", "終助詞"): 0, ("助動詞", "助詞"): 500,
    # 可能だが稀な遷移
    ("名詞", "動詞"): 1000, ("名詞", "形容詞"): 1000, ("名詞", "名詞"): 1500,
    ("形容詞", "形容詞"): 1500, ("動詞", "動詞"): 2000,
    ("助動詞", "名詞"): 1500, ("助動詞", "動詞"): 1500,
    # 悪い遷移
    ("助詞", "助詞"): 5000, ("助詞", "助動詞"): 3000,
}
_POS_PENALTY_DEFAULT = 2500
_POS_PENALTY_SENT_FINAL_NEXT = 8000


def _pos_connection_penalty(prev_pos: str, next_pos: str) -> int:
    """Return POS bigram connection penalty in mozc cost units."""
    if not prev_pos:   # BOS
        return 0
    if prev_pos == "終助詞":
        return _POS_PENALTY_SENT_FINAL_NEXT
    return _POS_PENALTY.get((prev_pos, next_pos), _POS_PENALTY_DEFAULT)


class MozcReadingIndex:
    def __init__(self, dict_dir: Path):
        # Store (surface, cost, pos_str) per reading
        self.map: dict[str, list[tuple[str, int, str]]] = defaultdict(list)
        for i in range(10):
            dict_file = dict_dir / f"dictionary0{i}.txt"
            if not dict_file.exists():
                continue
            with open(dict_file, encoding="utf-8") as f:
                for line in f:
                    parts = line.rstrip("\n").split("\t")
                    if len(parts) < 5:
                        continue
                    reading, lid_str, _rid, cost, surface = parts[:5]
                    try:
                        lid = int(lid_str)
                        pos = _lid_to_pos(lid)
                        self.map[reading].append((surface, int(cost), pos))
                    except ValueError:
                        continue

    def lookup(self, reading: str) -> list[tuple[str, int]]:
        return [(surface, cost) for surface, cost, _pos in self.map.get(reading, [])]

    def prefix_search(self, reading: str) -> list[tuple[int, str, int, str]]:
        """Returns (word_len, surface, cost, pos_str) for each matching prefix."""
        chars = list(reading)
        results = []
        for length in range(1, len(chars) + 1):
            prefix = "".join(chars[:length])
            for surface, cost, pos in self.map.get(prefix, []):
                results.append((length, surface, cost, pos))
        return results


def _viterbi_by_cost(
    reading: str,
    index: MozcReadingIndex,
    beam_width: int = 16,
    top_k: int = 20,
    pos_penalty_alpha: float = 1.0,
) -> list[tuple[int, str]]:
    """Viterbi beam search using mozc cost + POS bigram connection penalty.

    Returns [(total_cost, joined_surface), ...] sorted ascending by cost.
    Used as fallback for multi-bunsetsu readings not in direct lookup.
    """
    if not reading:
        return []

    chars = list(reading)
    total = len(chars)
    # Beam entries: (cumulative_cost, surfaces, last_pos_str)
    beam: list[list[tuple[int, list[str], str]]] = [[] for _ in range(total + 1)]
    beam[0].append((0, [], ""))  # empty last_pos = BOS

    for pos in range(total):
        if len(beam[pos]) > beam_width:
            beam[pos].sort(key=lambda x: x[0])
            beam[pos] = beam[pos][:beam_width]
        if not beam[pos]:
            continue

        remaining = "".join(chars[pos:])
        matches = index.prefix_search(remaining)

        if not matches:
            for cost, surfaces, last_pos in beam[pos]:
                beam[pos + 1].append((cost + 10000, surfaces + [chars[pos]], ""))
        else:
            for cost, surfaces, last_pos in beam[pos]:
                for word_len, surface, entry_cost, entry_pos in matches:
                    next_pos = pos + word_len
                    if next_pos > total:
                        continue
                    # Penalise single-char zero-cost entries (hiragana fallbacks).
                    effective_cost = entry_cost
                    if word_len == 1:
                        effective_cost = max(entry_cost, 3000)
                    # POS bigram connection penalty
                    pos_pen = int(pos_penalty_alpha * _pos_connection_penalty(last_pos, entry_pos))
                    beam[next_pos].append((
                        cost + effective_cost + pos_pen,
                        surfaces + [surface],
                        entry_pos,
                    ))

    final = beam[total]
    final.sort(key=lambda x: x[0])
    final = final[:top_k]

    seen: set[str] = set()
    result = []
    for cost, surfaces, _pos in final:
        joined = "".join(surfaces)
        if joined not in seen:
            seen.add(joined)
            result.append((cost, joined))
    return result


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
        print("[ERROR] No test items loaded.", file=sys.stderr)
        sys.exit(2)
    return items


def _alias_correct(candidate: str, item: TestItem, aliases: dict[str, list[str]]) -> bool:
    """Return True if candidate matches expected or an idiom alias."""
    if candidate == item.expected:
        return True
    if item.category == "idiom":
        return candidate in aliases.get(item.reading, [])
    return False


def evaluate(
    items: list[TestItem],
    index: MozcReadingIndex,
    lm: LanguageModel,
    top_k: int,
    verbose: bool,
    beam_width: int = 16,
    idiom_aliases: Optional[dict[str, list[str]]] = None,
) -> list[EvalResult]:
    """Hybrid evaluation:
    - Direct lookup (A/B/C/D/E categories): lookup(reading) → batch LM scoring
    - Viterbi fallback (F/unknown): cost-only Viterbi → batch LM scoring
    All LM scoring is batched globally in ONE kenlm call.
    """
    cost_alpha = 0.01

    def _build_lm_sentence(item: TestItem, surface: str) -> str:
        """Compose LM input sentence.

        KenLM model is trained with whitespace token boundaries. When context
        exists, score as "left surface right" instead of raw concatenation.
        """
        if item.context_left or item.context_right:
            return " ".join(p for p in (item.context_left, surface, item.context_right) if p)
        return surface

    # Phase 1: collect candidate surfaces + sentences
    item_cands: list[list[tuple[int, str]]] = []  # [(cost, surface)]
    all_sentences: list[str] = []
    slice_bounds: list[tuple[int, int]] = []

    print("[INFO] Phase 1: candidate generation ...", file=sys.stderr)
    for item in items:
        direct = index.lookup(item.reading)
        if direct:
            cands = direct  # [(surface, cost)]
            # Normalize to (cost, surface)
            normed = [(cost, surface) for surface, cost in cands]
        else:
            # Viterbi fallback for multi-bunsetsu / unknown words
            normed = _viterbi_by_cost(item.reading, index, beam_width, top_k * 4)
            if not normed:
                normed = [(0, item.reading)]

        item_cands.append(normed)
        start = len(all_sentences)
        for cost, surface in normed:
            sentence = _build_lm_sentence(item, surface)
            all_sentences.append(sentence)
        slice_bounds.append((start, len(all_sentences)))

    # Phase 2: batch score ALL sentences in ONE kenlm call
    print(f"[INFO] Phase 2: batch scoring {len(all_sentences)} sentences ...", file=sys.stderr)
    all_scores = lm.score_batch(all_sentences)

    # Penalty for surface == reading when kanji alternatives exist.
    # Avoids penalising naturally-hiragana words (e.g. ゆっくり).
    _PASSTHROUGH_PENALTY = 3.0

    def _has_kanji(s: str) -> bool:
        return any("\u4e00" <= c <= "\u9fff" for c in s)

    # Phase 3: rerank each item
    _aliases: dict[str, list[str]] = idiom_aliases or {}
    results = []
    for item, cands, (start, end) in zip(items, item_cands, slice_bounds):
        item_scores = all_scores[start:end]
        has_kanji_alt = any(_has_kanji(surface) for _, surface in cands)
        scored = [
            (
                lm_score
                - cost_alpha * (cost / 100.0)
                - (_PASSTHROUGH_PENALTY if (surface == item.reading and has_kanji_alt) else 0.0),
                surface,
            )
            for (cost, surface), lm_score in zip(cands, item_scores)
        ]
        scored.sort(reverse=True)

        seen: set[str] = set()
        top_candidates = []
        for _, surface in scored:
            if surface not in seen:
                seen.add(surface)
                top_candidates.append(surface)
                if len(top_candidates) >= top_k:
                    break
        if not top_candidates:
            top_candidates = [item.reading]

        top1_correct = _alias_correct(top_candidates[0], item, _aliases)
        top5_correct = any(_alias_correct(c, item, _aliases) for c in top_candidates)

        if verbose and not top1_correct:
            aliases_for_item = _aliases.get(item.reading, []) if item.category == "idiom" else []
            alias_info = f" aliases={aliases_for_item}" if aliases_for_item else ""
            print(
                f"[MISS] cat={item.category} reading={item.reading} "
                f"expected={item.expected} got={top_candidates[0]}{alias_info}",
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
    m1 = round(sum(1 for r in results if r.top1_correct) / total * 100, 1) if total else 0.0
    m2 = round(sum(1 for r in results if r.top5_correct) / total * 100, 1) if total else 0.0

    by_cat: dict[str, list[EvalResult]] = {}
    for r in results:
        by_cat.setdefault(r.item.category, []).append(r)

    m3 = {
        cat: round(sum(r.top1_correct for r in cat_res) / len(cat_res) * 100, 1)
        for cat, cat_res in by_cat.items()
    }

    hm = [r for r in results if r.item.category == "homophone"]
    with_ctx = [r for r in hm if r.item.context_left]
    without_ctx = [r for r in hm if not r.item.context_left]
    ctx_rate = round(sum(r.top1_correct for r in with_ctx) / len(with_ctx) * 100, 1) if with_ctx else 0.0
    noctx_rate = round(sum(r.top1_correct for r in without_ctx) / len(without_ctx) * 100, 1) if without_ctx else 0.0
    m4 = round(ctx_rate - noctx_rate, 1)
    m5 = round(sum(r.top1_correct for r in hm) / len(hm) * 100, 1) if hm else 0.0

    return {"total": total, "m1": m1, "m2": m2, "m3": m3, "m4": m4, "m5": m5}


def _judge(value: float, target: float) -> str:
    return "PASS" if value >= target else "FAIL"


def determine_overall(metrics: dict) -> tuple[str, str]:
    m1 = metrics["m1"]
    if m1 >= 70:
        return "PASS", f"M1 Top-1 {m1}% ≥ 70% 基準達成。Phase 2 へ進む。"
    elif m1 >= 50:
        return "NEEDS_IMPROVEMENT", f"M1 Top-1 {m1}%。改善手段A〜C を順次適用し再評価。"
    else:
        return "FAIL", f"M1 Top-1 {m1}% < 50%。学習パイプライン根本見直し（殿エスカレ即時）。"


def render_report(
    metrics: dict,
    results: list[EvalResult],
    model_path: Path,
    testset_path: Path,
    mozc_dir: Path,
    lm: LanguageModel,
    verbose: bool,
) -> str:
    now = datetime.now().strftime("%Y-%m-%d %H:%M")
    m1, m2, m3, m4, m5 = metrics["m1"], metrics["m2"], metrics["m3"], metrics["m4"], metrics["m5"]
    size_mb = lm.file_size_mb()
    n_gram = lm.n_gram_order()
    n_gram_str = f"{n_gram}-gram" if n_gram else "?-gram"
    verdict, reason = determine_overall(metrics)

    lines = [
        "# LM Evaluation Report (P2-T1 Viterbi)",
        "",
        f"- Date: {now}",
        f"- Model: {model_path} ({size_mb:.1f} MB, {n_gram_str})",
        f"- Test set: {testset_path} ({metrics['total']} samples)",
        f"- Mozc dict: {mozc_dir.name}",
        f"- Algorithm: direct lookup + Viterbi fallback (beam=16) + global batch KenLM",
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
        "",
        "## ベースライン比較",
        "",
        "- Baseline M1 (mozc greedy + LM, 2026-04-19): 58.5%",
        f"- P2-T1 M1 (direct lookup + Viterbi fallback, 2026-04-20): {m1}%",
        f"- Δ F 連文節短文: baseline=0.0% → {m3.get('multi_bunsetsu', 0.0)}%",
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

    lines += ["", "## 総合判定", "", f"**{verdict}** — {reason}"]
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description="N-gram LM accuracy evaluator for llmime")
    parser.add_argument("--testset", required=True, type=Path)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--mozc-dict", type=Path, dest="mozc_dict", default=None)
    parser.add_argument("--vibrato-dict", type=Path, dest="vibrato_dict", default=None)
    parser.add_argument("--top-k", type=int, default=5)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--category-filter", default="")
    parser.add_argument("--idiom-aliases", type=Path, dest="idiom_aliases", default=None)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    mozc_dir = args.mozc_dict or (
        Path("vendor/mozc_oss") if Path("vendor/mozc_oss").exists() else None
    )
    if mozc_dir is None:
        print("[ERROR] --mozc-dict required (or vendor/mozc_oss must exist)", file=sys.stderr)
        sys.exit(1)
    if not args.model.exists():
        print(f"[ERROR] Model not found: {args.model}", file=sys.stderr)
        sys.exit(1)

    try:
        lm = LanguageModel(args.model)
    except RuntimeError as e:
        print(f"[ERROR] {e}", file=sys.stderr)
        sys.exit(1)

    if not mozc_dir.exists():
        print(f"[ERROR] mozc dict dir not found: {mozc_dir}", file=sys.stderr)
        sys.exit(1)
    print(f"[INFO] Loading MozcReadingIndex from {mozc_dir} ...", file=sys.stderr)
    index = MozcReadingIndex(mozc_dir)
    print(f"[INFO] MozcReadingIndex loaded ({len(index.map)} unique readings).", file=sys.stderr)

    items = load_testset(args.testset, args.category_filter)
    print(f"[INFO] Loaded {len(items)} test items.", file=sys.stderr)

    idiom_aliases: dict[str, list[str]] = {}
    if args.idiom_aliases:
        if not args.idiom_aliases.exists():
            print(f"[ERROR] idiom-aliases file not found: {args.idiom_aliases}", file=sys.stderr)
            sys.exit(1)
        with open(args.idiom_aliases, encoding="utf-8") as f:
            idiom_aliases = json.load(f)
        print(f"[INFO] Loaded {len(idiom_aliases)} idiom alias entries.", file=sys.stderr)

    results = evaluate(items, index, lm, args.top_k, args.verbose, idiom_aliases=idiom_aliases)

    metrics = compute_metrics(results)
    print(f"[INFO] M1={metrics['m1']}%  M2={metrics['m2']}%", file=sys.stderr)

    report = render_report(metrics, results, args.model, args.testset, mozc_dir, lm, args.verbose)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(report, encoding="utf-8")
    print(f"[INFO] Report written to {args.output}", file=sys.stderr)

    verdict, _ = determine_overall(metrics)
    if verdict == "FAIL":
        sys.exit(3)


if __name__ == "__main__":
    main()
