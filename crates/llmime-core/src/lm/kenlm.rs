use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _};

use super::LanguageModel;

/// KenLM N-gram language model (Phase 1: subprocess backend).
///
/// Calls the KenLM `query` CLI tool as a subprocess so the trait contract
/// can be fulfilled without a Rust FFI binding.  A future phase can swap
/// in a proper crate (e.g. `kenlm`) without changing call sites.
pub struct KenLMModel {
    model_path: PathBuf,
}

impl LanguageModel for KenLMModel {
    fn score(&self, words: &[&str]) -> f64 {
        if words.is_empty() {
            return f64::NEG_INFINITY;
        }

        let sentence = words.join(" ");

        // `kenlm query -n <model> < <sentence>` prints log10 probability per line.
        let output = Command::new("query")
            .arg("-n")
            .arg(&self.model_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write as _;
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(sentence.as_bytes());
                    let _ = stdin.write_all(b"\n");
                }
                child.wait_with_output()
            });

        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                // The `query` tool outputs "Total: <log10_prob>" either on its own
                // line or tab-separated on the last word line (single-sentence mode).
                text.find("Total:")
                    .and_then(|pos| text[pos + "Total:".len()..].split_whitespace().next())
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(f64::NEG_INFINITY)
            }
            Err(_) => f64::NEG_INFINITY,
        }
    }

    fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            bail!("model file not found: {}", path.display());
        }
        let model_path = path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize: {}", path.display()))?;
        Ok(Self { model_path })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    struct MockLM;

    impl LanguageModel for MockLM {
        fn score(&self, words: &[&str]) -> f64 {
            if words.is_empty() {
                f64::NEG_INFINITY
            } else {
                -(words.len() as f64)
            }
        }

        fn load(path: &Path) -> anyhow::Result<Self> {
            if !path.exists() {
                anyhow::bail!("not found: {}", path.display());
            }
            Ok(Self)
        }
    }

    #[test]
    fn mock_score_empty_returns_neg_infinity() {
        let lm = MockLM;
        assert_eq!(lm.score(&[]), f64::NEG_INFINITY);
    }

    #[test]
    fn mock_score_nonempty_returns_negative() {
        let lm = MockLM;
        assert!(lm.score(&["きかん", "しゃ"]) < 0.0);
    }

    #[test]
    fn kenlm_load_nonexistent_path_returns_err() {
        let result = KenLMModel::load(Path::new("/nonexistent/model.klm"));
        assert!(result.is_err());
    }
}
