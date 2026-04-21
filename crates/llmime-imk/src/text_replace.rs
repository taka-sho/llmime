//! Candidate selection text replacement helpers for macOS IMK.
//! F-124: replace selected committed text via `insertText:replacementRange:`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    pub location: usize,
    pub length: usize,
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

#[cfg(test)]
mod tests {
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
}
