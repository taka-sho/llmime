#!/usr/bin/env bash
# llmime セットアップスクリプト (実機検証済み: 2026-04-18)
#
# 前提: macOS (Apple Silicon) + Homebrew + Rust/cargo インストール済み
# 検証環境: macOS Sequoia 25.3.0, Rust 1.95.0, Homebrew 4.x
#
# 使い方:
#   bash scripts/setup.sh            # 全ステップ実行
#   KENLM_INSTALL_DIR=/path/to/dir   # KenLM インストール先を変更する場合
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# KenLM バイナリのインストール先 (デフォルト: /tmp/kenlm/build/bin)
# 永続化する場合は ~/bin や /usr/local/bin に変更して PATH に追加せよ
KENLM_INSTALL_DIR="${KENLM_INSTALL_DIR:-/tmp/kenlm/build/bin}"

echo "========================================"
echo " llmime セットアップ開始"
echo " REPO_ROOT: ${REPO_ROOT}"
echo "========================================"

# ─── Step 1: cargo (Rust) ──────────────────────────────────────────────────
echo ""
echo "[Step 1] Rust/cargo の確認..."

if ! command -v cargo &>/dev/null; then
    if [[ -f "${HOME}/.cargo/env" ]]; then
        # shellcheck source=/dev/null
        source "${HOME}/.cargo/env"
    fi
fi

if ! command -v cargo &>/dev/null; then
    echo "[ERROR] cargo が見つかりません。以下からRustをインストールしてください:" >&2
    echo "        https://rustup.rs/" >&2
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
    exit 1
fi
echo "[OK] cargo: $(cargo --version)"

# ─── Step 2: vibrato tokenize CLIのインストール ────────────────────────────
echo ""
echo "[Step 2] vibrato tokenize CLI のインストール..."
# NOTE: `cargo install vibrato` はライブラリのみでCLIバイナリは含まれない。
#       git リポジトリの tokenize クレートを直接インストールする必要がある。
if command -v tokenize &>/dev/null; then
    echo "[OK] tokenize already in PATH: $(which tokenize)"
elif [[ -x "${HOME}/.cargo/bin/tokenize" ]]; then
    echo "[OK] tokenize installed at ${HOME}/.cargo/bin/tokenize"
    echo "     PATH に追加: export PATH=\"\$HOME/.cargo/bin:\$PATH\""
    export PATH="${HOME}/.cargo/bin:${PATH}"
else
    echo "[INFO] tokenize をインストール中 (数分かかる場合あり)..."
    cargo install --git https://github.com/daac-tools/vibrato tokenize
    echo "[OK] tokenize installed: $(which tokenize)"
fi

# ─── Step 3: UniDic 辞書のダウンロード ────────────────────────────────────
echo ""
echo "[Step 3] UniDic-cwj 辞書の確認..."
# setup_dict.sh が辞書DLを担当する。すでにある場合はスキップ。
DICT_PATH="${REPO_ROOT}/dict/unidic-cwj-3_1_1/system.dic.zst"
if [[ -f "${DICT_PATH}" ]]; then
    echo "[OK] 辞書確認済み: ${DICT_PATH}"
else
    echo "[INFO] 辞書をダウンロード中..."
    bash "${SCRIPT_DIR}/setup_dict.sh"
fi

# ─── Step 4: KenLM の依存ライブラリ確認 (Homebrew) ─────────────────────────
echo ""
echo "[Step 4] KenLM ビルド依存ライブラリの確認..."
MISSING_LIBS=()
for lib in cmake boost eigen; do
    if brew list "$lib" &>/dev/null 2>&1; then
        echo "[OK] ${lib} installed"
    else
        MISSING_LIBS+=("$lib")
    fi
done

if [[ ${#MISSING_LIBS[@]} -gt 0 ]]; then
    echo "[INFO] 不足ライブラリをインストール中: ${MISSING_LIBS[*]}"
    brew install "${MISSING_LIBS[@]}"
fi

# ─── Step 5: KenLM ソースビルド ───────────────────────────────────────────
echo ""
echo "[Step 5] KenLM のビルド..."
# NOTE: Homebrew formula なし。ソースビルド一択。
# NOTE: Boost 1.74+ では boost::system がヘッダオンリーになり、
#       KenLM の CMakeLists.txt が REQUIRED COMPONENTS に system を指定すると
#       cmake がエラーを返す。以下のパッチを当ててから cmake を実行する。
# NOTE: Apple Silicon Mac では cmake に -DCMAKE_OSX_ARCHITECTURES=arm64 が必要。
#       未指定だと Rosetta (x86_64) として検出され、arm64 の Homebrew ライブラリと
#       アーキテクチャ不一致でリンクエラーになる。

KENLM_SRC="/tmp/kenlm"
KENLM_BUILD="${KENLM_SRC}/build"
LMPLZ_BIN="${KENLM_INSTALL_DIR}/lmplz"
BUILD_BINARY_BIN="${KENLM_INSTALL_DIR}/build_binary"

if [[ -x "${LMPLZ_BIN}" && -x "${BUILD_BINARY_BIN}" ]]; then
    echo "[OK] KenLM バイナリ確認済み: ${KENLM_INSTALL_DIR}"
else
    if [[ ! -d "${KENLM_SRC}" ]]; then
        echo "[INFO] KenLM リポジトリをクローン中..."
        git clone https://github.com/kpu/kenlm "${KENLM_SRC}"
    fi

    # Boost 1.74+ 対応パッチ: system コンポーネントの削除
    if grep -q "  system$" "${KENLM_SRC}/CMakeLists.txt" 2>/dev/null; then
        echo "[INFO] CMakeLists.txt に Boost 互換パッチを適用中..."
        sed -i.bak '/^  system$/d' "${KENLM_SRC}/CMakeLists.txt"
    fi

    mkdir -p "${KENLM_BUILD}"
    echo "[INFO] cmake 設定中..."
    cmake -S "${KENLM_SRC}" -B "${KENLM_BUILD}" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_OSX_ARCHITECTURES=arm64

    echo "[INFO] ビルド中 (lmplz, build_binary)..."
    cmake --build "${KENLM_BUILD}" \
        --target lmplz build_binary \
        --parallel 4

    echo "[OK] KenLM バイナリ: ${KENLM_BUILD}/bin/"
fi

# ─── Step 6: PATH に KenLM を追加 ────────────────────────────────────────
# shellに追加するにはプロファイル (.zshrc 等) に以下を記載せよ:
#   export PATH="/tmp/kenlm/build/bin:$PATH"
# このスクリプトは現在のシェルセッションのみに追加する
export PATH="${KENLM_INSTALL_DIR}:${PATH}"

# ─── Step 7: 最小動作確認 ─────────────────────────────────────────────────
echo ""
echo "[Step 7] 最小動作確認..."

echo -n "  tokenize: "
echo "感心した" | tokenize -i "${DICT_PATH}" -O wakati 2>/dev/null \
    && echo "[OK]" || echo "[FAIL]"

echo -n "  lmplz:    "
if output=$("${LMPLZ_BIN}" --help 2>&1 | head -1) && [[ -n "$output" ]] \
    || [[ -n "$("${LMPLZ_BIN}" --help 2>&1 | head -1)" ]]; then
    echo "[OK]"
else
    echo "[FAIL]"
fi

echo -n "  build_binary: "
if [[ -n "$("${BUILD_BINARY_BIN}" --help 2>&1 | head -1)" ]]; then
    echo "[OK]"
else
    echo "[FAIL]"
fi

echo ""
echo "========================================"
echo " セットアップ完了"
echo "========================================"
echo ""
echo "次のステップ:"
echo "  1. Wikipedia dump のダウンロード:"
echo "       bash scripts/download_data.sh"
echo "  2. N-gram モデルの学習:"
echo "       export PATH=\"${KENLM_INSTALL_DIR}:\$PATH\""
echo "       bash scripts/build_lm.sh"
echo "  3. 評価実行:"
echo "       bash scripts/run_eval.sh"
echo ""
echo "注意: KenLM バイナリは ${KENLM_INSTALL_DIR} にある。"
echo "      永続的に使う場合は ~/bin にコピーして PATH に追加せよ:"
echo "        cp ${KENLM_INSTALL_DIR}/lmplz ~/bin/"
echo "        cp ${KENLM_INSTALL_DIR}/build_binary ~/bin/"
