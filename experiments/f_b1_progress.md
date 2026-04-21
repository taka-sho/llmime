# F-B1 実験進捗

## ベースライン
- Baseline (beam=16): F M1=6.7%
- 現在の実装 (beam=64): F M1=36.7%

## 軸1: _build_lm_sentence context (状態: 完了)
結果: 空白区切りで left/surface/right を結合するよう修正済み（Viterbiの空白区切り出力と整合）

## 軸2: BOS/EOS トークン (状態: 完了)
結果: KenLM CLI の `query -n` は BOS/EOS を自動処理。Python binding は bos=True, eos=True。
追加は不要（改善なし）。

## 軸3: cost_alpha 再calibration (状態: 完了)
結果: 0.01→0.05 に変更。F M1 に対して微改善。
f_b1_axis3.json に詳細。

## 軸4: _PASSTHROUGH_PENALTY 調整 (状態: 完了)
結果: 3.0 を維持（変更なし）。

## 追加実装: Viterbi segmentation 改善 (状態: 進行中)

### 実施済み変更
1. **空白区切りViterbi出力**: `" ".join(surfaces)` で形態素境界をKenLMに伝達
   - 効果: F M1 6.7% → 36.7% (+30pt)
2. **cost_alpha**: 0.01 → 0.05
3. **beam_width**: 16 → 64 (デフォルト)
4. **1文字読み floor**: word_len==1 の全エントリに cost floor=3000
5. **ひらがなパススルー floor**: surface==reading_slice で 3000 (word_len<=3), 800 (word_len>3)
6. **全ひらがナ surface floor**: 2000
7. **混合かな安価 floor**: _has_hira(surface) and cost<500 → 3000
8. **長さ reward**: word_len>=3 で min((word_len-2)*600, 1200) の削減

### 現在の失敗パターン（19/30 失敗）
1. **した→下 問題**: "下"(した,cost=20) が hiragana "した"(cost=6231) を圧倒
   - "図書館で勉強した"→"図書館で勉強下" (correct NOT in top 320 candidates)
   - Root cause: 単漢字2文字読みの安価エントリを floor する rule がない
2. **もっと→持っと 問題**: 名詞→名詞 POS penalty(1500×2=3000) で "もっと"+"ゆっくり" path が高コスト化
   - beam=128 では correct form が rank=21 に登場 (fixable)
3. **ふる→古 問題**: "古"(445) が "降る"(3385) を圧倒
   - 単漢字2文字読みの floor が必要

### 次の実施予定
1. beam_width デフォルトを 64 → 128 に変更
2. 名詞→名詞 POS penalty を 1500 → 300 に削減
3. mixed-hiragana cheap 閾値を 500 → 1500 に引き上げ
4. 単漢字2文字読み floor: word_len==2 かつ cost<1000 → floor=7000
   ⚠ "中"(なか), "本"(ほん) も対象になるため退行リスクあり → 要確認

## 評価コマンド (再開時)
```bash
cd /Users/taka-sho/Documents/github.com/taka-sho/llmime && \
  python3 scripts/evaluate_lm.py --testset tests/lm_eval/testset.csv \
    --model models/llmime.klm --mozc-dict vendor/mozc_oss \
    --idiom-aliases tests/lm_eval/idiom_aliases.json \
    --output /tmp/f_b1_eval.json
```
