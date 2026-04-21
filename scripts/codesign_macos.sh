#!/usr/bin/env bash
set -euo pipefail
# ADP登録後、GitHub Secret に CODESIGN_IDENTITY を設定することで有効化
: "${CODESIGN_IDENTITY:?CODESIGN_IDENTITY env var not set — skipping codesign}"
ENTITLEMENTS="${CODESIGN_ENTITLEMENTS:-resources/macos/llmime.entitlements}"
codesign --deep --force --options runtime --timestamp \
  --entitlements "$ENTITLEMENTS" \
  --sign "$CODESIGN_IDENTITY" \
  dist/llmime.app
echo "[ok] codesign complete"
