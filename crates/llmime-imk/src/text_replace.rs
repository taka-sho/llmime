//! Candidate selection text replacement helpers for macOS IMK.
//! F-124: replace selected committed text via `insertText:replacementRange:`.

use llmime_core::{HistoryStore, RerankHistoryEvent, RerankTriggerKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    pub location: usize,
    pub length: usize,
}

/// Metadata for recording a selection re-conversion event in the history DB.
pub struct SelectionReplaceParams<'a> {
    pub reading: &'a str,
    pub initial_surface: &'a str,
    pub reranked_surface: &'a str,
    pub left_ctx: &'a str,
    pub right_ctx: &'a str,
    pub score_delta: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceOutcome {
    pub used_fallback: bool,
    pub undo_entries: u32,
}

pub trait ImkTextClient {
    fn begin_undo_group(&mut self);
    fn end_undo_group(&mut self);
    fn selected_range(&self) -> Option<SelectionRange>;
    fn insert_text(&mut self, text: &str, replacement_range: Option<SelectionRange>) -> bool;
}

/// Replaces selected text atomically and falls back to insertion when range replacement fails.
pub fn replace_selected_text_atomic(
    client: &mut impl ImkTextClient,
    selected_text: &str,
) -> ReplaceOutcome {
    client.begin_undo_group();

    let primary_range = client.selected_range();
    let primary_ok = client.insert_text(selected_text, primary_range);

    let used_fallback = if primary_ok {
        false
    } else {
        let _ = client.insert_text(selected_text, None);
        true
    };

    client.end_undo_group();

    ReplaceOutcome {
        used_fallback,
        undo_entries: 1,
    }
}

/// Replaces selected text and records the event in the rerank history DB.
/// Calls `record_rerank` with `trigger_kind = Selection` unconditionally after replacement.
pub fn replace_and_record_selection(
    client: &mut impl ImkTextClient,
    params: &SelectionReplaceParams<'_>,
    store: &dyn HistoryStore,
) -> ReplaceOutcome {
    let outcome = replace_selected_text_atomic(client, params.reranked_surface);
    store.record_rerank(&RerankHistoryEvent {
        reading: params.reading,
        initial_surface: params.initial_surface,
        reranked_surface: params.reranked_surface,
        left_ctx: params.left_ctx,
        right_ctx: params.right_ctx,
        trigger_kind: RerankTriggerKind::Selection,
        score_delta: params.score_delta,
    });
    outcome
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use llmime_core::{Candidate, HistoryStore, RerankHistoryEvent, RerankTriggerKind};

    use super::*;

    #[derive(Default)]
    struct MockClient {
        selected: Option<SelectionRange>,
        primary_ok: bool,
        calls: Vec<Option<SelectionRange>>,
        begin_count: u32,
        end_count: u32,
    }

    impl ImkTextClient for MockClient {
        fn begin_undo_group(&mut self) {
            self.begin_count += 1;
        }

        fn end_undo_group(&mut self) {
            self.end_count += 1;
        }

        fn selected_range(&self) -> Option<SelectionRange> {
            self.selected
        }

        fn insert_text(&mut self, _text: &str, replacement_range: Option<SelectionRange>) -> bool {
            self.calls.push(replacement_range);
            if self.calls.len() == 1 {
                self.primary_ok
            } else {
                true
            }
        }
    }

    #[test]
    fn rerank_text_replace_uses_selected_range_on_macos() {
        let mut client = MockClient {
            selected: Some(SelectionRange {
                location: 10,
                length: 4,
            }),
            primary_ok: true,
            ..Default::default()
        };

        let outcome = replace_selected_text_atomic(&mut client, "候補");

        assert_eq!(client.calls, vec![client.selected]);
        assert!(!outcome.used_fallback);
    }

    #[test]
    fn rerank_text_replace_falls_back_when_primary_insert_fails() {
        let mut client = MockClient {
            selected: Some(SelectionRange {
                location: 2,
                length: 2,
            }),
            primary_ok: false,
            ..Default::default()
        };

        let outcome = replace_selected_text_atomic(&mut client, "置換");

        assert_eq!(client.calls.len(), 2);
        assert_eq!(client.calls[0], client.selected);
        assert_eq!(client.calls[1], None);
        assert!(outcome.used_fallback);
    }

    #[test]
    fn rerank_text_replace_groups_undo_into_single_entry_on_macos() {
        let mut client = MockClient {
            selected: Some(SelectionRange {
                location: 0,
                length: 1,
            }),
            primary_ok: true,
            ..Default::default()
        };

        let outcome = replace_selected_text_atomic(&mut client, "再");

        assert_eq!(client.begin_count, 1);
        assert_eq!(client.end_count, 1);
        assert_eq!(outcome.undo_entries, 1);
    }

    #[derive(Default)]
    struct SpyStore {
        recorded: Arc<Mutex<Vec<(String, RerankTriggerKind)>>>,
    }

    impl HistoryStore for SpyStore {
        fn record_conversion(&self, _: &str, _: &str, _: &str, _: &str) {}
        fn boost_candidates(&self, _: &str, _: &mut Vec<Candidate>) {}
        fn reset_all(&self) {}
        fn record_rerank(&self, event: &RerankHistoryEvent<'_>) {
            self.recorded
                .lock()
                .unwrap()
                .push((event.initial_surface.to_string(), event.trigger_kind));
        }
    }

    #[test]
    fn record_rerank_called_with_selection_trigger_kind_on_macos() {
        let mut client = MockClient {
            selected: Some(SelectionRange {
                location: 3,
                length: 2,
            }),
            primary_ok: true,
            ..Default::default()
        };
        let store = SpyStore::default();
        let params = SelectionReplaceParams {
            reading: "てんき",
            initial_surface: "天気",
            reranked_surface: "天氣",
            left_ctx: "明日の",
            right_ctx: "予報",
            score_delta: 0.5,
        };

        let outcome = replace_and_record_selection(&mut client, &params, &store);

        assert!(!outcome.used_fallback);
        let recorded = store.recorded.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "天気");
        assert_eq!(recorded[0].1, RerankTriggerKind::Selection);
    }
}
