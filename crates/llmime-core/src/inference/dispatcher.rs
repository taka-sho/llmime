use std::sync::Arc;

use crate::inference::{
    inferencer::DynInferencer, local_llm::LocalLlmInferencer, local_ngram::LocalNgramInferencer,
    mode::InputMode, workers_ai::WorkersAIInferencer,
};

const DEFAULT_THRESHOLD: usize = 15;

pub struct Dispatcher {
    ngram: Arc<LocalNgramInferencer>,
    workers_ai: Option<Arc<WorkersAIInferencer>>,
    local_llm: Option<Arc<LocalLlmInferencer>>,
    threshold_t: usize,
}

impl Dispatcher {
    pub fn new(
        ngram: Arc<LocalNgramInferencer>,
        workers_ai: Option<Arc<WorkersAIInferencer>>,
        local_llm: Option<Arc<LocalLlmInferencer>>,
    ) -> Self {
        Self {
            ngram,
            workers_ai,
            local_llm,
            threshold_t: DEFAULT_THRESHOLD,
        }
    }

    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.threshold_t = threshold;
        self
    }

    pub fn select_inferencer(&self, mode: InputMode, token_count: usize) -> DynInferencer {
        if token_count < self.threshold_t {
            return Arc::clone(&self.ngram) as DynInferencer;
        }
        match mode {
            InputMode::Privacy => self
                .local_llm
                .as_ref()
                .map(|l| Arc::clone(l) as DynInferencer)
                .unwrap_or_else(|| Arc::clone(&self.ngram) as DynInferencer),
            InputMode::Performance => self
                .workers_ai
                .as_ref()
                .map(|w| Arc::clone(w) as DynInferencer)
                .unwrap_or_else(|| Arc::clone(&self.ngram) as DynInferencer),
            InputMode::Pro => self
                .local_llm
                .as_ref()
                .map(|l| Arc::clone(l) as DynInferencer)
                .unwrap_or_else(|| Arc::clone(&self.ngram) as DynInferencer),
            // Hybrid must be resolved via ModeManager::effective_mode() before calling this.
            // Unresolved Hybrid falls back to ngram (Privacy-safe, NF-032).
            InputMode::Hybrid => Arc::clone(&self.ngram) as DynInferencer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ngram() -> Arc<LocalNgramInferencer> {
        Arc::new(LocalNgramInferencer::new_in_memory())
    }

    struct Case {
        mode: InputMode,
        token_count: usize,
        has_workers_ai: bool,
        has_local_llm: bool,
        expected_name: &'static str,
    }

    fn run_case(c: &Case) -> &'static str {
        let workers_ai = if c.has_workers_ai {
            Some(Arc::new(WorkersAIInferencer::new(
                "acct".to_string(),
                "token".to_string(),
                "model".to_string(),
            )))
        } else {
            None
        };
        let local_llm = if c.has_local_llm {
            Some(Arc::new(LocalLlmInferencer::new(None)))
        } else {
            None
        };
        let dispatcher = Dispatcher::new(ngram(), workers_ai, local_llm).with_threshold(15);
        dispatcher.select_inferencer(c.mode, c.token_count).name()
    }

    #[test]
    fn table_driven() {
        let cases = vec![
            // token_count < threshold → always ngram
            Case {
                mode: InputMode::Privacy,
                token_count: 0,
                has_workers_ai: true,
                has_local_llm: true,
                expected_name: "local-ngram",
            },
            Case {
                mode: InputMode::Performance,
                token_count: 14,
                has_workers_ai: true,
                has_local_llm: true,
                expected_name: "local-ngram",
            },
            Case {
                mode: InputMode::Pro,
                token_count: 14,
                has_workers_ai: true,
                has_local_llm: true,
                expected_name: "local-ngram",
            },
            // token_count >= threshold, Privacy with local_llm
            Case {
                mode: InputMode::Privacy,
                token_count: 15,
                has_workers_ai: false,
                has_local_llm: true,
                expected_name: "local-llm",
            },
            Case {
                mode: InputMode::Privacy,
                token_count: 100,
                has_workers_ai: true,
                has_local_llm: true,
                expected_name: "local-llm",
            },
            // Privacy without local_llm → fallback ngram
            Case {
                mode: InputMode::Privacy,
                token_count: 15,
                has_workers_ai: true,
                has_local_llm: false,
                expected_name: "local-ngram",
            },
            // Performance with workers_ai
            Case {
                mode: InputMode::Performance,
                token_count: 15,
                has_workers_ai: true,
                has_local_llm: false,
                expected_name: "workers-ai",
            },
            Case {
                mode: InputMode::Performance,
                token_count: 50,
                has_workers_ai: true,
                has_local_llm: true,
                expected_name: "workers-ai",
            },
            // Performance without workers_ai → fallback ngram
            Case {
                mode: InputMode::Performance,
                token_count: 15,
                has_workers_ai: false,
                has_local_llm: true,
                expected_name: "local-ngram",
            },
            // Pro with local_llm
            Case {
                mode: InputMode::Pro,
                token_count: 15,
                has_workers_ai: false,
                has_local_llm: true,
                expected_name: "local-llm",
            },
            Case {
                mode: InputMode::Pro,
                token_count: 50,
                has_workers_ai: true,
                has_local_llm: true,
                expected_name: "local-llm",
            },
            // Pro without local_llm → fallback ngram
            Case {
                mode: InputMode::Pro,
                token_count: 15,
                has_workers_ai: true,
                has_local_llm: false,
                expected_name: "local-ngram",
            },
            // threshold boundary: exactly threshold → not below, uses mode logic
            Case {
                mode: InputMode::Performance,
                token_count: 15,
                has_workers_ai: true,
                has_local_llm: false,
                expected_name: "workers-ai",
            },
            // threshold=0 edge: nothing below 0
            Case {
                mode: InputMode::Privacy,
                token_count: 0,
                has_workers_ai: false,
                has_local_llm: false,
                expected_name: "local-ngram",
            },
        ];

        for (i, c) in cases.iter().enumerate() {
            let got = run_case(c);
            assert_eq!(
                got, c.expected_name,
                "case {i}: mode={:?} tokens={} workers_ai={} local_llm={} → expected {} got {}",
                c.mode, c.token_count, c.has_workers_ai, c.has_local_llm, c.expected_name, got
            );
        }
    }

    #[test]
    fn custom_threshold() {
        let dispatcher = Dispatcher::new(ngram(), None, None).with_threshold(5);
        assert_eq!(
            dispatcher.select_inferencer(InputMode::Privacy, 4).name(),
            "local-ngram"
        );
        assert_eq!(
            dispatcher.select_inferencer(InputMode::Privacy, 5).name(),
            "local-ngram"
        );
        assert_eq!(
            dispatcher.select_inferencer(InputMode::Privacy, 6).name(),
            "local-ngram"
        );
    }
}
