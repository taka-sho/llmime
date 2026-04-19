#[derive(Debug, Clone, PartialEq)]
pub struct Morpheme {
    pub surface: String,
    pub reading: String,
    pub pos: String,
    pub pos_detail: String,
}

pub trait Tokenizer: Send + Sync {
    fn tokenize(&self, text: &str) -> anyhow::Result<Vec<Morpheme>>;

    fn reading_of(&self, text: &str) -> anyhow::Result<String> {
        let morphemes = self.tokenize(text)?;
        Ok(morphemes.iter().map(|m| m.reading.as_str()).collect())
    }
}

pub struct VibratoTokenizer {
    inner: vibrato::Tokenizer,
}

impl VibratoTokenizer {
    pub fn new(dict_path: &std::path::Path) -> anyhow::Result<Self> {
        use std::io::BufReader;
        let reader = BufReader::new(std::fs::File::open(dict_path)?);
        let dict = vibrato::Dictionary::read(reader)?;
        Ok(Self {
            inner: vibrato::Tokenizer::new(dict),
        })
    }
}

impl Tokenizer for VibratoTokenizer {
    fn tokenize(&self, text: &str) -> anyhow::Result<Vec<Morpheme>> {
        let mut worker = self.inner.new_worker();
        worker.reset_sentence(text);
        worker.tokenize();

        let mut result = Vec::new();
        for i in 0..worker.num_tokens() {
            let token = worker.token(i);
            let surface = token.surface().to_string();
            let feature = token.feature().to_string();
            result.push(parse_feature(&surface, &feature));
        }
        Ok(result)
    }
}

fn parse_feature(surface: &str, feature: &str) -> Morpheme {
    let parts: Vec<&str> = feature.split(',').collect();
    let pos = parts.first().copied().unwrap_or("*").to_string();
    let pos_detail = parts.get(1).copied().unwrap_or("*").to_string();

    // UniDic-lite: 読み形出現形 at col 9 (katakana)
    // IPAdic: 読み at col 7 (katakana)
    let reading_raw = parts
        .get(9)
        .or_else(|| parts.get(7))
        .copied()
        .unwrap_or("*");

    let reading = if reading_raw == "*" || reading_raw.is_empty() {
        surface.to_string()
    } else {
        katakana_to_hiragana(reading_raw)
    };

    Morpheme {
        surface: surface.to_string(),
        reading,
        pos,
        pos_detail,
    }
}

fn katakana_to_hiragana(s: &str) -> String {
    s.chars()
        .map(|c| {
            if ('\u{30A1}'..='\u{30F6}').contains(&c) {
                char::from_u32(c as u32 - 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTokenizer {
        tokens: Vec<Morpheme>,
    }

    impl Tokenizer for MockTokenizer {
        fn tokenize(&self, _text: &str) -> anyhow::Result<Vec<Morpheme>> {
            Ok(self.tokens.clone())
        }
    }

    #[test]
    fn mock_tokenizer_tokenize() {
        let mock = MockTokenizer {
            tokens: vec![Morpheme {
                surface: "東京".to_string(),
                reading: "とうきょう".to_string(),
                pos: "名詞".to_string(),
                pos_detail: "固有名詞".to_string(),
            }],
        };
        let result = mock.tokenize("東京").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].reading, "とうきょう");
    }

    #[test]
    fn mock_tokenizer_reading_of() {
        let mock = MockTokenizer {
            tokens: vec![
                Morpheme {
                    surface: "東京".to_string(),
                    reading: "とうきょう".to_string(),
                    pos: "名詞".to_string(),
                    pos_detail: "固有名詞".to_string(),
                },
                Morpheme {
                    surface: "都".to_string(),
                    reading: "と".to_string(),
                    pos: "名詞".to_string(),
                    pos_detail: "一般".to_string(),
                },
            ],
        };
        let reading = mock.reading_of("東京都").unwrap();
        assert_eq!(reading, "とうきょうと");
    }

    #[test]
    fn katakana_to_hiragana_conversion() {
        assert_eq!(katakana_to_hiragana("トウキョウ"), "とうきょう");
        assert_eq!(katakana_to_hiragana("テスト"), "てすと");
        assert_eq!(katakana_to_hiragana("ABC"), "ABC");
    }

    #[test]
    fn parse_feature_unidic_lite() {
        // UniDic-lite feature with reading at col 9
        let feature = "名詞,固有名詞,地名,一般,*,*,トウキョウ,東京,東京,トウキョウ,東京,トウキョウ";
        let m = parse_feature("東京", feature);
        assert_eq!(m.pos, "名詞");
        assert_eq!(m.pos_detail, "固有名詞");
        assert_eq!(m.reading, "とうきょう");
    }

    #[test]
    fn parse_feature_unknown_reading() {
        let feature = "名詞,固有名詞,*,*,*,*,*,*,*,*";
        let m = parse_feature("未知語", feature);
        assert_eq!(m.reading, "未知語");
    }

    #[test]
    #[ignore]
    fn vibrato_tokenizer_integration() {
        let dict_path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../dict/system.dic"
        ));
        if !dict_path.exists() {
            return;
        }
        let tokenizer = VibratoTokenizer::new(dict_path).unwrap();
        let tokens = tokenizer.tokenize("東京都").unwrap();
        assert!(!tokens.is_empty());
        for t in &tokens {
            assert!(!t.surface.is_empty());
            assert!(!t.reading.is_empty());
        }
    }
}
