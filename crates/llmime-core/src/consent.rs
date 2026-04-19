use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConsentError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub consented: bool,
    pub timestamp: String,
    pub version: String,
}

pub struct ConsentManager {
    consent_path: PathBuf,
}

impl ConsentManager {
    pub fn new() -> Self {
        let base = std::env::var_os("LLMIME_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::data_dir()
                    .expect("cannot resolve data dir")
                    .join("llmime")
            });
        Self {
            consent_path: base.join("consent.json"),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { consent_path: path }
    }

    pub fn is_consented(&self) -> bool {
        self.load().map(|r| r.consented).unwrap_or(false)
    }

    pub fn record_consent(&self) -> Result<(), ConsentError> {
        if let Some(parent) = self.consent_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let record = ConsentRecord {
            consented: true,
            timestamp: now_unix_str(),
            version: "1.0".to_string(),
        };
        std::fs::write(&self.consent_path, serde_json::to_string_pretty(&record)?)?;
        Ok(())
    }

    pub fn revoke_consent(&self) -> Result<(), ConsentError> {
        let mut record = if self.consent_path.exists() {
            self.load()?
        } else {
            ConsentRecord {
                consented: false,
                timestamp: now_unix_str(),
                version: "1.0".to_string(),
            }
        };
        record.consented = false;
        record.timestamp = now_unix_str();
        if let Some(parent) = self.consent_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.consent_path, serde_json::to_string_pretty(&record)?)?;
        Ok(())
    }

    fn load(&self) -> Result<ConsentRecord, ConsentError> {
        let data = std::fs::read_to_string(&self.consent_path)?;
        Ok(serde_json::from_str(&data)?)
    }
}

impl Default for ConsentManager {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("{}Z", d.as_secs()))
        .unwrap_or_else(|_| "0Z".to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use tempfile::tempdir;

    use super::*;
    use crate::inference::capabilities::InferencerCapabilities;
    use crate::inference::fallback_chain::FallbackChain;
    use crate::inference::inferencer::{
        AlwaysSucceedInferencer, CandidateSource, CandidateWithScore, Inferencer,
    };
    use crate::inference::InferenceError;

    struct ConsentCheckInferencer {
        consent_path: PathBuf,
    }

    #[async_trait]
    impl Inferencer for ConsentCheckInferencer {
        fn name(&self) -> &'static str {
            "consent-check"
        }

        fn capabilities(&self) -> InferencerCapabilities {
            InferencerCapabilities {
                supports_rerank: true,
                supports_right_context: false,
            }
        }

        async fn rerank(
            &self,
            _reading: &str,
            candidates: Vec<CandidateWithScore>,
            _left_context: Option<&str>,
        ) -> Result<Vec<CandidateWithScore>, InferenceError> {
            let cm = ConsentManager::with_path(self.consent_path.clone());
            if !cm.is_consented() {
                return Err(InferenceError::ConsentRequired);
            }
            Ok(candidates)
        }
    }

    #[test]
    fn consent_required() {
        let dir = tempdir().unwrap();
        let cm = ConsentManager::with_path(dir.path().join("consent.json"));
        assert!(!cm.is_consented());
    }

    #[test]
    fn record_consent() {
        let dir = tempdir().unwrap();
        let cm = ConsentManager::with_path(dir.path().join("consent.json"));
        assert!(!cm.is_consented());
        cm.record_consent().unwrap();
        assert!(cm.is_consented());
        let record = cm.load().unwrap();
        assert!(record.consented);
        assert_eq!(record.version, "1.0");
        assert!(!record.timestamp.is_empty());
    }

    #[test]
    fn revoke() {
        let dir = tempdir().unwrap();
        let cm = ConsentManager::with_path(dir.path().join("consent.json"));
        cm.record_consent().unwrap();
        assert!(cm.is_consented());
        cm.revoke_consent().unwrap();
        assert!(!cm.is_consented());
    }

    #[tokio::test]
    async fn fallback_on_no_consent() {
        let dir = tempdir().unwrap();
        let consent_path = dir.path().join("consent.json");

        let primary = Arc::new(ConsentCheckInferencer { consent_path });
        let fallback: Arc<dyn Inferencer> = Arc::new(AlwaysSucceedInferencer);
        let chain = FallbackChain::new(primary, vec![fallback], Duration::from_millis(1500));

        let candidates = vec![CandidateWithScore {
            surface: "天気".to_string(),
            score: 1.0,
            source: CandidateSource::Ngram,
        }];
        let result = chain.rerank("てんき", candidates, None).await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].surface, "天気");
    }
}
