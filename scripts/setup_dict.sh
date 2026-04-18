#!/usr/bin/env bash
# Setup Vibrato tokenizer CLI and UniDic-cwj dictionary.
# Run once before using build_lm.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DICT_DIR="${REPO_ROOT}/dict"
VIBRATO_VERSION="v0.5.0"
DICT_NAME="unidic-cwj-3_1_1"
DICT_URL="https://github.com/daac-tools/vibrato/releases/download/${VIBRATO_VERSION}/${DICT_NAME}.tar.xz"

# Ensure cargo is in PATH
if ! command -v cargo &>/dev/null; then
    if [[ -f "${HOME}/.cargo/env" ]]; then
        # shellcheck source=/dev/null
        source "${HOME}/.cargo/env"
    fi
fi

if ! command -v cargo &>/dev/null; then
    echo "[ERROR] cargo not found. Install Rust from https://rustup.rs/" >&2
    exit 1
fi

# Step 1: Install vibrato tokenize CLI
echo "[setup] Installing vibrato tokenize CLI (may take a few minutes to compile)..."
if command -v tokenize &>/dev/null; then
    echo "[setup] tokenize already installed: $(which tokenize)"
else
    cargo install --git https://github.com/daac-tools/vibrato tokenize
    echo "[setup] tokenize installed: $(which tokenize)"
fi

# Step 2: Download UniDic-cwj dictionary
mkdir -p "${DICT_DIR}"
DICT_EXTRACTED="${DICT_DIR}/${DICT_NAME}"

if [[ -f "${DICT_EXTRACTED}/system.dic.zst" ]]; then
    echo "[setup] Dictionary already exists: ${DICT_EXTRACTED}/system.dic.zst"
else
    echo "[setup] Downloading UniDic-cwj dictionary for Vibrato..."
    TARBALL="${DICT_DIR}/${DICT_NAME}.tar.xz"
    curl -L "${DICT_URL}" -o "${TARBALL}"
    tar xJf "${TARBALL}" -C "${DICT_DIR}/"
    rm -f "${TARBALL}"
    echo "[setup] Done. Dictionary: ${DICT_EXTRACTED}/system.dic.zst"
fi

echo ""
echo "[setup] Setup complete."
echo "[setup] Usage: export VIBRATO_DICT=${DICT_EXTRACTED}/system.dic.zst"
echo "[setup] Then run: bash scripts/build_lm.sh"
