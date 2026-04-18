#!/usr/bin/env bash
# Wikipedia(ja) 最新 dumpをダウンロード
# 容量: 約3GB (bz2圧縮)
# 展開後: 約10GB
set -euo pipefail

DATA_DIR="${1:-./data/wiki}"
DUMP_URL="https://dumps.wikimedia.org/jawiki/latest/jawiki-latest-pages-articles.xml.bz2"
DUMP_FILE="${DATA_DIR}/jawiki-latest-pages-articles.xml.bz2"

mkdir -p "${DATA_DIR}"

if [[ -f "${DUMP_FILE}" ]]; then
    echo "[INFO] dump already exists: ${DUMP_FILE}"
    exit 0
fi

echo "[INFO] Downloading Wikipedia(ja) dump (~3GB)..."
echo "[INFO] URL: ${DUMP_URL}"
echo "[INFO] Destination: ${DUMP_FILE}"

if command -v wget &>/dev/null; then
    wget -c -O "${DUMP_FILE}" "${DUMP_URL}"
elif command -v curl &>/dev/null; then
    curl -L -C - -o "${DUMP_FILE}" "${DUMP_URL}"
else
    echo "[ERROR] wget or curl is required" >&2
    exit 1
fi

echo "[INFO] Download complete: ${DUMP_FILE}"
