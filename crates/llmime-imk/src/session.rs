use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use llmime_core::{LlmimePaths, MozcReadingIndex, ReadingIndex};

/// Preedit buffer and candidate state for one IMK session.
pub struct Session {
    pub session_id: u64,
    pub preedit: String,
    pub candidates: Vec<String>,
}

impl Session {
    fn new(session_id: u64) -> Self {
        Self {
            session_id,
            preedit: String::new(),
            candidates: Vec::new(),
        }
    }

    /// Append a character to the preedit buffer and recompute candidates.
    pub fn push_char(&mut self, ch: char) {
        self.preedit.push(ch);
        self.refresh_candidates();
    }

    /// Remove the last character from the preedit buffer.
    pub fn pop_char(&mut self) {
        self.preedit.pop();
        self.refresh_candidates();
    }

    /// Commit the given candidate and clear preedit.
    pub fn commit(&mut self, candidate: &str) {
        log::debug!("session {}: commit {:?}", self.session_id, candidate);
        self.preedit.clear();
        self.candidates.clear();
    }

    fn refresh_candidates(&mut self) {
        if self.preedit.is_empty() {
            self.candidates.clear();
            return;
        }

        if let Some(idx) = reading_index() {
            let mut entries = idx.lookup(&self.preedit);
            entries.sort_by_key(|e| e.cost);
            self.candidates = entries.into_iter().take(10).map(|e| e.surface).collect();
            if self.candidates.is_empty() {
                // Fallback: passthrough when no dictionary match
                self.candidates = vec![self.preedit.clone()];
            }
        } else {
            // No index available: passthrough preedit as-is
            self.candidates = vec![self.preedit.clone()];
        }
    }
}

// ---------------------------------------------------------------------------
// Lazy-initialized MozcReadingIndex
// ---------------------------------------------------------------------------

fn reading_index() -> Option<&'static MozcReadingIndex> {
    static INDEX: OnceLock<Option<MozcReadingIndex>> = OnceLock::new();
    INDEX
        .get_or_init(|| {
            let paths = LlmimePaths::resolve();
            match MozcReadingIndex::load_from_dir(&paths.mozc_dir) {
                Ok(idx) => {
                    log::info!("MozcReadingIndex loaded from {:?}", paths.mozc_dir);
                    Some(idx)
                }
                Err(e) => {
                    log::warn!("MozcReadingIndex unavailable: {e}. Falling back to passthrough.");
                    None
                }
            }
        })
        .as_ref()
}

// ---------------------------------------------------------------------------
// Global session registry (keyed by session_id from Objective-C layer)
// ---------------------------------------------------------------------------

fn registry() -> &'static Mutex<HashMap<u64, Session>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, Session>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn session_begin(session_id: u64) {
    let mut map = registry().lock().unwrap();
    map.insert(session_id, Session::new(session_id));
    log::debug!("session {} started", session_id);
}

pub fn session_end(session_id: u64) {
    let mut map = registry().lock().unwrap();
    map.remove(&session_id);
    log::debug!("session {} ended", session_id);
}

/// Execute a closure with mutable access to the session; returns `None` if unknown.
pub fn with_session<F, R>(session_id: u64, f: F) -> Option<R>
where
    F: FnOnce(&mut Session) -> R,
{
    let mut map = registry().lock().unwrap();
    map.get_mut(&session_id).map(f)
}
