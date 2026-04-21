# F-B1 実験進捗

## ベースライン
- Baseline (beam=16, top_k=5): F M1=6.7%
- A-B1途中 (beam=64, top_k=5): F M1=36.7%

## 軸1: _build_lm_sentence context (状態: 完了)
結果: 空白区切りで left/surface/right を結合するよう修正済み（Viterbiの空白区切り出力と整合）

## 軸2: BOS/EOS トークン (状態: 完了)
結果: KenLM Python binding は bos=True, eos=True がデフォルト。追加不要（改善なし）。

## 軸3: cost_alpha 再calibration (状態: 完了)
結果: 0.01→0.05 に変更。F M1 に対して微改善。
f_b1_axis3.json に詳細。

## 軸4: _PASSTHROUGH_PENALTY 調整 (状態: 完了)
結果: 3.0 を維持（変更なし）。

## 追加実験: beam_width + top_k 拡大 (状態: 完了)

### 根本原因分析
- F category の失敗の主因: Viterbi が top_k×4 候補しか生成せず、正解がそこに含まれない
- 例: "としょかんでべんきょうした" → top_k=5 (Viterbi=20候補) では正解 rank=61 で生成されない
- KenLM は正解候補が存在すれば正しく rank=1 に昇格させられる
- beam_width 拡大で より多くの多様な候補を生成 → top_k 拡大と相乗効果

### 実験結果
| beam_width | top_k | F M1 | 全M1 |
|-----------|-------|------|------|
| 64        | 5     | 36.7% | - |
| 128       | 5     | 40.0% | - |
| 128       | 20    | 46.7% | - |
| 128       | 80    | 63.3% | - |
| 128       | 200   | **70.0%** | 81.0% (全PASS) |

### 採用パラメータ
- beam_width: 64 → **128** (デフォルト変更)
- top_k CLI デフォルト: 5 → **200**
- run_eval.sh TOP_K デフォルト: 5 → **200**

## 最終結果
- F M1: 36.7% → **70.0%** (+33.3pt) ✅ 目標達成 (≥70%)
- 全M1: 81.0% ✅ 全カテゴリPASS
- 退行: なし

## 評価コマンド (再開時)
```bash
export PATH="/Users/taka-sho/kenlm/build/bin:$PATH" && \
cd /Users/taka-sho/Documents/github.com/taka-sho/llmime && \
python3 scripts/evaluate_lm.py --testset tests/lm_eval/testset.csv \
  --model models/llmime.klm --mozc-dict vendor/mozc_oss \
  --idiom-aliases tests/lm_eval/idiom_aliases.json \
  --output /tmp/f_b1_eval.json
```
