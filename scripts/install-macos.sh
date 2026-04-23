#!/usr/bin/env bash
set -euo pipefail

# llmime macOS インストールスクリプト
# ADP登録前の暫定インストール手順 (将来 .pkg インストーラに置き換え予定)

INSTALL_DIR="/Library/Input Methods"
APP_NAME="llmime.app"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_PATH="$SCRIPT_DIR/$APP_NAME"

# --- 事前チェック ---
echo "=== llmime インストーラ (ADP未登録版) ==="

# アーキテクチャチェック
if [[ "$(uname -m)" != "arm64" ]]; then
  echo "⚠️  警告: このビルドは Apple Silicon (arm64) 向けです。"
  echo "   Intel Mac での動作は保証されません。"
  read -r -p "続行しますか？ [y/N] " yn
  [[ "$yn" =~ ^[Yy]$ ]] || exit 1
fi

# macOS バージョンチェック (13.0以上)
MACOS_VER=$(sw_vers -productVersion)
MACOS_MAJOR=$(echo "$MACOS_VER" | cut -d. -f1)
if [[ "$MACOS_MAJOR" -lt 13 ]]; then
  echo "❌ macOS 13.0 以上が必要です (現在: $MACOS_VER)"
  exit 1
fi

# .app の存在確認
if [[ ! -d "$APP_PATH" ]]; then
  echo "❌ $APP_PATH が見つかりません。"
  echo "   このスクリプトは llmime.app と同じディレクトリに置いてください。"
  exit 1
fi

# --- Quarantine 属性除去 (Gatekeeper 回避) ---
echo "→ Gatekeeper quarantine 属性を除去します..."
xattr -dr com.apple.quarantine "$APP_PATH" 2>/dev/null || true
echo "  完了"

# --- /Library/Input Methods への配置 ---
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
INSTALL_DEST="$INSTALL_DIR/$APP_NAME"

# インストール前: 旧登録を解除 (二重登録防止)
if [[ -x "$LSREGISTER" ]] && [[ -d "$INSTALL_DEST" ]]; then
  echo "→ 旧 LS DB 登録を解除中..."
  "$LSREGISTER" -u "$INSTALL_DEST" 2>/dev/null || true
  echo "  完了"
fi

echo "→ $INSTALL_DIR/$APP_NAME にインストールします (sudo 権限が必要です)..."
sudo cp -R "$APP_PATH" "$INSTALL_DIR/"
echo "  完了"

# --- LS DB 再登録 (IMK リロード) ---
echo "→ LS DB に新バイナリを登録中..."
if [[ -x "$LSREGISTER" ]]; then
  "$LSREGISTER" -f "$INSTALL_DEST" 2>/dev/null || true
else
  echo "[install] WARNING: lsregister 未検出。killall -HUP UserEventAgent にフォールバック"
  killall -HUP UserEventAgent 2>/dev/null || true
fi
sleep 1
echo "  完了"

# --- 次のステップ案内 ---
echo ""
echo "✅ llmime のインストールが完了しました。"
echo ""
echo "【次のステップ】"
echo "1. システム設定 → キーボード → 入力ソース → 「+」ボタン"
echo "   → 「日本語」→「llmime」を追加してください"
echo "2. Accessibility 権限を付与してください:"
echo "   システム設定 → プライバシーとセキュリティ → アクセシビリティ → llmime を有効化"
echo "3. Input Monitoring 権限を付与してください:"
echo "   システム設定 → プライバシーとセキュリティ → 入力監視 → llmime を有効化"
echo ""
echo "詳細な手順: docs/install-macos.md を参照してください"
echo ""
echo "⚠️  注意: このビルドはコード署名なし(ADP未登録)です。"
echo "   初回起動時に「開発元不明」警告が出た場合は、"
echo "   Finder で右クリック → 「開く」を選択してください。"
