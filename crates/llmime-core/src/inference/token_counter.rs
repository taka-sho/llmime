pub struct TokenCounter;

impl TokenCounter {
    /// Estimates morpheme count from character count (approximation for v1).
    pub fn count(text: &str) -> usize {
        (text.chars().count() + 2) / 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string() {
        assert_eq!(TokenCounter::count(""), 0);
    }

    #[test]
    fn short_text_below_threshold() {
        // "今日は" = 3 chars → (3+2)/3 = 1, well below 15
        let count = TokenCounter::count("今日は");
        assert!(count < 15, "expected < 15, got {count}");
        assert_eq!(count, 1);
    }

    #[test]
    fn long_text_at_or_above_threshold() {
        // 45 chars → (45+2)/3 = 15
        let text = "あ".repeat(45);
        assert_eq!(TokenCounter::count(&text), 15);

        // 46 chars → 16
        let text2 = "あ".repeat(46);
        assert_eq!(TokenCounter::count(&text2), 16);
    }

    #[test]
    fn mixed_ascii_and_fullwidth() {
        // "hello世界" = 7 chars → (7+2)/3 = 3
        assert_eq!(TokenCounter::count("hello世界"), 3);
    }

    #[test]
    fn single_char() {
        // 1 char → (1+2)/3 = 1
        assert_eq!(TokenCounter::count("あ"), 1);
    }

    #[test]
    fn boundary_at_threshold() {
        // For default threshold=15, need count >= 15, i.e. chars >= 43
        // 43 chars → (43+2)/3 = 15
        let text = "あ".repeat(43);
        assert_eq!(TokenCounter::count(&text), 15);
    }
}
