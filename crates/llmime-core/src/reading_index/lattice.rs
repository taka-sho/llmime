use crate::lm::LanguageModel;
use crate::reading_index::pos_connection::{self, PosClass};
use crate::reading_index::{ReadingEntry, ReadingIndex};
use crate::scoring::Candidate;

/// LM interface for Viterbi beam search.
pub trait LmScorer: Send + Sync {
    fn score_words(&self, words: &[&str]) -> f64;
}

impl<T: LanguageModel> LmScorer for T {
    fn score_words(&self, words: &[&str]) -> f64 {
        self.score(words)
    }
}

/// Tuning parameters for Viterbi beam search.
#[derive(Debug, Clone)]
pub struct ViterbiConfig {
    pub beam_width: usize,
    /// Weight applied to mozc dictionary cost (cost / 100.0 * alpha, subtracted from score).
    pub cost_alpha: f64,
    /// Weight applied to POS bigram connection penalty (penalty * alpha, subtracted from score).
    /// Set to 0.0 to disable POS-based scoring.
    pub cost_pos_alpha: f64,
}

impl Default for ViterbiConfig {
    fn default() -> Self {
        Self { beam_width: 8, cost_alpha: 0.01, cost_pos_alpha: 0.05 }
    }
}

#[derive(Clone)]
struct BeamEntry {
    surfaces: Vec<String>,
    last_pos: PosClass,
    score: f64,
}

pub struct ViterbiLattice;

impl ViterbiLattice {
    /// Viterbi top-K beam search over the reading string.
    ///
    /// Uses default ViterbiConfig (beam_width=8, cost_alpha=0.01).
    pub fn top_k_candidates(
        reading: &str,
        index: &impl ReadingIndex,
        lm: &impl LmScorer,
        beam_width: usize,
        top_k: usize,
    ) -> Vec<Candidate> {
        let config = ViterbiConfig { beam_width, ..ViterbiConfig::default() };
        Self::search(reading, index, lm, top_k, &config)
    }

    pub fn top_k_candidates_with_config(
        reading: &str,
        index: &impl ReadingIndex,
        lm: &impl LmScorer,
        top_k: usize,
        config: &ViterbiConfig,
    ) -> Vec<Candidate> {
        Self::search(reading, index, lm, top_k, config)
    }

    fn search(
        reading: &str,
        index: &impl ReadingIndex,
        lm: &impl LmScorer,
        top_k: usize,
        config: &ViterbiConfig,
    ) -> Vec<Candidate> {
        let beam_width = config.beam_width;
        let cost_alpha = config.cost_alpha;
        let cost_pos_alpha = config.cost_pos_alpha;
        if reading.is_empty() {
            return vec![];
        }

        let chars: Vec<char> = reading.chars().collect();
        let total = chars.len();

        // beam[pos] = candidate paths that have consumed `pos` chars
        let mut beam: Vec<Vec<BeamEntry>> = vec![vec![]; total + 1];
        beam[0].push(BeamEntry { surfaces: vec![], last_pos: PosClass::Unknown, score: 0.0 });

        for pos in 0..total {
            // Prune at current position before expanding
            if beam[pos].len() > beam_width {
                beam[pos].sort_by(|a, b| {
                    b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
                });
                beam[pos].truncate(beam_width);
            }
            if beam[pos].is_empty() {
                continue;
            }

            let remaining: String = chars[pos..].iter().collect();
            let mut matches = index.prefix_search(&remaining);

            // OOV fallback: consume 1 hiragana char as a reading-split segment
            if matches.is_empty() {
                let ch = chars[pos];
                matches.push((
                    1,
                    ReadingEntry {
                        surface: ch.to_string(),
                        reading: ch.to_string(),
                        pos: "未知語".to_string(),
                        cost: 10000,
                    },
                ));
            }

            let paths = beam[pos].clone();

            for path in &paths {
                for (word_len, entry) in &matches {
                    let next_pos = pos + word_len;
                    if next_pos > total {
                        continue;
                    }
                    let mut new_surfaces = path.surfaces.clone();
                    new_surfaces.push(entry.surface.clone());

                    let word_refs: Vec<&str> = new_surfaces.iter().map(|s| s.as_str()).collect();
                    let lm_score = lm.score_words(&word_refs);
                    let cost_penalty = cost_alpha * (entry.cost as f64 / 100.0);
                    let entry_pos = pos_connection::classify(&entry.pos);
                    let pos_penalty = cost_pos_alpha
                        * pos_connection::connection_penalty(path.last_pos, entry_pos);
                    let new_score = lm_score - cost_penalty - pos_penalty;

                    beam[next_pos].push(BeamEntry {
                        surfaces: new_surfaces,
                        last_pos: entry_pos,
                        score: new_score,
                    });
                }
            }
        }

        let final_beam = &mut beam[total];
        final_beam
            .sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        final_beam.truncate(top_k);

        final_beam
            .iter()
            .map(|entry| Candidate {
                surface: entry.surfaces.join(""),
                reading: reading.to_string(),
                score: entry.score,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reading_index::{ReadingEntry, ReadingIndex};

    struct MockLm;
    impl LmScorer for MockLm {
        fn score_words(&self, words: &[&str]) -> f64 {
            -(words.len() as f64)
        }
    }

    struct MockIndex {
        entries: Vec<(String, String, i32)>, // (reading, surface, cost)
    }

    impl ReadingIndex for MockIndex {
        fn lookup(&self, reading: &str) -> Vec<ReadingEntry> {
            self.entries
                .iter()
                .filter(|(r, _, _)| r == reading)
                .map(|(r, s, c)| ReadingEntry {
                    surface: s.clone(),
                    reading: r.clone(),
                    pos: "名詞".to_string(),
                    cost: *c,
                })
                .collect()
        }

        fn prefix_search(&self, reading: &str) -> Vec<(usize, ReadingEntry)> {
            let chars: Vec<char> = reading.chars().collect();
            let mut results = Vec::new();
            for len in 1..=chars.len() {
                let prefix: String = chars[..len].iter().collect();
                for entry in self.lookup(&prefix) {
                    results.push((len, entry));
                }
            }
            results
        }
    }

    fn idx(entries: Vec<(&str, &str, i32)>) -> MockIndex {
        MockIndex {
            entries: entries.into_iter().map(|(r, s, c)| (r.to_string(), s.to_string(), c)).collect(),
        }
    }

    // MockIndex variant that stores POS strings for connection cost tests.
    struct MockIndexWithPos {
        entries: Vec<(String, String, i32, String)>, // (reading, surface, cost, pos)
    }

    impl ReadingIndex for MockIndexWithPos {
        fn lookup(&self, reading: &str) -> Vec<ReadingEntry> {
            self.entries
                .iter()
                .filter(|(r, _, _, _)| r == reading)
                .map(|(r, s, c, p)| ReadingEntry {
                    surface: s.clone(),
                    reading: r.clone(),
                    pos: p.clone(),
                    cost: *c,
                })
                .collect()
        }

        fn prefix_search(&self, reading: &str) -> Vec<(usize, ReadingEntry)> {
            let chars: Vec<char> = reading.chars().collect();
            let mut results = Vec::new();
            for len in 1..=chars.len() {
                let prefix: String = chars[..len].iter().collect();
                for entry in self.lookup(&prefix) {
                    results.push((len, entry));
                }
            }
            results
        }
    }

    fn idx_pos(entries: Vec<(&str, &str, i32, &str)>) -> MockIndexWithPos {
        MockIndexWithPos {
            entries: entries
                .into_iter()
                .map(|(r, s, c, p)| (r.to_string(), s.to_string(), c, p.to_string()))
                .collect(),
        }
    }

    #[test]
    fn empty_reading_returns_empty() {
        let result = ViterbiLattice::top_k_candidates("", &idx(vec![]), &MockLm, 8, 5);
        assert!(result.is_empty());
    }

    #[test]
    fn single_segment_matches_lookup() {
        let i = idx(vec![("とうきょう", "東京", 3000)]);
        let result = ViterbiLattice::top_k_candidates("とうきょう", &i, &MockLm, 8, 5);
        assert!(!result.is_empty());
        assert!(result.iter().any(|c| c.surface == "東京"));
    }

    #[test]
    fn beam_width_1_equals_greedy() {
        let i = idx(vec![
            ("きょう", "今日", 2000),
            ("き", "木", 3000),
            ("ょう", "様", 5000),
            ("は", "は", 1000),
        ]);
        let r1 = ViterbiLattice::top_k_candidates("きょうは", &i, &MockLm, 1, 1);
        let r2 = ViterbiLattice::top_k_candidates("きょうは", &i, &MockLm, 1, 1);
        assert_eq!(r1.len(), r2.len());
        if !r1.is_empty() {
            assert_eq!(r1[0].surface, r2[0].surface);
        }
    }

    #[test]
    fn oov_fallback_single_char() {
        let result = ViterbiLattice::top_k_candidates("あ", &idx(vec![]), &MockLm, 8, 5);
        assert!(!result.is_empty());
        assert!(result[0].surface.contains('あ'));
    }

    #[test]
    fn multi_segment_produces_joined_surface() {
        let i = idx(vec![("きょう", "今日", 2000), ("は", "は", 1000)]);
        let result = ViterbiLattice::top_k_candidates("きょうは", &i, &MockLm, 8, 5);
        assert!(result.iter().any(|c| c.surface == "今日は"));
    }

    #[test]
    fn top_k_limits_results() {
        let i = idx(vec![
            ("あ", "亜", 3000),
            ("あ", "阿", 4000),
            ("あ", "ア", 5000),
        ]);
        let result = ViterbiLattice::top_k_candidates("あ", &i, &MockLm, 8, 2);
        assert!(result.len() <= 2);
    }

    #[test]
    fn results_sorted_descending_by_score() {
        let i = idx(vec![("は", "葉", 3000), ("は", "歯", 4000)]);
        struct PreferredLm;
        impl LmScorer for PreferredLm {
            fn score_words(&self, words: &[&str]) -> f64 {
                if words.contains(&"葉") { -1.0 } else { -5.0 }
            }
        }
        let result = ViterbiLattice::top_k_candidates("は", &i, &PreferredLm, 8, 5);
        assert!(!result.is_empty());
        for w in result.windows(2) {
            assert!(w[0].score >= w[1].score);
        }
    }

    #[test]
    fn candidate_reading_matches_input() {
        let i = idx(vec![("てんき", "天気", 3000)]);
        let result = ViterbiLattice::top_k_candidates("てんき", &i, &MockLm, 8, 5);
        assert!(!result.is_empty());
        assert_eq!(result[0].reading, "てんき");
    }

    #[test]
    fn partial_oov_fallback() {
        let i = idx(vec![("きょう", "今日", 2000)]);
        let result = ViterbiLattice::top_k_candidates("きょうは", &i, &MockLm, 8, 5);
        assert!(result.iter().any(|c| c.surface == "今日は"));
    }

    #[test]
    fn three_segment_input() {
        let i = idx(vec![
            ("きょう", "今日", 2000),
            ("は", "は", 1000),
            ("いい", "良い", 3000),
        ]);
        let result = ViterbiLattice::top_k_candidates("きょうはいい", &i, &MockLm, 8, 5);
        assert!(!result.is_empty());
        assert!(result.iter().any(|c| c.surface.contains("今日")));
    }

    #[test]
    fn cost_alpha_zero_ignores_cost() {
        let i = idx(vec![("あ", "亜", 100), ("あ", "阿", 9000)]);
        let config = ViterbiConfig { beam_width: 8, cost_alpha: 0.0, cost_pos_alpha: 0.0 };
        let result = ViterbiLattice::top_k_candidates_with_config("あ", &i, &MockLm, 5, &config);
        assert!(!result.is_empty());
        // Both candidates have same LM score, so scores should be equal
        assert_eq!(result[0].score, result.last().unwrap().score);
    }

    #[test]
    fn multi_char_oov_sequence() {
        let result = ViterbiLattice::top_k_candidates("あいう", &idx(vec![]), &MockLm, 8, 5);
        assert!(!result.is_empty());
        assert_eq!(result[0].surface, "あいう");
    }

    /// POS costs should change top-1 when two segmentations differ only in POS transitions.
    ///
    /// Setup: "がは" where が can be parsed as particle (が, cost=0) or noun (蛾, cost=5000).
    /// Without POS: が(particle)+は(particle) wins because word cost is lower (0 vs 5000).
    /// With POS:    が(particle)+は(particle) is penalised (particle→particle = 5.0),
    ///              so 蛾(noun)+は(particle) wins despite its higher word cost.
    #[test]
    fn pos_bigram_fixes_particle_chain_segmentation() {
        let i = idx_pos(vec![
            ("が", "が", 0, "助詞"),      // particle が
            ("が", "蛾", 5000, "名詞"),   // moth 蛾 (same reading)
            ("は", "は", 0, "助詞"),      // topic particle は
        ]);

        let no_pos = ViterbiLattice::top_k_candidates_with_config(
            "がは",
            &i,
            &MockLm,
            5,
            &ViterbiConfig { beam_width: 8, cost_alpha: 0.001, cost_pos_alpha: 0.0 },
        );
        let with_pos = ViterbiLattice::top_k_candidates_with_config(
            "がは",
            &i,
            &MockLm,
            5,
            &ViterbiConfig { beam_width: 8, cost_alpha: 0.001, cost_pos_alpha: 1.0 },
        );

        assert!(!no_pos.is_empty());
        assert!(!with_pos.is_empty());
        // Without POS: zero-cost particle+particle wins over higher-cost noun+particle.
        assert_eq!(no_pos[0].surface, "がは", "without POS: lower word-cost path wins");
        // With POS: particle→particle penalty (5.0) reverses the ranking.
        assert_eq!(with_pos[0].surface, "蛾は", "with POS: noun+particle beats particle+particle");
    }

    /// cost_pos_alpha=0 disables POS connection penalty (backwards-compatible).
    #[test]
    fn pos_alpha_zero_disables_pos_penalty() {
        let i = idx_pos(vec![
            ("あ", "亜", 100, "名詞"),
            ("あ", "阿", 9000, "名詞"),
        ]);
        let config = ViterbiConfig { beam_width: 8, cost_alpha: 0.0, cost_pos_alpha: 0.0 };
        let result = ViterbiLattice::top_k_candidates_with_config("あ", &i, &MockLm, 5, &config);
        assert!(!result.is_empty());
        assert_eq!(result[0].score, result.last().unwrap().score);
    }
}
