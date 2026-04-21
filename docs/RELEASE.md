# Release Guide

## ADP登録後の署名・公証有効化手順

1. GitHub Settings → Secrets に以下を追加:
   - `CODESIGN_IDENTITY`: `"Developer ID Application: <Name> (<TeamID>)"`
   - `NOTARIZE_APPLE_ID`: `your-apple-id@example.com`
   - `NOTARIZE_TEAM_ID`: `XXXXXXXXXX`
   - `NOTARIZE_PASSWORD`: (App-specific password)
2. 次の tag push 以降、自動的に署名+公証が有効化されます。

### スクリプトの場所

| スクリプト | 説明 |
|-----------|------|
| `scripts/codesign_macos.sh` | Hardened Runtime でコード署名 |
| `scripts/notarize_macos.sh` | Apple 公証 + staple |
| `resources/macos/llmime.entitlements` | Entitlements (最小構成) |

### CI での使用例

```yaml
- name: Code sign
  if: env.CODESIGN_IDENTITY != ''
  run: bash scripts/codesign_macos.sh

- name: Notarize
  if: env.NOTARIZE_APPLE_ID != ''
  run: bash scripts/notarize_macos.sh
```

> Note: `NOTARIZE_STAPLE` を `0` に設定するとステープル処理をスキップします（デフォルト: `1`）。
