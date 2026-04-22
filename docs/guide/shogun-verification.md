# llmime ローカル動作確認手順書（殿向け）

> **対象**: taka-sho（殿）によるローカル環境での動作確認。一般ユーザー向けの `docs/install-macos.md` ではなく、開発リポジトリからの動作確認に特化した手順書。
> **D-T5 (llmime-setup.app)** は設計レビュー中。本書は `install.sh` 経路を主体とし、D-T5 完成後に §8 を追記拡張する構成。

---

## 0. 前提環境

| 要件 | 内容 |
|------|------|
| OS | macOS 13 (Ventura) 以上 |
| CPU | Apple Silicon (arm64) — Intel Mac は v1 未対応 |
| Rust | 1.75 以上 (`rustup show` で確認) |
| ツール | `cargo`, `git`, `gh` (GitHub CLI) |
| (オプション) | ソースビルド済み `target/release/` バイナリ |

```bash
# 確認コマンド
rustup show | grep "active toolchain"
cargo --version
git --version
```

---

## 1. 入手経路

### 1.1 .zip 配布版（D-T3 CI 成果物）

GitHub Actions の `release-macos.yml` が `v*` タグ push 時に自動実行され、
`llmime-macos-<version>-arm64.zip` を GitHub Releases にアップロードする。

**GitHub Releases からダウンロード:**

```bash
# 最新リリースを取得
gh release download --repo taka-sho/llmime --pattern "*.zip"

# または手動 URL からダウンロード
# https://github.com/taka-sho/llmime/releases/latest
```

**手動ビルドして .zip を生成する場合:**

```bash
cd /path/to/llmime

# 1. arm64 向けビルド
cargo build -p llmime-imk --release --target aarch64-apple-darwin

# 2. .app バンドル生成
bash scripts/package_macos.sh v0.1.0-dev aarch64-apple-darwin
# → dist/llmime.app が生成される

# 3. .zip 化
cd dist && zip -r llmime-macos-dev-arm64.zip llmime.app
```

### 1.2 ソースビルド版（開発確認推奨）

リポジトリから直接ビルドして使う方法。CI を待たずに最新コードで確認できる。

```bash
git clone https://github.com/taka-sho/llmime.git
cd llmime

# ワークスペース全体をビルド（推奨）
cargo build --workspace --release

# 成果物
#   target/release/llmime           — CLI バイナリ
#   target/release/libllmime_imk.dylib — IMK ダイナミックライブラリ
```

---

## 2. IMK インストール手順

### 2.1 .zip 版: install.sh を使う

```bash
cd ~/Downloads
unzip llmime-macos-*.zip
cd llmime-macos-*/

# install.sh が以下を実行:
#   1. llmime.app を /Library/Input Methods/ にコピー
#   2. xattr -dr com.apple.quarantine で隔離属性を削除
#   3. UserEventAgent をリロード
bash install.sh
```

### 2.2 ソースビルド版: 手動インストール

```bash
# .app バンドルを生成（§1.2 参照）
bash scripts/package_macos.sh v0.1.0-dev aarch64-apple-darwin

# /Library/Input Methods/ に配置
sudo cp -R dist/llmime.app "/Library/Input Methods/"

# Gatekeeper 拡張属性を削除
sudo xattr -dr com.apple.quarantine "/Library/Input Methods/llmime.app"

# Input Method Server をリロード
sudo killall -HUP UserEventAgent
```

### 2.3 Gatekeeper 回避（ADP 未登録のため必要）

**方法 A: 右クリック → 開く**

1. Finder で `/Library/Input Methods/llmime.app` を右クリック（Control+クリック）
2. コンテキストメニューから「**開く**」を選択
3. 確認ダイアログで「**開く**」をクリック

**方法 B: システム設定から許可**

1. `llmime.app` をダブルクリック → 警告が出たら「キャンセル」
2. システム設定 → **プライバシーとセキュリティ**
3. 下部に表示される「**このまま開く**」をクリック

### 2.4 入力ソースへの追加

1. **システム設定** → **キーボード** → **入力ソース**
2. 右下の **「+」** ボタンをクリック
3. 左列から **「日本語」** を選択
4. 右列から **「llmime」** を選んで **「追加」**

> 一覧に表示されない場合は再ログインまたは `sudo killall -HUP UserEventAgent` を実行してから再試行。

---

## 3. TCC 権限設定

llmime の動作に2種類の権限が必要。**どちらも付与しないと変換が動作しない。**

### 3.1 アクセシビリティ（Accessibility）

| 項目 | 内容 |
|------|------|
| パス | システム設定 → プライバシーとセキュリティ → **アクセシビリティ** |
| 対象アプリ | **llmime** (バンドルID: `com.example.llmime`) |
| 操作 | トグルを **ON** |

### 3.2 入力監視（Input Monitoring）

| 項目 | 内容 |
|------|------|
| パス | システム設定 → プライバシーとセキュリティ → **入力監視** |
| 対象アプリ | **llmime** |
| 操作 | トグルを **ON** |

> 権限付与後は llmime を再起動するか、Mac をログアウト → ログインしてください。

---

## 4. 起動疎通確認

### 4.1 IME 切り替え

- メニューバーの入力ソースアイコン（あ/A）をクリック → **llmime** を選択
- またはキーボードショートカット `Ctrl+Space`（システム設定で変更可能）

### 4.2 テキストエディタでの日本語入力テスト

1. **TextEdit** を開く（または Safari のアドレスバーでも可）
2. llmime に切り替える
3. 「**とうきょう**」と入力 → 変換候補に「**東京**」が表示されることを確認
4. スペースキーで候補を選択 → 確定

### 4.3 CLI での疎通確認（ソースビルド時）

```bash
cd /path/to/llmime

# N-gram 変換テスト（API キー不要）
./target/release/llmime convert "てすと" --top-k 5

# 期待出力例:
# スコア    変換候補    読み
# -3.821    テスト      てすと
# -4.102    test        てすと
# ...
```

データが見つからない場合は `LLMIME_DATA_DIR` を指定:

```bash
export LLMIME_DATA_DIR="$HOME/Library/Application Support/llmime"
./target/release/llmime convert "てすと" --top-k 5
```

---

## 5. 動作モード切替確認

config.toml の場所:

```
~/Library/Application Support/llmime/config.toml
```

config.toml の設定例:

```toml
# input_mode: privacy | performance | pro
input_mode = "privacy"

[workers_ai]
account_id = "YOUR_ACCOUNT_ID"
api_token = "YOUR_API_TOKEN"
model_id = "@cf/qwen/qwen3-30b-a3b-fp8"
timeout_ms = 1500
retry_count = 2
cost_limit_hour = 0.10
cost_limit_day = 1.00

[local_llm]
# model_path = "/path/to/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf"
```

> 環境変数は TOML より常に優先される（`CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_API_TOKEN`, `LLMIME_INPUT_MODE` 等）。

### 5.1 N-gram モード（デフォルト / privacy）

```toml
input_mode = "privacy"
```

- API キー不要、完全オフライン動作
- 短トークン入力では全モード共通でこのエンジンが使われる

**確認**: llmime で「かんしん」→「関心 / 感心 / 歓心」の候補が表示されることを確認。

### 5.2 Workers AI モード（performance）

```toml
input_mode = "performance"
```

```bash
# 環境変数でも設定可能
export CLOUDFLARE_ACCOUNT_ID="your_account_id"
export CLOUDFLARE_API_TOKEN="your_api_token"
export LLMIME_INPUT_MODE="performance"
```

**前提**: Cloudflare Workers AI の Account ID と API Token が必要（Workers AI Read 権限）。
**確認**: 長いフレーズ（15 トークン以上）の変換でクラウド LLM による候補リランキングが動作することを確認。

> API キー未設定時はフォールバックとして N-gram モードが使われる（変換は継続する）。

### 5.3 ローカル LLM モード（pro）

```toml
input_mode = "pro"

[local_llm]
model_path = "/path/to/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf"
```

**モデルのダウンロード:**

```bash
# Hugging Face から GGUF モデルをダウンロード（例: Qwen2.5-1.5B Q4_K_M）
# Qwen2.5 は Apache 2.0 ライセンス — 商用配布可
mkdir -p ~/llmime-models
# ブラウザまたは huggingface-cli でダウンロード後に配置
ls ~/llmime-models/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf
```

**確認**: config.toml の `model_path` を設定後、llmime で変換 → ローカル LLM による候補リランキングが動作することを確認。

> モデルファイルが見つからない場合は自動的に N-gram にフォールバックする。

---

## 6. トラブルシュート

### 「開発元を確認できません」が表示される

install.sh 内の `xattr -dr` が正常に動作しているか確認:

```bash
# 隔離属性が残っていないか確認
xattr -l "/Library/Input Methods/llmime.app"

# 手動で削除
sudo xattr -dr com.apple.quarantine "/Library/Input Methods/llmime.app"
```

### IME が入力ソース一覧に表示されない

```bash
# Input Method Server を再起動
sudo killall -HUP UserEventAgent
```

上記で解消しない場合はログアウト → ログインを試みること。

### 変換候補が出ない

```bash
# データディレクトリの確認
ls "$HOME/Library/Application Support/llmime/"
# lm.binary (KenLM モデル) と vendor/mozc_oss/ (辞書) が必要

# CLI で同じ症状を再現して詳細エラーを確認
./target/release/llmime convert "てすと" 2>&1
```

### ログ確認

```bash
# 直近 5 分の llmime ログを表示
log show --predicate 'process == "llmime"' --last 5m

# リアルタイムで監視
log stream --predicate 'process == "llmime"'
```

### TCC 権限リセット（テスト用）

```bash
# 全 TCC 権限をリセット（テスト環境での再現確認用）
tccutil reset All com.example.llmime
```

> リセット後は §3 の手順で再度権限を付与すること。

### Workers AI 接続失敗

```bash
# API キーと Account ID の疎通確認
curl -s "https://api.cloudflare.com/client/v4/accounts/${CLOUDFLARE_ACCOUNT_ID}/ai/run/@cf/qwen/qwen3-30b-a3b-fp8" \
  -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"hello"}]}'
```

- `401 Unauthorized` → API トークンの権限不足（Workers AI Read 権限を確認）
- `404 Not Found` → Account ID またはモデル ID の誤り
- タイムアウト → `config.toml` の `timeout_ms` を 3000 以上に引き上げ

---

## 7. 既知制約（v1 alpha）

| 項目 | 状態 |
|------|------|
| コードサイニング | 未実装 (ADP 登録予定: 2026-10) — Gatekeeper 警告あり |
| 設定 UI (D-T5) | 設計レビュー中 — 現在は config.toml 直接編集 |
| CLI からの Workers AI 連携 | 未配線 — IMK/TSF バイナリ経由のみ有効 |
| モード切替 UI | 未実装 — config.toml または環境変数で設定 |

---

## 8. D-T5 (llmime-setup.app) — 追記予定

D-T5 は GUI セットアップアシスタント（SwiftUI 製）。設計レビュー完了後に本セクションを更新する。

現時点では §1〜§5 の手動手順を使用すること。

---

## 参考

- 一般ユーザー向けインストール: [`docs/install-macos.md`](../install-macos.md)
- ソースビルド詳細手順: [`docs/guide/local-trial.md`](local-trial.md)
- 要件定義書: [`docs/requirements.md`](../requirements.md)
- バグ報告: <https://github.com/taka-sho/llmime/issues>
