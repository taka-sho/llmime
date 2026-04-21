#!/usr/bin/env python3
"""
S-005: Rerank parameter sweep (N x P x D = 27 conditions)

Metrics:
  M1 = accuracy improvement rate = (top-1 after rerank) - (top-1 before rerank)  target: >= 10%
  M2 = rerank latency p50/p95 (boundary event -> UI update)                       target: p50 <= 300ms
  M3 = false-update rate = degraded / fires                                        target: <= 5%

Simulation model:
  - 50 homophone cases from s005_testset.csv (all have context_right)
  - Confidence distribution: Gaussian(mean=0.52, std=0.12) per homophone token
  - Rerank delta: Gaussian(mean=0.25, std=0.08) when context_right present
  - Initial accuracy for homophones: 45% (ambiguous without right context)
  - False-positive rate (rerank breaks correct answer): varies by D threshold
    - D=0.15: 13% (low gate lets noise through)
    - D=0.20:  4% (balanced)
    - D=0.30:  1% (strict)
    - N reduces false-positive by 1% per token above 3 (more context = better LLM accuracy)

Latency model:
  - M2 p50 = batch_window(40ms) + LLM_p50(150ms) = 190ms (constant across conditions)
  - M2 p95 = batch_window(40ms) + LLM_p95(280ms) + queue_pressure(fire_rate * 40ms)
"""

import csv
import random
import itertools
from pathlib import Path

SEED = 42

N_VALUES = [3, 5, 7]
P_VALUES = [0.6, 0.7, 0.8]
D_VALUES = [0.15, 0.2, 0.3]

INIT_CONF_MEAN = 0.52
INIT_CONF_STD = 0.12
INIT_CORRECT_PROB = 0.45

RERANK_DELTA_MEAN_WITH_CTX = 0.25
RERANK_DELTA_STD = 0.08

BATCH_WINDOW_MS = 40
LLM_P50_MS = 150
LLM_P95_MS = 280

M1_GOAL = 0.10
M2_GOAL = 300.0
M3_GOAL = 0.05

TESTSET = Path(__file__).parent.parent / "tests/lm_eval/s005_testset.csv"


def load_cases():
    with open(TESTSET) as f:
        return list(csv.DictReader(f))


def p_wrong_given_correct(n: int, d: float) -> float:
    """Probability rerank breaks an already-correct token, given D gate."""
    base = {0.15: 0.13, 0.20: 0.04, 0.30: 0.01}[d]
    return max(0.005, base - (n - 3) * 0.01)


def p_correct_given_wrong(n: int) -> float:
    """Probability rerank fixes an incorrect token."""
    return min(0.97, 0.84 + (n - 3) * 0.02)


def simulate(cases: list, n: int, p: float, d: float, rng: random.Random):
    """
    Simulate one (N, P, D) condition against all cases.
    Returns (m1, m2_p50, m2_p95, m3, fire_count, update_count).
    """
    p_wrong = p_wrong_given_correct(n, d)
    p_fix = p_correct_given_wrong(n)

    fire_count = 0
    update_count = 0
    improved = 0
    degraded = 0

    for _ in cases:
        # Sample N token confidences for this window position
        confs = [
            max(0.0, min(1.0, rng.gauss(INIT_CONF_MEAN, INIT_CONF_STD)))
            for _ in range(n)
        ]

        fired_tokens = [c for c in confs if c < p]
        if not fired_tokens:
            continue

        fire_count += 1

        delta = max(0.0, rng.gauss(RERANK_DELTA_MEAN_WITH_CTX, RERANK_DELTA_STD))
        if delta < d:
            continue

        update_count += 1

        was_correct = rng.random() < INIT_CORRECT_PROB
        if was_correct:
            if rng.random() < p_wrong:
                degraded += 1
        else:
            if rng.random() < p_fix:
                improved += 1

    total = len(cases)
    m1 = (improved - degraded) / total
    m3 = degraded / fire_count if fire_count > 0 else 0.0

    fire_rate = fire_count / total
    m2_p50 = float(BATCH_WINDOW_MS + LLM_P50_MS)
    m2_p95 = BATCH_WINDOW_MS + LLM_P95_MS + fire_rate * 40.0

    return m1, m2_p50, m2_p95, m3, fire_count, update_count


def main():
    rng = random.Random(SEED)
    cases = load_cases()
    print(f"Test cases: {len(cases)} (homophone, all with context_right)\n")

    conditions = list(itertools.product(N_VALUES, P_VALUES, D_VALUES))
    assert len(conditions) == 27, f"expected 27 conditions, got {len(conditions)}"

    results = []
    header = f"{'N':>3} {'P':>5} {'D':>5} | {'M1':>8} {'M2p50':>7} {'M2p95':>7} {'M3':>7} | {'Fires':>5} {'Updates':>7} | OK?"
    print(header)
    print("-" * len(header))

    for n, p, d in conditions:
        m1, m2_p50, m2_p95, m3, fires, updates = simulate(cases, n, p, d, rng)
        ok = m1 >= M1_GOAL and m2_p50 <= M2_GOAL and m3 <= M3_GOAL
        marker = "✓" if ok else "✗"
        print(
            f"{n:>3} {p:>5.2f} {d:>5.2f} | "
            f"{m1:>8.1%} {m2_p50:>7.0f} {m2_p95:>7.0f} {m3:>7.1%} | "
            f"{fires:>5} {updates:>7} | {marker}"
        )
        results.append(
            dict(n=n, p=p, d=d, m1=m1, m2_p50=m2_p50, m2_p95=m2_p95, m3=m3, ok=ok,
                 fires=fires, updates=updates)
        )

    passing = [r for r in results if r["ok"]]
    print(f"\n{len(passing)}/27 conditions satisfy M1≥{M1_GOAL:.0%}, M2p50≤{M2_GOAL:.0f}ms, M3≤{M3_GOAL:.0%}\n")

    if not passing:
        print("No passing conditions. Investigate model assumptions.")
        return results

    best = max(passing, key=lambda r: r["m1"] - r["m3"] * 2)
    print("=== Recommended (N*, P*, D*) ===")
    print(f"  N* = {best['n']}  (window_size in RerankConfig)")
    print(f"  P* = {best['p']}  (threshold in RerankConfig)")
    print(f"  D* = {best['d']}  (DEFAULT_MIN_CONFIDENCE_DELTA in update_gate.rs)")
    print()
    print(f"  M1 = {best['m1']:.1%}   (target ≥ {M1_GOAL:.0%})")
    print(f"  M2 p50 = {best['m2_p50']:.0f} ms, p95 = {best['m2_p95']:.0f} ms  (target p50 ≤ {M2_GOAL:.0f} ms)")
    print(f"  M3 = {best['m3']:.1%}   (target ≤ {M3_GOAL:.0%})")
    print(f"\n  Fires: {best['fires']}/50, Updates applied: {best['updates']}/50")

    return results


if __name__ == "__main__":
    main()
