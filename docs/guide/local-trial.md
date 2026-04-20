# llmime ローカル試行手順書 (v1 alpha)

v1 正式リリース前に、llmime を自分の環境で試すための最短ガイドです。
3 分で CLI 変換、それ以降で IMK (macOS) / TSF (Windows) のマニュアル登録手順を案内します。

> **対象**: v1 alpha (現在 Phase 4/5 実装中)。商用配布前のためコードサイニング未実装・設定 UI なし。
> **サポート OS**: macOS 13 以降 / Windows 10・11。

---

## 1. llmime-cli 最短試行

### 1.1 前提

- Rust toolchain 1.75 以上 (`cargo --version`)
- リポジトリ clone 済み: `git clone https://github.com/taka-sho/llmime.git`
- （任意）Cloudflare Workers AI API キー — **v1 alpha CLI では未配線** (§1.4 参照)

### 1.2 ビルド

```bash
cd llmime
cargo build --workspace --release
```

成果物:
- CLI バイナリ: `target/release/llmime`
- macOS IMK 共有ライブラリ: `target/release/libllmime_imk.dylib`
- Windows TSF DLL: `target/release/llmime_tsf.dll`

### 1.3 N-gram 単独試行 (API キー不要)

v1 alpha の CLI はローカル N-gram（KenLM + Mozc 辞書 + Viterbi 格子）で動作します。

**データ配置（初回のみ）**:

```bash
# デフォルトデータディレクトリ
#   macOS:   ~/Library/Application Support/llmime/
#   Windows: %APPDATA%\llmime\
#   Linux:   ~/.local/share/llmime/
# 上書きしたい場合は LLMIME_DATA_DIR を設定
export LLMIME_DATA_DIR="$HOME/llmime-data"

mkdir -p "$LLMIME_DATA_DIR/models"
mkdir -p "$LLMIME_DATA_DIR/vendor/mozc_oss"

# 1) KenLM バイナリモデルを配置: $LLMIME_DATA_DIR/models/lm.binary
# 2) Mozc 辞書を配置:            $LLMIME_DATA_DIR/vendor/mozc_oss/{id.def,dictionary*.txt,...}
#    → ビルド済み辞書は build-artifacts/mozc_oss/ を参照
```

**変換実行**:

```bash
./target/release/llmime convert "かんしん" --top-k 10
# 出力例: スコア<TAB>変換候補<TAB>読み
# -4.521    関心    かんしん
# -4.893    感心    かんしん
# ...
```

明示的に指定したい場合:

```bash
./target/release/llmime convert "てんき" \
  --model  "$LLMIME_DATA_DIR/models/lm.binary" \
  --mozc-dict "$LLMIME_DATA_DIR/vendor/mozc_oss"
```

CLI オプション:

| オプション | 環境変数 | 説明 |
|-----------|---------|------|
| `--model <PATH>` | `LLMIME_MODEL` | KenLM バイナリモデル (`lm.binary`) |
| `--mozc-dict <DIR>` | `LLMIME_MOZC_DICT` | Mozc 辞書ディレクトリ |
| `--dict <PATH>` | `LLMIME_DICT` | Vibrato 辞書（Mozc 辞書未指定時のフォールバック） |
| `-n, --top-k <N>` | — | 上位候補数（デフォルト 10） |

### 1.4 Workers AI 連携試行について（v1 alpha 制約）

**現状**: v1 alpha の `llmime-cli` は Workers AI リランキングを公開していません。
`WorkersAIInferencer`（`crates/llmime-core/src/inference/workers_ai.rs`）はライブラリ層に実装済みですが、
CLI サブコマンドとしては未配線です。CLI で試せるのは N-gram のみです。

**Workers AI を試したい場合**（開発者向け、ライブラリ経由）:

```bash
# .env.local を llmime リポジトリ直下に作成
cat > .env.local <<'EOF'
CLOUDFLARE_ACCOUNT_ID=your_account_id
CLOUDFLARE_API_TOKEN=your_api_token
WORKERS_AI_MODEL_ID=@cf/qwen/qwen3-30b-a3b-fp8
WORKERS_AI_TIMEOUT_MS=1500
EOF

# 統合テスト経由で動作確認
cargo test -p llmime-core --test workers_ai_integration -- --ignored
```

CLI からの `--mode performance` / `--mode pro` 切替は **P4-T11 (Phase 4)** で実装予定です。
現時点では `LlmimeConfig::load()` が `LLMIME_INPUT_MODE=privacy|performance|pro` を読みますが、
IMK / TSF バイナリ経由でのみ有効です。

---

## 2. llmime-imk 手動登録手順 (macOS)

### 2.1 ビルド成果物

v1 alpha 時点では `.bundle` パッケージングスクリプトは未提供です。
共有ライブラリ (`libllmime_imk.dylib`) のみが生成されます。

```bash
cargo build -p llmime-imk --release
ls target/release/libllmime_imk.dylib
```

`.bundle` 化には Objective-C ラッパー (`LlmimeIMController.m`) と `Info.plist` を含む
`llmime.bundle/Contents/MacOS/llmime` 構造が必要です。
Phase 2 の開発環境には手動で bundle を組む手順があります — 詳細は開発チームに問い合わせてください。

### 2.2 配置とシステム登録（bundle 完成後）

```bash
# Bundle は /Library/Input Methods/ 配下に配置（全ユーザー向け）
# もしくは ~/Library/Input Methods/ （現在のユーザーのみ）
sudo cp -R llmime.bundle /Library/Input\ Methods/

# Input Method Server を再起動
sudo killall -HUP UserEventAgent
```

### 2.3 入力ソース追加

1. **システム設定 → キーボード → 入力ソース → 「+」**
2. 左カラムで **「日本語」** を選択
3. 右カラムから **「llmime」** を選んで **「追加」**
4. メニューバーの入力ソースアイコンから llmime を選択

### 2.4 Gatekeeper 警告回避

コードサイニング未実装のため、初回起動時に「開発元を確認できない」警告が出ます。

1. **システム設定 → プライバシーとセキュリティ**
2. 画面下部「llmime.bundle はブロックされました」→ **「このまま開く」**
3. 管理者パスワード入力

### 2.5 Accessibility 権限付与

llmime が preedit を表示するには Accessibility 権限が必要です（初回入力時にダイアログ表示）。

1. ダイアログの **「システム設定を開く」** をクリック
2. **プライバシーとセキュリティ → アクセシビリティ**
3. llmime のトグルを ON
4. アプリ（Safari など）を再起動して入力確認

---

## 3. llmime-tsf 手動登録手順 (Windows)

### 3.1 ビルド成果物

```powershell
cargo build -p llmime-tsf --release
dir target\release\llmime_tsf.dll
```

v1 alpha 時点ではインストーラー (`.msi`) は未提供です。`regsvr32` で直接登録します。

### 3.2 DLL 登録

管理者権限の PowerShell で実行:

```powershell
# DLL を System32 にコピー（64bit Windows）
Copy-Item .\target\release\llmime_tsf.dll C:\Windows\System32\

# COM サーバーとして登録
regsvr32 C:\Windows\System32\llmime_tsf.dll
```

成功時に「DllRegisterServer in llmime_tsf.dll succeeded」と表示されます。

### 3.3 レジストリ確認

登録状況は以下のレジストリキーで確認できます:

```
HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\CTF\TIP\{llmime の CLSID}
HKEY_CLASSES_ROOT\CLSID\{llmime の CLSID}
```

CLSID は `crates/llmime-tsf/src/tsf.rs` の `CLSID_LLMIME_TSF` 定数を参照してください。

### 3.4 日本語 IME として追加

1. **設定 → 時刻と言語 → 言語と地域**
2. **日本語 → 言語のオプション**
3. **キーボード → キーボードの追加 → llmime**
4. タスクバーの IME インジケーター（あ/A）から **llmime** を選択

### 3.5 SmartScreen 警告回避

コードサイニング未実装のため、初回登録時に SmartScreen 警告が出る場合があります。

- **「詳細情報」→「実行」** で続行
- Windows Defender がブロックする場合は一時的に除外設定

---

## 4. 既知制約 (v1 alpha)

| 項目 | 状態 | 追跡 |
|------|------|------|
| コードサイニング | 未実装 (Gatekeeper / SmartScreen 警告) | v1 GA 時点で実装予定 |
| 設定 UI | 未実装 — `~/.config/llmime/config.toml` 直接編集 | Phase 6 |
| センシティブフィールド検出 | P5 実装中 — 現在は全フィールドで N-gram のみ | P5-T1〜T6 |
| 再推論機能 (precomputed rerank) | P5.5 実装予定 | [docs/requirements.md §5-3] |
| CLI からの Workers AI 連携 | 未配線 — ライブラリ層のみ | P4-T11 |
| モード切替 UI (Privacy/Performance/Pro) | IMK/TSF のみ、CLI 未対応 | Phase 4 |
| 傾向 DB (SQLite 個人適応) | 実装済みだが UI 無 | P2-T6 完了 |

### 設定ファイル (`config.toml`) の最小例

```toml
# macOS:   ~/Library/Application Support/llmime/config.toml
# Windows: %APPDATA%\llmime\config.toml
# Linux:   ~/.config/llmime/config.toml

input_mode = "privacy"  # privacy | performance | pro

[workers_ai]
account_id = "your_account_id"
api_token = "your_api_token"
model_id = "@cf/qwen/qwen3-30b-a3b-fp8"
timeout_ms = 1500
retry_count = 2
cost_limit_hour = 0.10
cost_limit_day = 1.00

[local_llm]
# model_path = "/path/to/local/llm.gguf"  # 将来の Pro モード用
```

環境変数は常に TOML より優先されます（`CLOUDFLARE_ACCOUNT_ID` 等）。

---

## 5. トラブルシューティング

### 5.1 ビルドエラー

```bash
# 依存関係の確認
cargo check --workspace

# target/ をクリーンして再ビルド
cargo clean && cargo build --workspace --release

# ワークスペースメンバー単体でビルド確認
cargo build -p llmime-core
cargo build -p llmime-cli
```

`clang` 不足エラー（macOS）: `xcode-select --install`
`link.exe` 不足エラー（Windows）: Visual Studio Build Tools 2019+ をインストール

### 5.2 CLI が「model file not found」で失敗

```bash
# $LLMIME_DATA_DIR の解決結果を確認
./target/release/llmime convert "てすと" 2>&1 | head -3
# → model file not found: /Users/.../Application Support/llmime/models/lm.binary

# 明示的に --model を渡して解決
./target/release/llmime convert "てすと" --model /path/to/lm.binary
```

### 5.3 IMK が認識されない (macOS)

```bash
# 1) bundle が正しい場所にあるか
ls -la /Library/Input\ Methods/llmime.bundle/

# 2) Info.plist の CFBundleIdentifier 確認
plutil -p /Library/Input\ Methods/llmime.bundle/Contents/Info.plist

# 3) システムログ確認
open -a Console.app
# Console で "llmime" でフィルタ

# 4) Input Method Server 再起動
sudo killall -HUP UserEventAgent
```

### 5.4 Workers AI 接続失敗

```bash
# API キーとアカウント ID の動作確認
curl -s "https://api.cloudflare.com/client/v4/accounts/${CLOUDFLARE_ACCOUNT_ID}/ai/run/@cf/qwen/qwen3-30b-a3b-fp8" \
  -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"hello"}]}'
```

- `401 Unauthorized` → API トークン無効 or 権限不足（Workers AI Read 権限が必要）
- `404 Not Found` → アカウント ID 誤り or モデル ID 誤り
- タイムアウト → `WORKERS_AI_TIMEOUT_MS` を 3000 以上に引き上げ

### 5.5 Windows DLL 登録失敗

```powershell
# 管理者権限で PowerShell を起動していることを確認
whoami /groups | findstr "S-1-16-12288"  # Mandatory Label\High Mandatory Level

# regsvr32 が DLL を見つけられるか
Test-Path C:\Windows\System32\llmime_tsf.dll

# 登録解除して再登録
regsvr32 /u C:\Windows\System32\llmime_tsf.dll
regsvr32 C:\Windows\System32\llmime_tsf.dll

# 依存 DLL 確認 (Visual C++ ランタイム等)
dumpbin /dependents C:\Windows\System32\llmime_tsf.dll
```

---

## 参考

- 要件定義書: [docs/requirements.md](../requirements.md)
- Cargo ワークスペース構成: `crates/llmime-core`, `llmime-cli`, `llmime-imk`, `llmime-tsf`
- バグ報告: <https://github.com/taka-sho/llmime/issues>
