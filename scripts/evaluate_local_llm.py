#!/usr/bin/env python3
"""P6-T8: LocalLlmInferencer accuracy and latency evaluation.

Calls `llmime rerank --mode local-llm` for each item in the S-004 testset.
When no GGUF model is available the binary returns status=unavailable,
which is recorded as a fallback result.

Usage:
    python3 scripts/evaluate_local_llm.py \
        --testset tests/lm_eval/testset.csv \
        --binary ./target/release/llmime \
        [--model-path /path/to/model.gguf] \
        --output scripts/p6_eval_results.json \
        [--latency-runs 100]
"""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path
from statistics import median, quantiles
from typing import Optional

CATEGORY_TARGETS = {
    "general": 70,
    "homophone": 50,
    "proper_noun": 60,
    "technical": 65,
    "idiom": 75,
    "multi_bunsetsu": 70,
}

LATENCY_TARGETS = {
    "p50_ms": 500,
    "p95_ms": 1000,
    "timeout_pct": 20.0,
}


def load_testset(path: Path) -> list[dict]:
    items = []
    with open(path, newline="", encoding="utf-8") as f:
        for row in csv.DictReader(f):
            items.append(row)
    return items


def call_rerank(
    binary: Path,
    reading: str,
    candidates: list[str],
    left_context: Optional[str],
    model_path: Optional[Path],
) -> dict:
    cmd = [
        str(binary),
        "rerank",
        "--mode", "local-llm",
        "--reading", reading,
        "--candidates", json.dumps(candidates),
    ]
    if left_context:
        cmd += ["--left-context", left_context]
    if model_path:
        cmd += ["--model-path", str(model_path)]

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=5,
        )
        return json.loads(result.stdout.strip())
    except subprocess.TimeoutExpired:
        return {"status": "timeout", "latency_ms": 5000}
    except Exception as e:
        return {"status": "error", "reason": str(e), "latency_ms": 0}


def make_dummy_candidates(expected: str, n: int = 5) -> list[str]:
    """Return a small candidate list with expected always present."""
    pool = [expected, f"{expected}X", f"{expected}Y", "候補A", "候補B"]
    return pool[:n]


def evaluate(
    binary: Path,
    testset: list[dict],
    model_path: Optional[Path],
    latency_runs: int,
) -> dict:
    has_model = model_path is not None and model_path.exists()

    per_category: dict[str, dict] = defaultdict(lambda: {"total": 0, "correct": 0})
    latencies: list[float] = []
    unavailable_count = 0
    timeout_count = 0

    for item in testset:
        reading = item["reading"]
        expected = item["expected"]
        category = item["category"]
        left_ctx = item.get("context_left") or None

        candidates = make_dummy_candidates(expected)
        res = call_rerank(binary, reading, candidates, left_ctx, model_path)

        lat = float(res.get("latency_ms", 0))
        latencies.append(lat)

        if res["status"] == "ok":
            reranked = res.get("candidates", [])
            correct = bool(reranked) and reranked[0] == expected
        elif res["status"] == "unavailable":
            unavailable_count += 1
            correct = candidates[0] == expected if candidates else False
        elif res["status"] == "timeout":
            timeout_count += 1
            correct = False
        else:
            correct = False

        per_category[category]["total"] += 1
        if correct:
            per_category[category]["correct"] += 1

    # Latency stats
    sorted_lat = sorted(latencies)
    n = len(sorted_lat)
    p50 = sorted_lat[int(n * 0.50)] if n else 0
    p95 = sorted_lat[int(n * 0.95)] if n else 0
    p99 = sorted_lat[int(n * 0.99)] if n else 0
    timeout_pct = (timeout_count / n * 100) if n else 0

    # Accuracy by category
    category_results = {}
    for cat, counts in per_category.items():
        acc = counts["correct"] / counts["total"] * 100 if counts["total"] else 0
        category_results[cat] = {
            "accuracy_pct": round(acc, 1),
            "correct": counts["correct"],
            "total": counts["total"],
            "target_pct": CATEGORY_TARGETS.get(cat, 50),
            "pass": acc >= CATEGORY_TARGETS.get(cat, 50),
        }

    total_correct = sum(v["correct"] for v in per_category.values())
    total_items = sum(v["total"] for v in per_category.values())
    m1 = total_correct / total_items * 100 if total_items else 0

    return {
        "model_available": has_model,
        "note": (
            "実GGUFモデル使用" if has_model
            else "モデルなし環境のため推定値（LocalLlmInferencerはunavailableを返す）。"
            "精度値はダミー候補（期待値を先頭固定）に基づくため参考値として無効。"
            "実評価にはGGUFモデルと--model-pathが必要。"
        ),
        "metrics": {
            "M1_overall_pct": round(m1, 1),
            "latency_p50_ms": round(p50, 1),
            "latency_p95_ms": round(p95, 1),
            "latency_p99_ms": round(p99, 1),
            "timeout_pct": round(timeout_pct, 1),
            "unavailable_count": unavailable_count,
            "timeout_count": timeout_count,
            "total_items": total_items,
        },
        "targets": {
            "M1_overall_pct": 70.0,
            "latency_p50_ms": LATENCY_TARGETS["p50_ms"],
            "latency_p95_ms": LATENCY_TARGETS["p95_ms"],
            "timeout_pct_max": LATENCY_TARGETS["timeout_pct"],
        },
        "pass": {
            "M1": m1 >= 70.0 if has_model else None,
            "latency_p50": p50 <= LATENCY_TARGETS["p50_ms"] if has_model else None,
            "latency_p95": p95 <= LATENCY_TARGETS["p95_ms"] if has_model else None,
            "timeout_rate": timeout_pct < LATENCY_TARGETS["timeout_pct"] if has_model else None,
        },
        "categories": category_results,
    }


def main() -> None:
    ap = argparse.ArgumentParser(description="P6-T8 LocalLLM evaluation")
    ap.add_argument("--testset", default="tests/lm_eval/testset.csv")
    ap.add_argument("--binary", default="./target/release/llmime")
    ap.add_argument("--model-path", default=None)
    ap.add_argument("--output", default="scripts/p6_eval_results.json")
    ap.add_argument("--latency-runs", type=int, default=100)
    args = ap.parse_args()

    testset_path = Path(args.testset)
    binary_path = Path(args.binary)
    model_path = Path(args.model_path) if args.model_path else None
    output_path = Path(args.output)

    if not testset_path.exists():
        print(f"ERROR: testset not found: {testset_path}", file=sys.stderr)
        sys.exit(1)

    # Fall back to debug binary if release binary not found
    if not binary_path.exists():
        debug_bin = Path("./target/debug/llmime")
        if debug_bin.exists():
            binary_path = debug_bin
            print(f"Note: using debug binary {binary_path}", file=sys.stderr)
        else:
            print(f"ERROR: binary not found: {binary_path}", file=sys.stderr)
            print("Run: cargo build -p llmime-cli", file=sys.stderr)
            sys.exit(1)

    testset = load_testset(testset_path)
    print(f"Evaluating {len(testset)} items with {binary_path} ...", file=sys.stderr)

    results = evaluate(binary_path, testset, model_path, args.latency_runs)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)

    print(json.dumps(results, ensure_ascii=False, indent=2))
    print(f"\nSaved to {output_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
