# llmime macOS インストールガイド

## 対応環境

- **CPU**: Apple Silicon (arm64) ※ Intel Mac は v1 未対応
- **OS**: macOS 13.0 (Ventura) 以上

---

## 1. ダウンロード

[GitHub Releases](https://github.com/taka-sho/llmime/releases/latest) から最新版の
`llmime-macos-arm64-<version>.zip` をダウンロードしてください。

### SHA256 チェックサム確認

```bash
# ダウンロードしたファイルと同じディレクトリで実行
shasum -a 256 -c llmime-macos-arm64-<version>.zip.sha256
```

`llmime-macos-arm64-<version>.zip: OK` と表示されれば正常です。

---

## 2. インストール

### 2.1 .zip 展開

```bash
cd ~/Downloads
unzip llmime-macos-arm64-*.zip
cd llmime-macos-arm64-*/
```

<!-- ADP_TEMP_START: ADP登録後(2026-10予定)に削除予定 -->
### 2.2 Gatekeeper 警告の回避（ADP登録前の暫定手順）

署名なしのアプリのため、初回起動時に「開発元を確認できません」という警告が表示されます。

**方法 A: 右クリックで開く**

1. Finder で `llmime.app` を右クリック（または Control + クリック）
2. コンテキストメニューから「開く」を選択
3. 確認ダイアログで「開く」をクリック

**方法 B: システム設定から許可**

1. `llmime.app` をダブルクリック → 警告が表示されたら「キャンセル」
2. システム設定 → プライバシーとセキュリティ
3. 「このまま開く」ボタンをクリック
<!-- ADP_TEMP_END -->

### 2.3 install.sh 実行

```bash
bash install.sh
```

このスクリプトは以下を実行します:

- `llmime.app` を `/Library/Input Methods/` にコピー
- Gatekeeper 拡張属性の削除 (`xattr -dr com.apple.quarantine`)
- 入力ソースデーモンのリロード

> **注意**: `sudo` パスワードの入力を求められる場合があります。

---

## 3. 入力ソース追加

1. **システム設定** を開く
2. **キーボード** → **入力ソース** を選択
3. 右下の **「+」** ボタンをクリック
4. 左列から **「日本語」** カテゴリを選択
5. 一覧から **llmime** を選択し **「追加」** をクリック

<!-- TODO: add screenshot (入力ソース追加ダイアログ) -->

---

## 4. 権限付与

llmime には2つの権限が必要です。

### 4.1 アクセシビリティ (Accessibility)

1. **システム設定** → **プライバシーとセキュリティ** → **アクセシビリティ**
2. **llmime** のトグルを **ON** にする

<!-- TODO: add screenshot (アクセシビリティ設定画面) -->

### 4.2 入力監視 (Input Monitoring)

1. **システム設定** → **プライバシーとセキュリティ** → **入力監視**
2. **llmime** のトグルを **ON** にする

<!-- TODO: add screenshot (入力監視設定画面) -->

> 権限付与後、llmime を再起動してください（または Mac をログアウト → ログイン）。

---

## 5. 動作確認

1. **TextEdit** を開く
2. メニューバーの入力ソースアイコンから **llmime** に切り替える
3. 「とうきょう」と入力 → 変換候補に **「東京」** が表示されることを確認

---

## 5.1 Workers AI モード（オプション）

デフォルトは N-gram モード（オフライン）です。Cloudflare Workers AI を使用するには API キーが必要です。

> **注意**: macOS IMK は環境変数を継承しないため、**必ず `config.toml` に記載**してください。

```bash
# 設定ファイルを作成
mkdir -p ~/Library/Application\ Support/llmime
cat > ~/Library/Application\ Support/llmime/config.toml << 'EOF'
[workers_ai]
account_id = "YOUR_CLOUDFLARE_ACCOUNT_ID"
api_token   = "YOUR_CLOUDFLARE_API_TOKEN"

input_mode = "performance"
EOF
```

Cloudflare ダッシュボード → Workers & Pages で Account ID を確認、API トークンは「Workers AI 読み取り」権限で発行してください。

---

## 6. アンインストール

```bash
sudo rm -rf "/Library/Input Methods/llmime.app"
killall -HUP UserEventAgent 2>/dev/null || true
```

その後、システム設定 → キーボード → 入力ソース から llmime を削除してください。

---

## 7. トラブルシューティング

### 入力ソース一覧に llmime が表示されない

```bash
killall -HUP UserEventAgent
```

を実行してから、システム設定 → キーボード → 入力ソースを再確認してください。
それでも表示されない場合は一度ログアウト → ログインをお試しください。

### 変換候補が表示されない

Console.app を開き、検索フィルタに `llmime` と入力してエラーログを確認してください。

### TCC 権限を再付与したい

システム設定 → プライバシーとセキュリティ で該当権限の llmime を一度削除し、再度追加してください。

<!-- ADP_TEMP_START: ADP登録後(2026-10予定)に削除予定 -->
### 「開発元不明」警告が繰り返し表示される

install.sh に `xattr -dr com.apple.quarantine` が含まれているか確認してください:

```bash
grep xattr install.sh
```

含まれていない場合は手動で実行してください:

```bash
sudo xattr -dr com.apple.quarantine "/Library/Input Methods/llmime.app"
```
<!-- ADP_TEMP_END -->

---

## 8. フィードバック / Issue 報告

バグ報告や機能要望は以下からお願いします:

<https://github.com/taka-sho/llmime/issues>
