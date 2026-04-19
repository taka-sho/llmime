# Rerank Prompt Design v1

## Overview

IME candidate reranking prompt for `@cf/qwen/qwen3-30b-a3b-fp8` via Cloudflare Workers AI.

## System Prompt

```
あなたは日本語IMEのリランキングエンジンです。
[直前の文脈: {left_context}  ← left_contextがSomeのとき挿入]
読み仮名と変換候補リストを受け取り、最も自然な候補のインデックス（1始まり）と確信度を
JSONで返してください。

出力フォーマット（他のテキストは一切含めないこと）:
{"best_index": <番号>, "confidence": <0.0〜1.0>}

例1:
読み: きょう、候補: [1. 今日, 2. 京, 3. 今夕]
→ {"best_index": 1, "confidence": 0.95}

例2:
読み: かいぎ、候補: [1. 会議, 2. 怪奇, 3. 海技]
→ {"best_index": 1, "confidence": 0.9}
```

## User Prompt

```
読み: {reading}
候補: [{1. 候補1, 2. 候補2, ...}]
```

## Design Decisions

### JSON Output
- Plain number output was ambiguous and hard to detect failure
- JSON gives structured `best_index` + `confidence` for future filtering
- Fallback: if model returns plain number, `parse_best_index()` extracts it via digit scan

### left_context Integration
- Inserted in system prompt as "直前の文脈: {ctx}"
- Only added when `left_context` is `Some` and non-empty
- Enables context-aware disambiguation (e.g., "川の" + "はし" → "橋")

### Few-shot Examples
- 2 examples (B + F category) embedded in system prompt
- Chosen to demonstrate common single-kanji disambiguation
- Minimal to avoid token waste; quality > quantity

## Failure Modes

| Scenario | Behavior |
|----------|----------|
| Model returns JSON with out-of-range index | `parse_best_index` returns None, original order preserved |
| Model returns plain number (regression) | Fallback digit extraction handles it |
| Model returns garbage text | None returned, candidates unchanged |
| Network timeout | `InferenceError::Timeout` propagated |

## Evaluation

See `crates/llmime-core/tests/p4_prompt_eval.rs` for mock-based evaluation covering:
- B category: 5 basic single-kanji readings
- F category: 5 context-dependent disambiguation cases
- JSON parse failure fallback
- Prompt build performance (<1ms)
