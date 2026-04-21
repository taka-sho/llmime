#!/usr/bin/env python3
"""S-004 F-A1: Viterbi tuning grid search (27 conditions).

Measures CIR@20 for F (multi_bunsetsu) category across:
  beam_width ∈ {16, 32, 64}
  top_k ∈ {20, 40, 80}
  pos_alpha ∈ {0.5, 1.0, 2.0}

Results saved to /tmp/f_a1_grid_results.json
"""
from __future__ import annotations

import json
import sys
import time
from itertools import product
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from evaluate_lm import MozcReadingIndex, TestItem, load_testset, _viterbi_by_cost

REPO_ROOT = Path(__file__).parent.parent
TESTSET = REPO_ROOT / "tests/lm_eval/testset.csv"
MOZC_DICT = REPO_ROOT / "vendor/mozc_oss"
OUTPUT = Path("/tmp/f_a1_grid_results.json")

BEAM_WIDTHS = [16, 32, 64]
TOP_KS = [20, 40, 80]
POS_ALPHAS = [0.5, 1.0, 2.0]
CIR_AT = 20


def measure_cir(
    items: list[TestItem],
    index: MozcReadingIndex,
    beam_width: int,
    top_k: int,
    pos_alpha: float,
) -> float:
    correct = 0
    for item in items:
        # Viterbi returns [(cost, surface)] sorted ascending by cost
        cands = _viterbi_by_cost(item.reading, index, beam_width, top_k, pos_alpha)
        surfaces = [s for _, s in cands[:CIR_AT]]
        if item.expected in surfaces:
            correct += 1
    return round(correct / len(items) * 100, 1) if items else 0.0


def main() -> None:
    print(f"[grid] Loading testset: {TESTSET}", flush=True)
    items = load_testset(TESTSET, category_filter="multi_bunsetsu")
    print(f"[grid] F (multi_bunsetsu) items: {len(items)}", flush=True)

    print(f"[grid] Loading mozc index: {MOZC_DICT}", flush=True)
    index = MozcReadingIndex(MOZC_DICT)
    print("[grid] Index loaded.", flush=True)

    conditions = list(product(BEAM_WIDTHS, TOP_KS, POS_ALPHAS))
    print(f"[grid] Running {len(conditions)} conditions (CIR@{CIR_AT}) ...", flush=True)

    results = []
    best_cir = -1.0
    best_params = {}

    for i, (bw, tk, pa) in enumerate(conditions, 1):
        t0 = time.perf_counter()
        cir = measure_cir(items, index, bw, tk, pa)
        elapsed = time.perf_counter() - t0
        entry = {
            "beam_width": bw,
            "top_k": tk,
            "pos_alpha": pa,
            f"cir_at_{CIR_AT}": cir,
            "elapsed_s": round(elapsed, 2),
        }
        results.append(entry)
        marker = " *** BEST" if cir > best_cir else ""
        print(f"  [{i:02d}/27] beam={bw:2d} top_k={tk:2d} pos_alpha={pa} → CIR@{CIR_AT}={cir}%{marker}", flush=True)
        if cir > best_cir:
            best_cir = cir
            best_params = {"beam_width": bw, "top_k": tk, "pos_alpha": pa}

    output = {
        "conditions_count": len(conditions),
        "cir_at": CIR_AT,
        "category": "multi_bunsetsu",
        "sample_count": len(items),
        "best_params": best_params,
        "best_cir": best_cir,
        "results": results,
    }
    OUTPUT.write_text(json.dumps(output, ensure_ascii=False, indent=2))
    print(f"\n[grid] Results saved to {OUTPUT}")
    print(f"[grid] Best: beam_width={best_params.get('beam_width')} top_k={best_params.get('top_k')} pos_alpha={best_params.get('pos_alpha')} → CIR@{CIR_AT}={best_cir}%")


if __name__ == "__main__":
    main()
