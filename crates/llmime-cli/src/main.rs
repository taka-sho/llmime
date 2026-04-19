use clap::{Parser, Subcommand};
use llmime_core::{KenLMModel, LanguageModel, NgramScorer, Scorer, VibratoTokenizer};

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
        } => {
            let model_path = model.ok_or_else(|| {
                anyhow::anyhow!(
                    "model path is required: use --model <PATH> or set $LLMIME_MODEL"
                )
            })?;
            if !model_path.exists() {
                anyhow::bail!("model file not found: {}", model_path.display());
            }
            let dict_path = dict.ok_or_else(|| {
                anyhow::anyhow!(
                    "dict path is required: use --dict <PATH> or set $LLMIME_DICT"
                )
            })?;
            if !dict_path.exists() {
                anyhow::bail!("dict file not found: {}", dict_path.display());
            }
            let tokenizer = VibratoTokenizer::new(&dict_path)?;
            let lm = KenLMModel::load(&model_path)?;
            let scorer = NgramScorer::new(tokenizer, lm);
            run_convert(&scorer, &reading, top_k)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use llmime_core::Candidate;

    struct MockScorer {
        candidates: Vec<Candidate>,
    }

    impl Scorer for MockScorer {
        fn score(&self, _reading: &str, top_k: usize) -> anyhow::Result<Vec<Candidate>> {
            Ok(self.candidates.iter().take(top_k).cloned().collect())
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
