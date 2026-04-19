use std::collections::HashMap;
use std::path::Path;

use super::{ReadingEntry, ReadingIndex};

pub struct MozcReadingIndex {
    map: HashMap<String, Vec<ReadingEntry>>,
}

impl MozcReadingIndex {
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Self> {
        let mut map: HashMap<String, Vec<ReadingEntry>> = HashMap::new();

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("txt") {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with("dictionary") {
                continue;
            }
            let content = std::fs::read_to_string(&path)?;
            Self::parse_tsv_into(&content, &mut map);
        }

        Ok(Self { map })
    }

    pub fn from_tsv_str(tsv: &str) -> anyhow::Result<Self> {
        let mut map: HashMap<String, Vec<ReadingEntry>> = HashMap::new();
        Self::parse_tsv_into(tsv, &mut map);
        Ok(Self { map })
    }

    fn parse_tsv_into(tsv: &str, map: &mut HashMap<String, Vec<ReadingEntry>>) {
        for line in tsv.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = line.splitn(6, '\t').collect();
            if cols.len() < 5 {
                continue;
            }
            let reading = cols[0].to_string();
            let lid: i32 = cols[1].parse().unwrap_or(0);
            let cost: i32 = cols[3].parse().unwrap_or(0);
            let surface = cols[4].to_string();
            let pos = lid_to_pos(lid);

            map.entry(reading.clone()).or_default().push(ReadingEntry {
                surface,
                reading,
                pos,
                cost,
            });
        }
    }
}

fn lid_to_pos(lid: i32) -> String {
    // Simplified POS mapping from lid ranges (mozc IPAdic-based)
    match lid {
        1285..=1572 => "名詞".to_string(),
        31..=68 => "動詞".to_string(),
        69..=115 => "形容詞".to_string(),
        _ => "名詞".to_string(),
    }
}

impl ReadingIndex for MozcReadingIndex {
    fn lookup(&self, reading: &str) -> Vec<ReadingEntry> {
        self.map.get(reading).cloned().unwrap_or_default()
    }

    fn prefix_search(&self, _reading: &str) -> Vec<(usize, ReadingEntry)> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMMY_TSV: &str = "\
あ\t1285\t1285\t3000\tア
あい\t1285\t1285\t4500\t愛
あい\t1285\t1285\t5000\t合い
あいこ\t1285\t1285\t6000\t愛子
うえ\t1285\t1285\t3500\t上
した\t1285\t1285\t3200\t下
ひと\t1285\t1285\t2800\t人
くに\t1285\t1285\t4000\t国
まち\t1285\t1285\t3800\t町
はな\t1285\t1285\t3100\t花
";

    #[test]
    fn lookup_exact_match_single() {
        let idx = MozcReadingIndex::from_tsv_str(DUMMY_TSV).unwrap();
        let results = idx.lookup("あ");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].surface, "ア");
        assert_eq!(results[0].cost, 3000);
    }

    #[test]
    fn lookup_exact_match_multiple() {
        let idx = MozcReadingIndex::from_tsv_str(DUMMY_TSV).unwrap();
        let results = idx.lookup("あい");
        assert_eq!(results.len(), 2);
        let surfaces: Vec<&str> = results.iter().map(|e| e.surface.as_str()).collect();
        assert!(surfaces.contains(&"愛"));
        assert!(surfaces.contains(&"合い"));
    }

    #[test]
    fn lookup_no_match_returns_empty() {
        let idx = MozcReadingIndex::from_tsv_str(DUMMY_TSV).unwrap();
        let results = idx.lookup("zzz");
        assert!(results.is_empty());
    }

    #[test]
    fn prefix_search_returns_empty_in_phase1() {
        let idx = MozcReadingIndex::from_tsv_str(DUMMY_TSV).unwrap();
        let results = idx.prefix_search("あい");
        assert!(results.is_empty());
    }

    #[test]
    fn from_tsv_str_boundary_empty_input() {
        let idx = MozcReadingIndex::from_tsv_str("").unwrap();
        let results = idx.lookup("あ");
        assert!(results.is_empty());
    }

    #[test]
    fn from_tsv_str_ignores_comment_and_blank_lines() {
        let tsv = "# comment\n\nあ\t1285\t1285\t3000\tア\n";
        let idx = MozcReadingIndex::from_tsv_str(tsv).unwrap();
        let results = idx.lookup("あ");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn from_tsv_str_skips_malformed_lines() {
        let tsv = "incomplete\nあ\t1285\t1285\t3000\tア\n";
        let idx = MozcReadingIndex::from_tsv_str(tsv).unwrap();
        let results = idx.lookup("あ");
        assert_eq!(results.len(), 1);
    }

    #[test]
    #[ignore]
    fn load_from_dir_integration() {
        let dir = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../vendor/mozc_oss"
        ));
        if !dir.exists() {
            return;
        }
        let idx = MozcReadingIndex::load_from_dir(dir).unwrap();
        let results = idx.lookup("とうきょう");
        assert!(!results.is_empty());
    }
}
