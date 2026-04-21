#!/usr/bin/env bash
set -euo pipefail
# ADP登録後、GitHub Secrets に NOTARIZE_APPLE_ID/TEAM_ID/PASSWORD を設定で有効化
: "${NOTARIZE_APPLE_ID:?}" "${NOTARIZE_TEAM_ID:?}" "${NOTARIZE_PASSWORD:?}"
ditto -c -k --keepParent dist/llmime.app dist/notarize.zip
xcrun notarytool submit dist/notarize.zip \
  --apple-id "$NOTARIZE_APPLE_ID" \
  --team-id  "$NOTARIZE_TEAM_ID" \
  --password "$NOTARIZE_PASSWORD" \
  --wait
[ "${NOTARIZE_STAPLE:-1}" = "1" ] && xcrun stapler staple dist/llmime.app
echo "[ok] notarize + staple complete"
