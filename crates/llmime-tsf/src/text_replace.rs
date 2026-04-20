//! Selection replacement helpers for TSF.
//! F-124: use RequestEditSession + ITfRange::SetText semantics.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceError(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceOutcome {
    pub used_fallback: bool,
    pub undo_entries: u32,
}

pub trait TsfRangeWriter {
    fn set_text(&mut self, start: i32, end: i32, text: &str) -> Result<(), ReplaceError>;
}

pub trait TsfEditSession {
    fn begin_undo_group(&mut self);
    fn end_undo_group(&mut self);
    fn request_edit_session<F>(&mut self, f: F) -> Result<(), ReplaceError>
    where
        F: FnOnce(&mut dyn TsfRangeWriter) -> Result<(), ReplaceError>;
}

/// Replaces a selected range with a reranked candidate via TSF edit session.
/// When primary replacement fails, retries with fallback range.
pub fn replace_selected_text_via_tsf(
    session: &mut impl TsfEditSession,
    primary_range: (i32, i32),
    fallback_range: (i32, i32),
    candidate: &str,
) -> ReplaceOutcome {
    session.begin_undo_group();

    let primary = session
        .request_edit_session(|range| range.set_text(primary_range.0, primary_range.1, candidate));

    let used_fallback = if primary.is_ok() {
        false
    } else {
        let _ = session.request_edit_session(|range| {
            range.set_text(fallback_range.0, fallback_range.1, candidate)
        });
        true
    };

    session.end_undo_group();

    ReplaceOutcome {
        used_fallback,
        undo_entries: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockWriter {
        calls: Vec<(i32, i32, String)>,
        fail_on_first: bool,
    }

    impl TsfRangeWriter for MockWriter {
        fn set_text(&mut self, start: i32, end: i32, text: &str) -> Result<(), ReplaceError> {
            self.calls.push((start, end, text.to_string()));
            if self.fail_on_first && self.calls.len() == 1 {
                Err(ReplaceError("set_text failed".into()))
            } else {
                Ok(())
            }
        }
    }

    #[derive(Default)]
    struct MockSession {
        writer: MockWriter,
        begin_count: u32,
        end_count: u32,
        request_count: u32,
    }

    impl TsfEditSession for MockSession {
        fn begin_undo_group(&mut self) {
            self.begin_count += 1;
        }

        fn end_undo_group(&mut self) {
            self.end_count += 1;
        }

        fn request_edit_session<F>(&mut self, f: F) -> Result<(), ReplaceError>
        where
            F: FnOnce(&mut dyn TsfRangeWriter) -> Result<(), ReplaceError>,
        {
            self.request_count += 1;
            f(&mut self.writer)
        }
    }

    #[test]
    fn rerank_text_replace_windows_uses_request_edit_session_and_set_text() {
        let mut session = MockSession::default();

        let outcome = replace_selected_text_via_tsf(&mut session, (5, 8), (0, 0), "候補");

        assert!(!outcome.used_fallback);
        assert_eq!(session.request_count, 1);
        assert_eq!(session.writer.calls, vec![(5, 8, "候補".to_string())]);
    }

    #[test]
    fn rerank_text_replace_windows_falls_back_when_primary_fails() {
        let mut session = MockSession {
            writer: MockWriter {
                fail_on_first: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let outcome = replace_selected_text_via_tsf(&mut session, (10, 14), (2, 2), "再変換");

        assert!(outcome.used_fallback);
        assert_eq!(session.request_count, 2);
        assert_eq!(session.writer.calls[0], (10, 14, "再変換".to_string()));
        assert_eq!(session.writer.calls[1], (2, 2, "再変換".to_string()));
    }

    #[test]
    fn rerank_text_replace_windows_groups_undo_into_single_entry() {
        let mut session = MockSession::default();

        let outcome = replace_selected_text_via_tsf(&mut session, (1, 3), (1, 1), "置換");

        assert_eq!(session.begin_count, 1);
        assert_eq!(session.end_count, 1);
        assert_eq!(outcome.undo_entries, 1);
    }
}
