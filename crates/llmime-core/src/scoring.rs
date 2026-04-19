use crate::lm::LanguageModel;
use crate::morphology::Tokenizer;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub surface: String,
    pub reading: String,
    pub score: f64,
}

pub trait Scorer: Send + Sync {
    /// 読み文字列から候補リストを生成し、スコア順にソートして返す
    fn score(&self, reading: &str, top_k: usize) -> anyhow::Result<Vec<Candidate>>;
}

pub struct NgramScorer<T: Tokenizer, L: LanguageModel> {
    tokenizer: T,
    lm: L,
}

impl<T: Tokenizer, L: LanguageModel> NgramScorer<T, L> {
    pub fn new(tokenizer: T, lm: L) -> Self {
        Self { tokenizer, lm }
    }
}

impl<T: Tokenizer, L: LanguageModel> Scorer for NgramScorer<T, L> {
    fn score(&self, reading: &str, top_k: usize) -> anyhow::Result<Vec<Candidate>> {
        if reading.is_empty() {
            return Ok(vec![]);
        }

        let morphemes = self.tokenizer.tokenize(reading)?;
        if morphemes.is_empty() {
            return Ok(vec![]);
        }

        // Generate candidates from all consecutive sub-sequences of morphemes
        let n = morphemes.len();
        let mut candidates = Vec::new();

        for start in 0..n {
            for end in (start + 1)..=n {
                let slice = &morphemes[start..end];
                let surface: String = slice.iter().map(|m| m.surface.as_str()).collect();
                let cand_reading: String = slice.iter().map(|m| m.reading.as_str()).collect();
                let words: Vec<&str> = slice.iter().map(|m| m.surface.as_str()).collect();
                let score = self.lm.score(&words);
                candidates.push(Candidate {
                    surface,
                    reading: cand_reading,
                    score,
                });
            }
        }

        // Sort descending by score (higher log-prob = better)
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.dedup_by(|a, b| a.surface == b.surface);
        candidates.truncate(top_k);
        Ok(candidates)
    }
}

// Re-export for lib.rs compatibility
pub use Candidate as CandidateScore;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::morphology::{Morpheme, Tokenizer};

    struct MockTokenizer {
        tokens: Vec<Morpheme>,
    }

    impl Tokenizer for MockTokenizer {
        fn tokenize(&self, _text: &str) -> anyhow::Result<Vec<Morpheme>> {
            Ok(self.tokens.clone())
        }
    }

    struct MockLM {
        score_val: f64,
    }

    impl crate::lm::LanguageModel for MockLM {
        fn score(&self, _words: &[&str]) -> f64 {
            self.score_val
        }
        fn load(_path: &std::path::Path) -> anyhow::Result<Self> {
            Ok(Self { score_val: -1.0 })
        }
    }

    fn make_morpheme(surface: &str, reading: &str) -> Morpheme {
        Morpheme {
            surface: surface.to_string(),
            reading: reading.to_string(),
            pos: "名詞".to_string(),
            pos_detail: "一般".to_string(),
        }
    }

    #[test]
    fn scorer_empty_reading_returns_empty() {
        let scorer = NgramScorer::new(MockTokenizer { tokens: vec![] }, MockLM { score_val: -1.0 });
        let result = scorer.score("", 5).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn scorer_single_morpheme_returns_one_candidate() {
        let scorer = NgramScorer::new(
            MockTokenizer {
                tokens: vec![make_morpheme("東京", "とうきょう")],
            },
            MockLM { score_val: -2.5 },
        );
        let result = scorer.score("とうきょう", 5).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surface, "東京");
        assert_eq!(result[0].reading, "とうきょう");
        assert!((result[0].score - (-2.5)).abs() < f64::EPSILON);
    }

    #[test]
    fn scorer_two_morphemes_returns_candidates_sorted() {
        struct VariedLM;
        impl crate::lm::LanguageModel for VariedLM {
            fn score(&self, words: &[&str]) -> f64 {
                match words {
                    w if w == ["東京"] => -3.0,
                    w if w == ["都"] => -4.0,
                    w if w == ["東京", "都"] => -2.0,
                    _ => -10.0,
                }
            }
            fn load(_path: &std::path::Path) -> anyhow::Result<Self> {
                Ok(Self)
            }
        }

        let scorer = NgramScorer::new(
            MockTokenizer {
                tokens: vec![
                    make_morpheme("東京", "とうきょう"),
                    make_morpheme("都", "と"),
                ],
            },
            VariedLM,
        );
        let result = scorer.score("とうきょうと", 5).unwrap();
        // 3 combinations: "東京", "都", "東京都"
        assert_eq!(result.len(), 3);
        // First should be highest score (-2.0)
        assert_eq!(result[0].surface, "東京都");
        assert!((result[0].score - (-2.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn scorer_top_k_limits_results() {
        let scorer = NgramScorer::new(
            MockTokenizer {
                tokens: vec![
                    make_morpheme("東京", "とうきょう"),
                    make_morpheme("都", "と"),
                ],
            },
            MockLM { score_val: -1.0 },
        );
        let result = scorer.score("とうきょうと", 2).unwrap();
        assert!(result.len() <= 2);
    }

    #[test]
    #[ignore]
    fn ngram_scorer_integration() {
        // Requires LLMIME_MODEL env var and dict at dict/system.dic
        let model_path = match std::env::var("LLMIME_MODEL") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => return,
        };
        let dict_path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../dict/system.dic"
        ));
        if !dict_path.exists() || !model_path.exists() {
            return;
        }
        let tokenizer = crate::morphology::VibratoTokenizer::new(dict_path).unwrap();
        let lm = crate::lm::KenLMModel::load(&model_path).unwrap();
        let scorer = NgramScorer::new(tokenizer, lm);
        let result = scorer.score("とうきょうと", 5).unwrap();
        assert!(!result.is_empty());
        // Sorted descending
        for i in 1..result.len() {
            assert!(result[i - 1].score >= result[i].score);
        }
    }
}
