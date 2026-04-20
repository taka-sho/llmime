# llmime ローカル試行ガイド

v1 alpha 向けのローカル環境でのビルド・動作確認手順です。

## 1. llmime-cli 最短試行

### 前提条件

- Rust toolchain（stable）: `rustup show` で確認
- Cloudflare Workers AI APIキー（WorkersAI連携を使う場合のみ）

### ビルド

```bash
cargo build --workspace
```

### N-gram 単独試行（APIキー不要）

```bash
cargo run -p llmime-cli -- convert "入力テキスト"
```

N-gram モデルのみで変換候補を返します。ネットワーク接続・APIキー不要です。

### WorkersAI 連携試行（APIキー必要）

環境変数を設定してから同コマンドを実行します。

```bash
export CLOUDFLARE_API_TOKEN="your-api-token"
export CLOUDFLARE_ACCOUNT_ID="your-account-id"
cargo run -p llmime-cli -- convert "入力テキスト"
```

設定の詳細（config.toml の場所・書き方）は [config-setup.md](./config-setup.md) を参照してください。

---

## 2. llmime-imk 手動登録手順（macOS）

### ビルド成果物のパス

```
target/release/llmime-imk.bundle
```

### インストール手順

```bash
# /Library/Input Methods/ へ配置（管理者権限が必要）
sudo cp -r target/release/llmime-imk.bundle /Library/Input\ Methods/
```

### 入力ソースへの追加

1. システム設定 → キーボード → 入力ソース
2. 左下の「+」ボタンをクリック
3. 日本語カテゴリから「llmime」を選択して追加

### Gatekeeper 警告の回避

公証（notarization）は未実装のため、初回起動時に警告が表示されます。

1. システム設定 → プライバシーとセキュリティ
2. 画面下部に表示される「"llmime-imk.bundle" は開発元を確認できません」の横にある「このまま開く」をクリック

### Accessibility 権限付与

初回起動時にダイアログが表示されます。

1. 「システム設定を開く」をクリック
2. システム設定 → プライバシーとセキュリティ → アクセシビリティ
3. llmime のトグルをオンにする

---

## 3. llmime-tsf 手動登録手順（Windows）

### ビルド成果物のパス

```
target\release\llmime_tsf.dll
```

### DLL 登録

**管理者権限で**コマンドプロンプトを開き、以下を実行します。

```cmd
regsvr32 "C:\path\to\target\release\llmime_tsf.dll"
```

### レジストリ確認

```
HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\CTF\TIP
```

レジストリエディタ（`regedit`）で上記パスを開き、llmime のエントリが存在することを確認します。

### 入力メソッドとして追加

1. 設定 → 時刻と言語 → 言語と地域
2. 日本語 → 言語オプション → キーボードの追加
3. 一覧から「llmime」を選択

---

## 4. 既知制約（v1 alpha）

| 項目 | 状態 |
|------|------|
| コードサイニング | 未実装（Gatekeeper/SmartScreen 警告が出る） |
| 設定 UI | なし（config.toml 直接編集 → [config-setup.md](./config-setup.md) 参照） |
| センシティブフィールド検出 | P5 実装中（現在は全フィールドで N-gram のみ） |
| 再推論機能 | P5.5 実装予定 |

---

## 5. トラブルシューティング

### ビルドエラー

```bash
cargo check
```

依存関係の問題を確認します。Rust toolchain のバージョンが古い場合は `rustup update` を実行してください。

### IMK が認識されない（macOS）

Console.app を開き、検索フィルタに `llmime` を入力してシステムログを確認します。

Accessibility 権限が付与されているか再確認してください（[Accessibility 権限付与](#accessibility-権限付与) 参照）。

### WorkersAI 接続失敗

APIキーとアカウントID の設定を確認してください（[config-setup.md](./config-setup.md) 参照）。

### Windows DLL 登録失敗

- コマンドプロンプトを**管理者として実行**しているか確認
- DLL のフルパスが正しいか確認（スペースを含む場合はダブルクォートで囲む）
