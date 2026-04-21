#!/usr/bin/env bash
# scripts/package_macos.sh — .app バンドル生成スクリプト
# 使用法: bash scripts/package_macos.sh <version> [arch]
# 例: bash scripts/package_macos.sh v0.1.0 aarch64-apple-darwin
set -euo pipefail

VERSION="${1:?Usage: $0 <version> [arch]}"
ARCH="${2:-aarch64-apple-darwin}"
VERSION_CLEAN="${VERSION#v}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$ROOT/target/$ARCH/release/llmime"
BUNDLE_DIR="$ROOT/dist"
BUNDLE="$BUNDLE_DIR/llmime.app"
CONTENTS="$BUNDLE/Contents"

# --- 事前チェック ---
if [[ ! -f "$BINARY" ]]; then
  echo "❌ バイナリが見つかりません: $BINARY"
  echo "   先に cargo build -p llmime-imk --release --target $ARCH を実行してください"
  exit 1
fi

# --- バンドルディレクトリ初期化 (冪等) ---
rm -rf "$BUNDLE"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

# --- バイナリ配置 ---
cp "$BINARY" "$CONTENTS/MacOS/llmime"
chmod +x "$CONTENTS/MacOS/llmime"

# --- Info.plist 生成 (テンプレートから @VERSION@ 置換) ---
PLIST_IN="$ROOT/resources/macos/Info.plist.in"
if [[ ! -f "$PLIST_IN" ]]; then
  echo "❌ Info.plist.in が見つかりません: $PLIST_IN"
  exit 1
fi
sed "s/@VERSION@/$VERSION_CLEAN/g" "$PLIST_IN" > "$CONTENTS/Info.plist"

# --- リソース・ローカライズのコピー ---
if [[ -d "$ROOT/resources/macos/Resources" ]]; then
  cp -R "$ROOT/resources/macos/Resources/"* "$CONTENTS/Resources/" 2>/dev/null || true
fi
if [[ -d "$ROOT/resources/macos/ja.lproj" ]]; then
  cp -R "$ROOT/resources/macos/ja.lproj" "$CONTENTS/Resources/"
fi

# --- PkgInfo (任意だが macOS の慣習) ---
printf 'APPL????' > "$CONTENTS/PkgInfo"

echo "✅ $BUNDLE を生成しました (version=$VERSION_CLEAN, arch=$ARCH)"
echo "   サイズ: $(du -sh "$BUNDLE" | cut -f1)"
