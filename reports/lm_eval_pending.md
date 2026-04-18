# LM Evaluation Report — Pending

- Date: 2026-04-18
- Status: **モデル学習待ち**

## 未完了理由

本評価は以下の前提条件が整っていないため実施できなかった:

| 前提条件 | 状態 |
|----------|------|
| KenLM モデル (`models/wiki-ja.5gram.bin`) | **未生成** — `scripts/build_lm.sh` 実行待ち |
| Vibrato CLI (`vibrato`) | **未インストール** — https://github.com/daac-tools/vibrato からビルド要 |
| kenlm Python binding | **Python 3.13 非互換** — Python ≤ 3.12 環境か CLI fallback が必要 |

## 完了済み成果物

| 成果物 | 状態 |
|--------|------|
| `tests/lm_eval/testset.csv` (200件) | ✅ 完了 |
| `scripts/evaluate_lm.py` | ✅ 実装完了 |
| `scripts/test_evaluate_lm.py` (pytest 19件 PASS) | ✅ 完了 |
| `scripts/run_eval.sh` | ✅ 完了 |

## 本評価実施手順

```bash
# 1. Vibrato のビルドとインストール
cargo install vibrato
vibrato download-dict --unidic-lite dict/system.dic

# 2. KenLM モデルの学習
bash scripts/download_data.sh
bash scripts/build_lm.sh

# 3. 評価実行
bash scripts/run_eval.sh
# または
python3 scripts/evaluate_lm.py \
    --testset tests/lm_eval/testset.csv \
    --model models/wiki-ja.5gram.bin \
    --vibrato-dict dict/system.dic \
    --top-k 5 \
    --output reports/lm_eval_$(date +%Y%m%d).md
```

## 備考

- kenlm Python binding は Python 3.13 でビルドエラー（`_PyLong_AsByteArray` API変更）
- `evaluate_lm.py` は kenlm CLI (`query`) フォールバック実装済み
- モデル準備後は `bash scripts/run_eval.sh` 1コマンドで評価可能
