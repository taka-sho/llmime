//! User dictionary backed by SQLite (§10 user-dict).
//!
//! Allows users to register custom words that are merged into candidate
//! lookup results alongside the built-in Mozc dictionary.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::reading_index::{ReadingEntry, ReadingIndex};

const SCHEMA: &str = "
    PRAGMA journal_mode=WAL;

    CREATE TABLE IF NOT EXISTS user_dict (
        id       INTEGER PRIMARY KEY AUTOINCREMENT,
        word     TEXT NOT NULL,
        reading  TEXT NOT NULL,
        pos      TEXT NOT NULL DEFAULT '',
        UNIQUE(word, reading)
    );

    CREATE INDEX IF NOT EXISTS idx_user_dict_reading ON user_dict(reading);
";

pub struct UserDict {
    conn: Mutex<Connection>,
}

impl UserDict {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        let d = Self {
            conn: Mutex::new(conn),
        };
        d.apply_schema()?;
        Ok(d)
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let d = Self {
            conn: Mutex::new(conn),
        };
        d.apply_schema()?;
        Ok(d)
    }

    fn apply_schema(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(SCHEMA)?;
        Ok(())
    }

    /// Add a custom word. Silently succeeds if the (word, reading) pair already exists.
    pub fn add_entry(&self, word: &str, reading: &str, pos: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO user_dict (word, reading, pos) VALUES (?1, ?2, ?3)",
            params![word, reading, pos],
        )?;
        Ok(())
    }

    /// Return all user-dict entries whose reading matches exactly.
    pub fn lookup(&self, reading: &str) -> Vec<ReadingEntry> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            match conn.prepare("SELECT word, reading, pos FROM user_dict WHERE reading = ?1") {
                Ok(s) => s,
                Err(_) => return vec![],
            };
        stmt.query_map(params![reading], |row| {
            Ok(ReadingEntry {
                surface: row.get(0)?,
                reading: row.get(1)?,
                pos: row.get(2)?,
                cost: 0,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Remove a custom entry. No-op if not found.
    pub fn remove_entry(&self, word: &str, reading: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM user_dict WHERE word = ?1 AND reading = ?2",
            params![word, reading],
        )?;
        Ok(())
    }
}

/// ReadingIndex stub that serves ViterbiLattice with user-dict entries.
///
/// Wrap alongside MozcReadingIndex via MergedReadingIndex for full coverage.
impl ReadingIndex for UserDict {
    fn lookup(&self, reading: &str) -> Vec<ReadingEntry> {
        UserDict::lookup(self, reading)
    }

    fn prefix_search(&self, reading: &str) -> Vec<(usize, ReadingEntry)> {
        // Iterate over all char-boundary prefixes and return exact matches.
        let mut results = Vec::new();
        let bytes = reading.as_bytes();
        let mut len = 0usize;
        for ch in reading.chars() {
            len += ch.len_utf8();
            let prefix = &reading[..len];
            for entry in UserDict::lookup(self, prefix) {
                results.push((bytes[..len].len(), entry));
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> UserDict {
        UserDict::open_in_memory().expect("in-memory DB")
    }

    // 1. add and lookup basic entry
    #[test]
    fn add_and_lookup() {
        let d = dict();
        d.add_entry("llmime", "えるえるまいむ", "名詞").unwrap();
        let results = d.lookup("えるえるまいむ");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].surface, "llmime");
        assert_eq!(results[0].pos, "名詞");
    }

    // 2. lookup unknown reading returns empty
    #[test]
    fn lookup_unknown_returns_empty() {
        let d = dict();
        assert!(d.lookup("みつからない").is_empty());
    }

    // 3. remove_entry deletes the row
    #[test]
    fn remove_entry_deletes() {
        let d = dict();
        d.add_entry("テスト", "てすと", "名詞").unwrap();
        d.remove_entry("テスト", "てすと").unwrap();
        assert!(d.lookup("てすと").is_empty());
    }

    // 4. duplicate add is idempotent (no error, no duplicate row)
    #[test]
    fn duplicate_add_idempotent() {
        let d = dict();
        d.add_entry("東京", "とうきょう", "固有名詞").unwrap();
        d.add_entry("東京", "とうきょう", "固有名詞").unwrap();
        assert_eq!(d.lookup("とうきょう").len(), 1);
    }

    // 5. multiple entries for same reading all returned
    #[test]
    fn multiple_entries_same_reading() {
        let d = dict();
        d.add_entry("変換", "へんかん", "名詞").unwrap();
        d.add_entry("返還", "へんかん", "名詞").unwrap();
        let results = d.lookup("へんかん");
        assert_eq!(results.len(), 2);
    }

    // 6. remove is specific to (word, reading) — other entries survive
    #[test]
    fn remove_leaves_other_entries() {
        let d = dict();
        d.add_entry("変換", "へんかん", "名詞").unwrap();
        d.add_entry("返還", "へんかん", "名詞").unwrap();
        d.remove_entry("変換", "へんかん").unwrap();
        let results = d.lookup("へんかん");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].surface, "返還");
    }

    // 7. lookup returns ReadingEntry with cost=0 for Viterbi integration
    #[test]
    fn lookup_entry_cost_is_zero() {
        let d = dict();
        d.add_entry("llmime", "えるえるまいむ", "名詞").unwrap();
        let results = d.lookup("えるえるまいむ");
        assert_eq!(results[0].cost, 0);
    }
}
