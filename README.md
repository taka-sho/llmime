# llmime

llmime（LLM-IME）は Rust 製の日本語 IME に LLM リランキングを統合したプロダクトです。既存 IME が変換速度を最優先とするのに対し、**変換精度・個人適応性・プライバシー透明性**を訴求点とし、macOS 13 以降と Windows 10/11 をターゲットとします。

## セットアップ

```bash
cargo build --workspace
```

## 開発ステータス

**Phase 1 進行中** — Rust ワークスペース初期化段階。コア変換エンジン（ローカル N-gram 基盤）を実装中。
