//! SelectionSink — detects selection range changes via Windows TSF.

use std::time::{Duration, Instant};

pub struct SelectionEvent {
    pub selected_text: String,
    pub start: i32,
    pub end: i32,
    pub timestamp: Instant,
}

struct PendingSelection {
    start: i32,
    end: i32,
    text: String,
    since: Instant,
}

/// Detects selection range changes, suppressing events during active composition.
pub struct SelectionSink {
    is_composing: bool,
    last_selection: Option<(i32, i32)>,
    pending: Option<PendingSelection>,
}

impl SelectionSink {
    pub const CONFIRM_DELAY: Duration = Duration::from_millis(200);

    pub fn new() -> Self {
        Self {
            is_composing: false,
            last_selection: None,
            pending: None,
        }
    }

    /// Update composition state; suppresses selection events while composing.
    pub fn set_composing(&mut self, composing: bool) {
        self.is_composing = composing;
        if composing {
            self.pending = None;
        }
    }

    /// Called when the selection range changes (from ITextStoreACP::OnSelectionChange).
    /// Returns `None` immediately; call `poll_confirmed` after debounce delay.
    pub fn on_selection_change(
        &mut self,
        acp_start: i32,
        acp_end: i32,
        text: &str,
    ) -> Option<SelectionEvent> {
        self.on_selection_change_at(acp_start, acp_end, text, Instant::now())
    }

    fn on_selection_change_at(
        &mut self,
        acp_start: i32,
        acp_end: i32,
        text: &str,
        now: Instant,
    ) -> Option<SelectionEvent> {
        if self.is_composing {
            return None;
        }
        if acp_start == acp_end {
            self.pending = None;
            return None;
        }
        if self.last_selection == Some((acp_start, acp_end)) {
            return None;
        }
        self.pending = Some(PendingSelection {
            start: acp_start,
            end: acp_end,
            text: text.to_string(),
            since: now,
        });
        None
    }

    pub fn poll_confirmed(&mut self) -> Option<SelectionEvent> {
        self.poll_confirmed_at(Instant::now())
    }

    pub fn poll_confirmed_at(&mut self, now: Instant) -> Option<SelectionEvent> {
        let pending = self.pending.as_ref()?;
        if now.duration_since(pending.since) < Self::CONFIRM_DELAY {
            return None;
        }
        let pending = self.pending.take().expect("pending must exist");
        self.last_selection = Some((pending.start, pending.end));
        Some(SelectionEvent {
            selected_text: pending.text,
            start: pending.start,
            end: pending.end,
            timestamp: now,
        })
    }

    /// Entry point for Cmd/Ctrl+Shift+R style forced re-evaluation.
    pub fn force_reevaluate(
        &mut self,
        acp_start: i32,
        acp_end: i32,
        text: &str,
    ) -> Option<SelectionEvent> {
        if acp_start == acp_end || text.is_empty() {
            return None;
        }
        self.pending = None;
        self.last_selection = Some((acp_start, acp_end));
        Some(SelectionEvent {
            selected_text: text.to_string(),
            start: acp_start,
            end: acp_end,
            timestamp: Instant::now(),
        })
    }
}

impl Default for SelectionSink {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_change_fires_event_after_delay() {
        let mut sink = SelectionSink::new();
        let t0 = Instant::now();
        assert!(sink.on_selection_change_at(0, 5, "hello", t0).is_none());
        assert!(sink
            .poll_confirmed_at(t0 + Duration::from_millis(199))
            .is_none());
        let event = sink
            .poll_confirmed_at(t0 + SelectionSink::CONFIRM_DELAY)
            .expect("should fire event after debounce");
        assert_eq!(event.selected_text, "hello");
        assert_eq!(event.start, 0);
        assert_eq!(event.end, 5);
    }

    #[test]
    fn preedit_active_suppresses_event() {
        let mut sink = SelectionSink::new();
        sink.set_composing(true);
        let event = sink.on_selection_change(0, 3, "abc");
        assert!(event.is_none(), "should suppress event during composition");
    }

    #[test]
    fn empty_selection_no_event() {
        let mut sink = SelectionSink::new();
        // acpStart == acpEnd means cursor position only, not a selection
        let event = sink.on_selection_change(4, 4, "");
        assert!(event.is_none(), "cursor position should not fire event");
    }

    #[test]
    fn events_resume_after_composition_ends() {
        let mut sink = SelectionSink::new();
        let t0 = Instant::now();
        sink.set_composing(true);
        assert!(sink.on_selection_change(0, 3, "abc").is_none());
        sink.set_composing(false);
        sink.on_selection_change_at(0, 3, "abc", t0);
        let event = sink.poll_confirmed_at(t0 + SelectionSink::CONFIRM_DELAY);
        assert!(event.is_some(), "should fire event after composition ends");
    }

    #[test]
    fn duplicate_selection_is_suppressed() {
        let mut sink = SelectionSink::new();
        let t0 = Instant::now();
        sink.on_selection_change_at(0, 3, "abc", t0);
        assert!(sink
            .poll_confirmed_at(t0 + SelectionSink::CONFIRM_DELAY)
            .is_some());
        sink.on_selection_change(0, 3, "abc");
        assert!(sink
            .poll_confirmed_at(t0 + Duration::from_secs(1))
            .is_none());
    }

    #[test]
    fn force_reevaluate_emits_immediately() {
        let mut sink = SelectionSink::new();
        let event = sink
            .force_reevaluate(2, 5, "再評価")
            .expect("forced re-eval should emit");
        assert_eq!(event.start, 2);
        assert_eq!(event.end, 5);
        assert_eq!(event.selected_text, "再評価");
    }
}
