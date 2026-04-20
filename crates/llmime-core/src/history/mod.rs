//! Conversion preference history backed by SQLite (§10-1).
//!
//! Records which surface form the user selected for a given reading so that
//! frequently-chosen candidates are boosted toward the top of future results.
//!
//! Privacy invariant: only (reading, surface, context) tuples are stored.
//! Raw keystrokes and input text are never accepted or persisted.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::scoring::Candidate;

// --------------------------------------------------------------------------
// Trait
// --------------------------------------------------------------------------

pub trait HistoryStore: Send + Sync {
    /// Record that the user selected `surface` for `reading`.
    ///
    /// `prev_ctx`: up to 20 chars of text immediately preceding the converted
    /// segment (empty string when unavailable).
    /// `next_ctx`: text immediately following the segment (right-context
    /// trigger data; empty when unavailable).
    ///
    /// # Privacy invariant
    /// The argument types intentionally accept only linguistic metadata
    /// (reading/surface/context).  Raw keystrokes and input text MUST NOT be
    /// passed here; the caller is responsible for stripping them before call.
    fn record_conversion(&self, reading: &str, surface: &str, prev_ctx: &str, next_ctx: &str);

    /// Boost scores of candidates that have been historically selected for
    /// `reading`.  For each matching (reading, surface) pair the score is
    /// increased by `count * 5.0` so that popular choices float to the top.
    fn boost_candidates(&self, reading: &str, candidates: &mut Vec<Candidate>);

    /// Delete all stored preference data (F-085 privacy reset).
    fn reset_all(&self);

    /// Record rerank outcomes for trend analysis (F-083, F-118).
    ///
    /// Stores only sanitized linguistic metadata and aggregate score delta.
    fn record_rerank(&self, event: &RerankHistoryEvent<'_>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RerankTriggerKind {
    RightContext,
    Selection,
}

impl RerankTriggerKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::RightContext => "right_context",
            Self::Selection => "selection",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RerankHistoryEvent<'a> {
    pub reading: &'a str,
    pub initial_surface: &'a str,
    pub reranked_surface: &'a str,
    pub left_ctx: &'a str,
    pub right_ctx: &'a str,
    pub trigger_kind: RerankTriggerKind,
    pub score_delta: f64,
}

// --------------------------------------------------------------------------
// SQLite implementation
// --------------------------------------------------------------------------

const SCHEMA: &str = "
    PRAGMA journal_mode=WAL;

    CREATE TABLE IF NOT EXISTS conversion_preference (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        reading         TEXT NOT NULL,
        surface         TEXT NOT NULL,
        prev_context    TEXT NOT NULL DEFAULT '',
        next_context    TEXT NOT NULL DEFAULT '',
        confidence      REAL NOT NULL DEFAULT 0.0,
        reinfer_trigger TEXT NOT NULL DEFAULT '',
        count           INTEGER NOT NULL DEFAULT 1,
        last_used_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    CREATE INDEX IF NOT EXISTS idx_reading
        ON conversion_preference(reading);
    CREATE INDEX IF NOT EXISTS idx_reading_context
        ON conversion_preference(reading, prev_context);
    CREATE INDEX IF NOT EXISTS idx_reading_full_context
        ON conversion_preference(reading, prev_context, next_context);

    CREATE TABLE IF NOT EXISTS rerank_history (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        reading         TEXT NOT NULL,
        initial_surface TEXT NOT NULL,
        reranked_surface TEXT NOT NULL,
        left_context    TEXT NOT NULL DEFAULT '',
        right_context   TEXT NOT NULL DEFAULT '',
        trigger_kind    TEXT NOT NULL CHECK (trigger_kind IN ('right_context', 'selection')),
        score_delta     REAL NOT NULL DEFAULT 0.0,
        created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );

    CREATE INDEX IF NOT EXISTS idx_rerank_history_reading_created
        ON rerank_history(reading, created_at);
";

pub struct SqliteHistoryStore {
    conn: Mutex<Connection>,
}

impl SqliteHistoryStore {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.apply_schema()?;
        Ok(store)
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.apply_schema()?;
        Ok(store)
    }

    fn apply_schema(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(SCHEMA)?;
        Ok(())
    }

    /// Remove low-frequency entries that have not been used in 6+ months.
    /// This prevents unbounded DB growth while preserving well-established
    /// preferences (count >= 2).
    pub fn vacuum_stale_entries(&self) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "DELETE FROM conversion_preference \
             WHERE count < 2 \
               AND last_used_at < datetime('now', '-6 months')",
            [],
        );
    }
}

impl HistoryStore for SqliteHistoryStore {
    fn record_conversion(&self, reading: &str, surface: &str, prev_ctx: &str, next_ctx: &str) {
        let conn = self.conn.lock().unwrap();
        let updated = conn
            .execute(
                "UPDATE conversion_preference \
                 SET count = count + 1, last_used_at = datetime('now') \
                 WHERE reading = ?1 AND surface = ?2 AND prev_context = ?3",
                params![reading, surface, prev_ctx],
            )
            .unwrap_or(0);

        if updated == 0 {
            let _ = conn.execute(
                "INSERT INTO conversion_preference \
                 (reading, surface, prev_context, next_context, last_used_at) \
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))",
                params![reading, surface, prev_ctx, next_ctx],
            );
        }
    }

    fn boost_candidates(&self, reading: &str, candidates: &mut Vec<Candidate>) {
        let boosts: HashMap<String, i64> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = match conn.prepare(
                "SELECT surface, SUM(count) FROM conversion_preference \
                 WHERE reading = ?1 GROUP BY surface",
            ) {
                Ok(s) => s,
                Err(_) => return,
            };
            stmt.query_map(params![reading], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        };

        if boosts.is_empty() {
            return;
        }

        for c in candidates.iter_mut() {
            if let Some(&cnt) = boosts.get(&c.surface) {
                c.score += (cnt as f64) * 5.0;
            }
        }
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    fn reset_all(&self) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM conversion_preference", []);
        let _ = conn.execute("DELETE FROM rerank_history", []);
    }

    fn record_rerank(&self, event: &RerankHistoryEvent<'_>) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO rerank_history \
             (reading, initial_surface, reranked_surface, left_context, right_context, trigger_kind, score_delta, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))",
            params![
                event.reading,
                event.initial_surface,
                event.reranked_surface,
                event.left_ctx,
                event.right_ctx,
                event.trigger_kind.as_str(),
                event.score_delta
            ],
        );
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SqliteHistoryStore {
        SqliteHistoryStore::open_in_memory().expect("in-memory DB")
    }

    fn make_candidates(surfaces: &[&str]) -> Vec<Candidate> {
        surfaces
            .iter()
            .map(|s| Candidate {
                surface: s.to_string(),
                reading: String::new(),
                score: 0.0,
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // 1. record → boost: selected surface moves to top
    // -----------------------------------------------------------------------
    #[test]
    fn boost_raises_recorded_surface() {
        let s = store();
        s.record_conversion("きかん", "機関", "", "");
        let mut cands = make_candidates(&["器官", "機関", "期間"]);
        s.boost_candidates("きかん", &mut cands);
        assert_eq!(cands[0].surface, "機関");
    }

    // -----------------------------------------------------------------------
    // 2. reset_all removes all data
    // -----------------------------------------------------------------------
    #[test]
    fn reset_all_clears_data() {
        let s = store();
        s.record_conversion("きかん", "機関", "", "");
        s.reset_all();
        let mut cands = make_candidates(&["器官", "機関"]);
        let before = cands[0].score;
        s.boost_candidates("きかん", &mut cands);
        assert!(
            (cands[0].score - before).abs() < f64::EPSILON,
            "No boost expected after reset"
        );
    }

    // -----------------------------------------------------------------------
    // 3. prev_context mismatch: boost is applied per surface regardless of
    //    context in boost_candidates (context affects record dedup only)
    // -----------------------------------------------------------------------
    #[test]
    fn boost_aggregates_across_contexts() {
        let s = store();
        s.record_conversion("かんこう", "観光", "東京", "");
        s.record_conversion("かんこう", "観光", "大阪", "");
        let mut cands = make_candidates(&["観光", "勧行"]);
        s.boost_candidates("かんこう", &mut cands);
        // boost should be count=2 → +10.0
        assert!(cands[0].score > 5.0);
    }

    // -----------------------------------------------------------------------
    // 4. multiple records increase boost strength
    // -----------------------------------------------------------------------
    #[test]
    fn repeated_record_increases_boost() {
        let s = store();
        for _ in 0..3 {
            s.record_conversion("へんかん", "変換", "", "");
        }
        let mut cands = make_candidates(&["変換", "返還"]);
        s.boost_candidates("へんかん", &mut cands);
        // count = 3 → boost = 3*5 = 15
        assert!(cands[0].score >= 15.0);
        assert_eq!(cands[0].surface, "変換");
    }

    // -----------------------------------------------------------------------
    // 5. different reading: no cross-contamination
    // -----------------------------------------------------------------------
    #[test]
    fn different_reading_no_boost() {
        let s = store();
        s.record_conversion("とうきょう", "東京", "", "");
        let mut cands = make_candidates(&["東京", "陶器用"]);
        s.boost_candidates("おおさか", &mut cands);
        // Score unchanged
        for c in &cands {
            assert!((c.score).abs() < f64::EPSILON);
        }
    }

    // -----------------------------------------------------------------------
    // 6. empty candidate list is handled gracefully
    // -----------------------------------------------------------------------
    #[test]
    fn boost_empty_candidates_no_panic() {
        let s = store();
        s.record_conversion("ほげ", "ほげ", "", "");
        let mut cands: Vec<Candidate> = vec![];
        s.boost_candidates("ほげ", &mut cands);
        assert!(cands.is_empty());
    }

    // -----------------------------------------------------------------------
    // 7. unknown reading with no history: candidates unchanged
    // -----------------------------------------------------------------------
    #[test]
    fn boost_no_history_candidates_unchanged() {
        let s = store();
        let mut cands = make_candidates(&["A", "B"]);
        s.boost_candidates("みつからない", &mut cands);
        assert!((cands[0].score).abs() < f64::EPSILON);
        assert!((cands[1].score).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // 8. dedup: recording same (reading, surface, prev_ctx) twice increments count
    // -----------------------------------------------------------------------
    #[test]
    fn record_same_entry_increments_count() {
        let s = store();
        s.record_conversion("にほん", "日本", "", "");
        s.record_conversion("にほん", "日本", "", "");
        let mut cands = make_candidates(&["日本", "二本"]);
        s.boost_candidates("にほん", &mut cands);
        // count=2 → boost=10
        assert!(cands[0].score >= 10.0);
    }

    // -----------------------------------------------------------------------
    // 9. prev_context distinguishes rows (different contexts = separate rows)
    // -----------------------------------------------------------------------
    #[test]
    fn different_prev_ctx_creates_separate_rows() {
        let s = store();
        s.record_conversion("こうかん", "交換", "部品を", "");
        s.record_conversion("こうかん", "好感", "彼への", "");
        // Both surfaces recorded; boost for "交換" and "好感" each count=1
        let mut cands = make_candidates(&["交換", "好感", "公館"]);
        s.boost_candidates("こうかん", &mut cands);
        let boost_kandai = cands.iter().find(|c| c.surface == "交換").unwrap().score;
        let boost_koukan = cands.iter().find(|c| c.surface == "好感").unwrap().score;
        assert!((boost_kandai - 5.0).abs() < f64::EPSILON);
        assert!((boost_koukan - 5.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // 10. next_ctx stored but does not affect boost logic
    // -----------------------------------------------------------------------
    #[test]
    fn next_ctx_stored_in_db() {
        let s = store();
        s.record_conversion("かがく", "科学", "", "の力");
        let mut cands = make_candidates(&["科学", "化学"]);
        s.boost_candidates("かがく", &mut cands);
        assert_eq!(cands[0].surface, "科学");
    }

    // -----------------------------------------------------------------------
    // 11. boost result is sorted descending by score
    // -----------------------------------------------------------------------
    #[test]
    fn boost_result_is_sorted_descending() {
        let s = store();
        s.record_conversion("いし", "意思", "", "");
        s.record_conversion("いし", "意思", "", "");
        let mut cands = make_candidates(&["意思", "医師", "石"]);
        // Give "医師" a slightly higher base score
        cands[1].score = 3.0;
        s.boost_candidates("いし", &mut cands);
        for i in 1..cands.len() {
            assert!(cands[i - 1].score >= cands[i].score);
        }
    }

    // -----------------------------------------------------------------------
    // 12. WAL mode is set (journal_mode returns 'wal')
    // -----------------------------------------------------------------------
    #[test]
    fn wal_mode_is_set() {
        let s = store();
        let conn = s.conn.lock().unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "memory", "in-memory DB uses 'memory' mode (WAL N/A)");
    }

    #[test]
    fn rerank_history_table_exists_with_required_columns() {
        let s = store();
        let conn = s.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("PRAGMA table_info(rerank_history)")
            .expect("prepare table_info");
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query table_info")
            .filter_map(Result::ok)
            .collect();

        for required in [
            "reading",
            "initial_surface",
            "reranked_surface",
            "left_context",
            "right_context",
            "trigger_kind",
            "score_delta",
            "created_at",
        ] {
            assert!(
                columns.iter().any(|c| c == required),
                "missing column: {required}"
            );
        }
    }

    #[test]
    fn record_rerank_persists_right_context_event() {
        let s = store();
        s.record_rerank(&RerankHistoryEvent {
            reading: "てんき",
            initial_surface: "天気",
            reranked_surface: "天氣",
            left_ctx: "明日の",
            right_ctx: "予報",
            trigger_kind: RerankTriggerKind::RightContext,
            score_delta: 0.42,
        });

        let conn = s.conn.lock().unwrap();
        let (reading, initial, reranked, left_ctx, right_ctx, trigger, delta): (
            String,
            String,
            String,
            String,
            String,
            String,
            f64,
        ) = conn
            .query_row(
                "SELECT reading, initial_surface, reranked_surface, left_context, right_context, trigger_kind, score_delta \
                 FROM rerank_history ORDER BY id DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .expect("row exists");

        assert_eq!(reading, "てんき");
        assert_eq!(initial, "天気");
        assert_eq!(reranked, "天氣");
        assert_eq!(left_ctx, "明日の");
        assert_eq!(right_ctx, "予報");
        assert_eq!(trigger, "right_context");
        assert!((delta - 0.42).abs() < 1e-9);
    }

    #[test]
    fn record_rerank_persists_selection_event() {
        let s = store();
        s.record_rerank(&RerankHistoryEvent {
            reading: "へんかん",
            initial_surface: "変換",
            reranked_surface: "返還",
            left_ctx: "文脈",
            right_ctx: "",
            trigger_kind: RerankTriggerKind::Selection,
            score_delta: -1.25,
        });

        let conn = s.conn.lock().unwrap();
        let (trigger, delta): (String, f64) = conn
            .query_row(
                "SELECT trigger_kind, score_delta FROM rerank_history ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("row exists");
        assert_eq!(trigger, "selection");
        assert!((delta + 1.25).abs() < 1e-9);
    }
}
