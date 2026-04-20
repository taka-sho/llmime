use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use crate::morphology::{Morpheme, Tokenizer};

pub struct StreamingBoundaryDetector {
    tokenizer: Arc<dyn Tokenizer>,
    prev_tokens: Vec<Morpheme>,
    fired_positions: HashSet<usize>,
}

pub struct BoundaryEvent {
    pub position: usize,
    pub new_token: Morpheme,
    pub timestamp: Instant,
}

impl StreamingBoundaryDetector {
    pub fn new(tokenizer: Arc<dyn Tokenizer>) -> Self {
        Self {
            tokenizer,
            prev_tokens: Vec::new(),
            fired_positions: HashSet::new(),
        }
    }

    fn sync_fired_positions(&mut self, input: &str) {
        let input_len = input.chars().count();
        self.fired_positions.retain(|&p| p < input_len);
    }

    /// Feed new input state; returns boundary events for newly confirmed tokens.
    ///
    /// A token is "confirmed" once it is no longer the last (currently-being-typed) token.
    /// Each confirmed start position fires at most once until reset.
    pub fn feed(&mut self, input: &str) -> Vec<BoundaryEvent> {
        self.sync_fired_positions(input);

        let new_tokens = match self.tokenizer.tokenize(input) {
            Ok(t) => t,
            Err(_) => return vec![],
        };

        let confirmed_count = if new_tokens.is_empty() {
            0
        } else {
            new_tokens.len() - 1
        };

        let mut events = Vec::new();
        let mut pos = 0usize;

        for (i, token) in new_tokens.iter().enumerate() {
            if i < confirmed_count && !self.fired_positions.contains(&pos) {
                self.fired_positions.insert(pos);
                events.push(BoundaryEvent {
                    position: pos,
                    new_token: token.clone(),
                    timestamp: Instant::now(),
                });
            }
            pos += token.surface.chars().count();
        }

        self.prev_tokens = new_tokens;
        events
    }

    pub fn reset(&mut self) {
        self.prev_tokens.clear();
        self.fired_positions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTokenizer {
        responses: std::collections::HashMap<String, Vec<Morpheme>>,
    }

    impl MockTokenizer {
        fn new(pairs: Vec<(&str, Vec<Morpheme>)>) -> Self {
            Self {
                responses: pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            }
        }
    }

    fn m(surface: &str) -> Morpheme {
        Morpheme {
            surface: surface.to_string(),
            reading: surface.to_string(),
            pos: "noun".to_string(),
            pos_detail: "general".to_string(),
        }
    }

    impl Tokenizer for MockTokenizer {
        fn tokenize(&self, text: &str) -> anyhow::Result<Vec<Morpheme>> {
            Ok(self
                .responses
                .get(text)
                .cloned()
                .unwrap_or_else(|| vec![m(text)]))
        }
    }

    /// 1文字ずつ進めた時の境界発火タイミング検証
    #[test]
    fn boundary_fires_when_new_token_confirmed() {
        let tokenizer: Arc<dyn Tokenizer> = Arc::new(MockTokenizer::new(vec![
            ("\u{6771}", vec![m("\u{6771}")]),
            ("\u{6771}\u{4eac}", vec![m("\u{6771}\u{4eac}")]),
            (
                "\u{6771}\u{4eac}\u{90fd}",
                vec![m("\u{6771}\u{4eac}"), m("\u{90fd}")],
            ),
            (
                "\u{6771}\u{4eac}\u{90fd}\u{306b}",
                vec![m("\u{6771}\u{4eac}"), m("\u{90fd}"), m("\u{306b}")],
            ),
        ]));
        let mut det = StreamingBoundaryDetector::new(tokenizer);

        assert!(det.feed("\u{6771}").is_empty(), "single token, no boundary");
        assert!(
            det.feed("\u{6771}\u{4eac}").is_empty(),
            "still single token"
        );

        let e3 = det.feed("\u{6771}\u{4eac}\u{90fd}");
        assert_eq!(e3.len(), 1);
        assert_eq!(e3[0].position, 0);
        assert_eq!(e3[0].new_token.surface, "\u{6771}\u{4eac}");

        let e4 = det.feed("\u{6771}\u{4eac}\u{90fd}\u{306b}");
        assert_eq!(e4.len(), 1);
        assert_eq!(e4[0].position, 2);
        assert_eq!(e4[0].new_token.surface, "\u{90fd}");
    }

    /// ASCII入力でイベントなし
    #[test]
    fn no_boundary_for_ascii_input() {
        let tokenizer: Arc<dyn Tokenizer> = Arc::new(MockTokenizer::new(vec![]));
        let mut det = StreamingBoundaryDetector::new(tokenizer);

        for partial in &["h", "he", "hel", "hell", "hello"] {
            assert!(
                det.feed(partial).is_empty(),
                "no events for ascii: {:?}",
                partial
            );
        }
    }

    /// 複数境界を1feedで検出
    #[test]
    fn multiple_boundaries_in_single_feed() {
        let tokenizer: Arc<dyn Tokenizer> = Arc::new(MockTokenizer::new(vec![(
            "\u{6771}\u{4eac}\u{90fd}\u{306b}",
            vec![m("\u{6771}\u{4eac}"), m("\u{90fd}"), m("\u{306b}")],
        )]));
        let mut det = StreamingBoundaryDetector::new(tokenizer);

        let events = det.feed("\u{6771}\u{4eac}\u{90fd}\u{306b}");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].position, 0);
        assert_eq!(events[0].new_token.surface, "\u{6771}\u{4eac}");
        assert_eq!(events[1].position, 2);
        assert_eq!(events[1].new_token.surface, "\u{90fd}");
    }
}
