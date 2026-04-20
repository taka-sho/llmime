use std::sync::Arc;
use std::time::Instant;

use crate::morphology::{Tokenizer, VibratoTokenizer};

#[derive(Debug, Clone)]
pub struct Token {
    pub surface: String,
    pub reading: String,
    pub pos: String,
    pub pos_detail: String,
    pub start: usize,
    pub end: usize,
    pub confidence: f32,
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.surface == other.surface
            && self.reading == other.reading
            && self.pos == other.pos
            && self.pos_detail == other.pos_detail
            && self.start == other.start
            && self.end == other.end
    }
}

impl Eq for Token {}

pub struct StreamingBoundaryDetector<T: Tokenizer = VibratoTokenizer> {
    tokenizer: Arc<T>,
    prev_tokens: Vec<Token>,
}

#[derive(Debug, Clone)]
pub struct BoundaryEvent {
    pub position: usize,
    pub new_token: Token,
    pub timestamp: Instant,
    pub recent_tokens: Vec<Token>,
}

impl StreamingBoundaryDetector<VibratoTokenizer> {
    pub fn new(tokenizer: Arc<VibratoTokenizer>) -> Self {
        Self::with_tokenizer(tokenizer)
    }
}

impl<T: Tokenizer> StreamingBoundaryDetector<T> {
    pub fn with_tokenizer(tokenizer: Arc<T>) -> Self {
        Self {
            tokenizer,
            prev_tokens: Vec::new(),
        }
    }

    fn tokenize_with_positions(&self, input: &str) -> anyhow::Result<Vec<Token>> {
        let mut pos = 0usize;
        let mut out = Vec::new();
        for m in self.tokenizer.tokenize(input)? {
            let len = m.surface.chars().count();
            out.push(Token {
                surface: m.surface,
                reading: m.reading,
                pos: m.pos,
                pos_detail: m.pos_detail,
                start: pos,
                end: pos + len,
                confidence: 1.0,
            });
            pos += len;
        }
        Ok(out)
    }

    fn common_prefix_len(a: &[Token], b: &[Token]) -> usize {
        a.iter()
            .zip(b.iter())
            .take_while(|(left, right)| left == right)
            .count()
    }

    pub fn feed(&mut self, input: &str) -> Vec<BoundaryEvent> {
        let current_tokens = match self.tokenize_with_positions(input) {
            Ok(tokens) => tokens,
            Err(_) => return Vec::new(),
        };

        // Treat the tail token as in-flight: it can still be merged/split by subsequent input.
        let prev_confirmed = &self.prev_tokens[..self.prev_tokens.len().saturating_sub(1)];
        let current_confirmed = &current_tokens[..current_tokens.len().saturating_sub(1)];
        let prefix_len = Self::common_prefix_len(prev_confirmed, current_confirmed);

        let recent_tokens = current_tokens.clone();

        let events = current_confirmed[prefix_len..]
            .iter()
            .cloned()
            .map(|token| BoundaryEvent {
                position: token.end,
                new_token: token,
                timestamp: Instant::now(),
                recent_tokens: recent_tokens.clone(),
            })
            .collect();

        self.prev_tokens = current_tokens;
        events
    }

    pub fn reset(&mut self) {
        self.prev_tokens.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::morphology::Morpheme;
    use std::collections::HashMap;

    struct MockTokenizer {
        responses: HashMap<String, Vec<Morpheme>>,
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
        let tokenizer = Arc::new(MockTokenizer::new(vec![
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
        let mut det = StreamingBoundaryDetector::with_tokenizer(tokenizer);

        assert!(det.feed("\u{6771}").is_empty(), "single token, no boundary");
        assert!(
            det.feed("\u{6771}\u{4eac}").is_empty(),
            "still single token"
        );

        let e3 = det.feed("\u{6771}\u{4eac}\u{90fd}");
        assert_eq!(e3.len(), 1);
        assert_eq!(e3[0].position, 2);
        assert_eq!(e3[0].new_token.surface, "\u{6771}\u{4eac}");

        let e4 = det.feed("\u{6771}\u{4eac}\u{90fd}\u{306b}");
        assert_eq!(e4.len(), 1);
        assert_eq!(e4[0].position, 3);
        assert_eq!(e4[0].new_token.surface, "\u{90fd}");
    }

    /// ASCII入力でイベントなし
    #[test]
    fn no_boundary_for_ascii_input() {
        let tokenizer = Arc::new(MockTokenizer::new(vec![]));
        let mut det = StreamingBoundaryDetector::with_tokenizer(tokenizer);

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
        let tokenizer = Arc::new(MockTokenizer::new(vec![(
            "\u{6771}\u{4eac}\u{90fd}\u{306b}",
            vec![m("\u{6771}\u{4eac}"), m("\u{90fd}"), m("\u{306b}")],
        )]));
        let mut det = StreamingBoundaryDetector::with_tokenizer(tokenizer);

        let events = det.feed("\u{6771}\u{4eac}\u{90fd}\u{306b}");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].position, 2);
        assert_eq!(events[0].new_token.surface, "\u{6771}\u{4eac}");
        assert_eq!(events[1].position, 3);
        assert_eq!(events[1].new_token.surface, "\u{90fd}");
    }
}
