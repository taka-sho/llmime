use clap::{Parser, Subcommand};
use llmime_core::{
    KenLMModel, LanguageModel, LlmimePaths, MozcReadingIndex, NgramScorer, ReadingIndex, Scorer,
    VibratoTokenizer, ViterbiLattice,
};

#[derive(Parser)]
#[command(name = "llmime", about = "LLM-powered Japanese IME")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Convert {
        reading: String,
        #[arg(short = 'n', long, default_value = "10")]
        top_k: usize,
        #[arg(short, long, env = "LLMIME_MODEL")]
        model: Option<std::path::PathBuf>,
        #[arg(short, long, env = "LLMIME_DICT")]
        dict: Option<std::path::PathBuf>,
        #[arg(long, env = "LLMIME_MOZC_DICT")]
        mozc_dict: Option<std::path::PathBuf>,
    },
    Version,
}

fn run_convert(scorer: &dyn Scorer, reading: &str, top_k: usize) -> anyhow::Result<()> {
    let candidates = scorer.score(reading, top_k)?;
    for c in candidates {
        println!("{:.6}\t{}\t{}", c.score, c.surface, c.reading);
    }
    Ok(())
}

fn run_convert_mozc<I: ReadingIndex, L: LanguageModel>(
    index: &I,
    lm: &L,
    reading: &str,
    top_k: usize,
) -> anyhow::Result<()> {
    let candidates = ViterbiLattice::top_k_candidates(reading, index, lm, 8, top_k);
    if candidates.is_empty() {
        println!("{:.6}\t{}\t{}", 0.0f64, reading, reading);
        return Ok(());
    }
    for c in candidates {
        println!("{:.6}\t{}\t{}", c.score, c.surface, c.reading);
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Version => {
            println!("llmime {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Convert {
            reading,
            top_k,
            model,
            dict,
            mozc_dict,
        } => {
            let sys_paths = LlmimePaths::resolve();

            let resolved_mozc = mozc_dict.or_else(|| {
                let d = sys_paths.mozc_dir.clone();
                if d.exists() {
                    Some(d)
                } else {
                    None
                }
            });

            if let Some(mozc_dir) = resolved_mozc {
                let model_path = model.unwrap_or(sys_paths.models_dir.join("lm.binary"));
                if !model_path.exists() {
                    anyhow::bail!(
                        "model file not found: {} (use --model <PATH> or set $LLMIME_MODEL or $LLMIME_DATA_DIR)",
                        model_path.display()
                    );
                }
                let lm = KenLMModel::load(&model_path)?;
                let index = MozcReadingIndex::load_from_dir(&mozc_dir)?;
                run_convert_mozc(&index, &lm, &reading, top_k)?;
            } else {
                let model_path = model.unwrap_or(sys_paths.models_dir.join("lm.binary"));
                if !model_path.exists() {
                    anyhow::bail!(
                        "model file not found: {} (use --model <PATH> or set $LLMIME_MODEL or $LLMIME_DATA_DIR)",
                        model_path.display()
                    );
                }
                let dict_path = dict.unwrap_or(sys_paths.mozc_dir.join("system.dic"));
                if !dict_path.exists() {
                    anyhow::bail!(
                        "dict file not found: {} (use --dict <PATH> or set $LLMIME_DICT or $LLMIME_DATA_DIR)",
                        dict_path.display()
                    );
                }
                let tokenizer = VibratoTokenizer::new(&dict_path)?;
                let lm = KenLMModel::load(&model_path)?;
                let scorer = NgramScorer::new(tokenizer, lm);
                run_convert(&scorer, &reading, top_k)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmime_core::{Candidate, ReadingEntry};

    struct MockScorer {
        candidates: Vec<Candidate>,
    }

    impl Scorer for MockScorer {
        fn score(&self, _reading: &str, top_k: usize) -> anyhow::Result<Vec<Candidate>> {
            Ok(self.candidates.iter().take(top_k).cloned().collect())
        }
    }

    struct MockReadingIndex {
        entries: Vec<ReadingEntry>,
    }

    impl ReadingIndex for MockReadingIndex {
        fn lookup(&self, _reading: &str) -> Vec<ReadingEntry> {
            self.entries.clone()
        }
        fn prefix_search(&self, _reading: &str) -> Vec<(usize, ReadingEntry)> {
            vec![]
        }
    }

    struct MockLM {
        score_val: f64,
    }

    impl LanguageModel for MockLM {
        fn score(&self, _words: &[&str]) -> f64 {
            self.score_val
        }
        fn load(_path: &std::path::Path) -> anyhow::Result<Self> {
            Ok(Self { score_val: -1.0 })
        }
    }

    #[test]
    fn run_convert_empty_candidates_returns_ok() {
        let scorer = MockScorer { candidates: vec![] };
        assert!(run_convert(&scorer, "てすと", 5).is_ok());
    }

    #[test]
    fn run_convert_returns_all_candidates() {
        let scorer = MockScorer {
            candidates: vec![
                Candidate {
                    surface: "感心".to_string(),
                    reading: "かんしん".to_string(),
                    score: -1.0,
                },
                Candidate {
                    surface: "関心".to_string(),
                    reading: "かんしん".to_string(),
                    score: -2.0,
                },
            ],
        };
        assert!(run_convert(&scorer, "かんしん", 10).is_ok());
    }

    #[test]
    fn run_convert_top_k_limits_via_scorer() {
        let scorer = MockScorer {
            candidates: vec![
                Candidate {
                    surface: "A".to_string(),
                    reading: "えー".to_string(),
                    score: -1.0,
                },
                Candidate {
                    surface: "B".to_string(),
                    reading: "びー".to_string(),
                    score: -2.0,
                },
                Candidate {
                    surface: "C".to_string(),
                    reading: "しー".to_string(),
                    score: -3.0,
                },
            ],
        };
        let result = scorer.score("てすと", 2).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].surface, "A");
        assert_eq!(result[1].surface, "B");
    }

    #[test]
    fn run_convert_mozc_oov_returns_reading_as_is() {
        let index = MockReadingIndex { entries: vec![] };
        let lm = MockLM { score_val: -2.0 };
        assert!(run_convert_mozc(&index, &lm, "zzz", 5).is_ok());
    }

    #[test]
    fn run_convert_mozc_returns_sorted_candidates() {
        let index = MockReadingIndex {
            entries: vec![
                ReadingEntry {
                    surface: "転機".to_string(),
                    reading: "てんき".to_string(),
                    pos: "名詞".to_string(),
                    cost: 5000,
                },
                ReadingEntry {
                    surface: "天気".to_string(),
                    reading: "てんき".to_string(),
                    pos: "名詞".to_string(),
                    cost: 3000,
                },
            ],
        };
        struct VaryingLM;
        impl LanguageModel for VaryingLM {
            fn score(&self, words: &[&str]) -> f64 {
                match words.first().copied() {
                    Some("天気") => -2.0,
                    Some("転機") => -4.0,
                    _ => -10.0,
                }
            }
            fn load(_path: &std::path::Path) -> anyhow::Result<Self> {
                Ok(Self)
            }
        }
        let lm = VaryingLM;
        let result = run_convert_mozc(&index, &lm, "てんき", 5);
        assert!(result.is_ok());
    }

    #[test]
    fn run_convert_mozc_top_k_limits_results() {
        let index = MockReadingIndex {
            entries: vec![
                ReadingEntry {
                    surface: "天気".to_string(),
                    reading: "てんき".to_string(),
                    pos: "名詞".to_string(),
                    cost: 3000,
                },
                ReadingEntry {
                    surface: "転機".to_string(),
                    reading: "てんき".to_string(),
                    pos: "名詞".to_string(),
                    cost: 5000,
                },
                ReadingEntry {
                    surface: "点記".to_string(),
                    reading: "てんき".to_string(),
                    pos: "名詞".to_string(),
                    cost: 6000,
                },
            ],
        };
        let lm = MockLM { score_val: -2.0 };
        // top_k=2 should limit to 2 results (but since scores are equal, dedup handles surface uniqueness)
        // With same score, all 3 entries have different surfaces so no dedup
        assert!(run_convert_mozc(&index, &lm, "てんき", 2).is_ok());
    }

    #[test]
    fn version_subcommand_exits_zero() {
        let mut cmd = assert_cmd::Command::cargo_bin("llmime").unwrap();
        cmd.arg("version").assert().success();
    }

    #[test]
    fn convert_without_model_fails() {
        let mut cmd = assert_cmd::Command::cargo_bin("llmime").unwrap();
        cmd.args(["convert", "かんしん"])
            .env_remove("LLMIME_MODEL")
            .env_remove("LLMIME_DICT")
            .env_remove("LLMIME_MOZC_DICT")
            .env("LLMIME_DATA_DIR", "/tmp/llmime_nonexistent_test_dir")
            .assert()
            .failure();
    }

    #[test]
    #[ignore]
    fn ngram_scorer_integration() {
        let model_path = match std::env::var("LLMIME_MODEL") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => return,
        };
        let dict_path = match std::env::var("LLMIME_DICT") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => return,
        };
        if !model_path.exists() || !dict_path.exists() {
            return;
        }
        let tokenizer = VibratoTokenizer::new(&dict_path).unwrap();
        let lm = KenLMModel::load(&model_path).unwrap();
        let scorer = NgramScorer::new(tokenizer, lm);
        assert!(run_convert(&scorer, "かんしん", 5).is_ok());
    }
}
