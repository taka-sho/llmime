#!/usr/bin/env bash
# Thin wrapper around evaluate_lm.py.
# Usage: ./scripts/run_eval.sh [--category-filter <cat>] [--verbose]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TESTSET="${REPO_ROOT}/tests/lm_eval/testset.csv"
MODEL="${MODEL:-${REPO_ROOT}/models/wiki-ja.5gram.bin}"
DICT="${VIBRATO_DICT:-${REPO_ROOT}/dict/system.dic}"
TOP_K="${TOP_K:-5}"
DATE=$(date +%Y%m%d)
OUTPUT="${REPO_ROOT}/reports/lm_eval_${DATE}.md"

echo "[run_eval.sh] model   : ${MODEL}"
echo "[run_eval.sh] dict    : ${DICT}"
echo "[run_eval.sh] testset : ${TESTSET}"
echo "[run_eval.sh] output  : ${OUTPUT}"

mkdir -p "${REPO_ROOT}/reports"

python3 "${REPO_ROOT}/scripts/evaluate_lm.py" \
    --testset "${TESTSET}" \
    --model "${MODEL}" \
    --vibrato-dict "${DICT}" \
    --top-k "${TOP_K}" \
    --output "${OUTPUT}" \
    "$@"

echo "[run_eval.sh] Done. See ${OUTPUT}"
