#!/usr/bin/env bash
# Usage: ./scripts/build_lm.sh [data_dir] [output_dir]
# data_dir: Wikipedia(ja) dump の展開先 (default: ./data/wiki)
# output_dir: モデル出力先 (default: ./models)
set -euo pipefail

DATA_DIR="${1:-./data/wiki}"
OUTPUT_DIR="${2:-./models}"
DUMP_FILE="${DATA_DIR}/jawiki-latest-pages-articles.xml.bz2"
EXTRACTED_DIR="${DATA_DIR}/extracted"
READING_CORPUS="${DATA_DIR}/readings.txt"
ARPA_FILE="${OUTPUT_DIR}/llmime.arpa"
KLM_FILE="${OUTPUT_DIR}/llmime.klm"
NGRAM_ORDER=3

# Step 1: dumpファイル存在確認
if [[ ! -f "${DUMP_FILE}" ]]; then
    echo "[ERROR] Wikipedia dump not found: ${DUMP_FILE}" >&2
    echo "[INFO]  Run scripts/download_data.sh first, or download manually:" >&2
    echo "[INFO]  URL: https://dumps.wikimedia.org/jawiki/latest/jawiki-latest-pages-articles.xml.bz2" >&2
    exit 1
fi

# Step 2: WikiExtractorで本文抽出
if [[ ! -d "${EXTRACTED_DIR}" ]]; then
    echo "[INFO] Extracting Wikipedia articles..."
    python3 -m wikiextractor \
        --output "${EXTRACTED_DIR}" \
        --processes "$(nproc 2>/dev/null || sysctl -n hw.logicalcpu 2>/dev/null || echo 4)" \
        --no-templates \
        "${DUMP_FILE}"
else
    echo "[INFO] Extracted dir already exists, skipping extraction: ${EXTRACTED_DIR}"
fi

# Step 3: Vibratoで形態素解析 → 読み列に変換
if [[ ! -f "${READING_CORPUS}" ]]; then
    echo "[INFO] Running morphological analysis with Vibrato..."

    if ! command -v tokenize &>/dev/null; then
        echo "[ERROR] 'tokenize' command (vibrato) not found." >&2
        echo "[INFO]  Run: bash scripts/setup_dict.sh" >&2
        exit 1
    fi

    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
    _DEFAULT_DICT="${REPO_ROOT}/dict/unidic-cwj-3_1_1/system.dic.zst"
    UNIDIC_SYSTEM="${VIBRATO_DICT:-${_DEFAULT_DICT}}"
    if [[ ! -f "${UNIDIC_SYSTEM}" ]]; then
        echo "[ERROR] UniDic system.dic.zst not found: ${UNIDIC_SYSTEM}" >&2
        echo "[INFO]  Run: bash scripts/setup_dict.sh" >&2
        echo "[INFO]  Or set: export VIBRATO_DICT=/path/to/system.dic.zst" >&2
        exit 1
    fi

    find "${EXTRACTED_DIR}" -type f | sort | xargs cat | \
        tokenize -i "${UNIDIC_SYSTEM}" -O wakati \
        > "${READING_CORPUS}"
    echo "[INFO] Corpus saved: ${READING_CORPUS}"
else
    echo "[INFO] Reading corpus already exists, skipping Vibrato: ${READING_CORPUS}"
fi

mkdir -p "${OUTPUT_DIR}"

# Step 4: lmplzでN-gramカウント＋ARPA生成
if [[ ! -f "${ARPA_FILE}" ]]; then
    echo "[INFO] Building ${NGRAM_ORDER}-gram ARPA model..."

    if ! command -v lmplz &>/dev/null; then
        echo "[ERROR] 'lmplz' not found. Install KenLM: https://github.com/kpu/kenlm" >&2
        exit 1
    fi

    lmplz -o "${NGRAM_ORDER}" --text "${READING_CORPUS}" --arpa "${ARPA_FILE}"
    echo "[INFO] ARPA saved: ${ARPA_FILE}"
else
    echo "[INFO] ARPA already exists, skipping: ${ARPA_FILE}"
fi

# Step 5: ARPAをKLMバイナリに変換
echo "[INFO] Converting ARPA to binary KLM..."

if ! command -v build_binary &>/dev/null; then
    echo "[ERROR] 'build_binary' not found. Install KenLM: https://github.com/kpu/kenlm" >&2
    exit 1
fi

build_binary "${ARPA_FILE}" "${KLM_FILE}"
echo "[INFO] KLM binary saved: ${KLM_FILE}"
echo "[DONE] Language model built: ${KLM_FILE}"
