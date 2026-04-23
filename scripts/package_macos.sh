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

# --- LS DB 登録解除 (dist は配布物ではなく作業ファイル — 二重登録防止) ---
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
if [[ ! -x "$LSREGISTER" ]]; then
  LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"
fi
if [[ -x "$LSREGISTER" ]]; then
  echo "[package] LS DB から dist/llmime.app を解除中..."
  "$LSREGISTER" -u "$BUNDLE" 2>/dev/null || true
else
  echo "[package] WARNING: lsregister が見つからない (macOS バージョン変更の可能性)"
fi

# --- plutil -lint: Info.plist 整合性チェック ---
if command -v plutil &>/dev/null; then
  plutil -lint "$CONTENTS/Info.plist" || { echo "[package] ERROR: Info.plist が不正"; exit 1; }
  for KEY in NSPrincipalClass CFBundleIdentifier CFBundleExecutable \
             InputMethodConnectionName; do
    /usr/libexec/PlistBuddy -c "Print :${KEY}" "$CONTENTS/Info.plist" &>/dev/null || \
      { echo "[package] ERROR: 必須キー '${KEY}' が存在しない"; exit 1; }
  done
  # tsInputModeListKey / tsVisibleInputModeOrderedArrayKey は ComponentInputModeDict 直下
  for NESTED_KEY in tsInputModeListKey tsVisibleInputModeOrderedArrayKey; do
    /usr/libexec/PlistBuddy -c "Print :ComponentInputModeDict:${NESTED_KEY}" "$CONTENTS/Info.plist" &>/dev/null || \
      { echo "[package] ERROR: ComponentInputModeDict.${NESTED_KEY} が存在しない"; exit 1; }
  done
  echo "[package] Info.plist 検証 OK"
else
  echo "[package] WARNING: plutil 未検出 (Linux環境では skip)"
fi

echo "✅ $BUNDLE を生成しました (version=$VERSION_CLEAN, arch=$ARCH)"
echo "   サイズ: $(du -sh "$BUNDLE" | cut -f1)"
