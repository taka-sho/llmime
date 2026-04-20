//! SelectionSink — detects selection range changes via Windows TSF.

use std::time::Instant;

pub struct SelectionEvent {
    pub selected_text: String,
    pub start: i32,
    pub end: i32,
    pub timestamp: Instant,
}

/// Detects selection range changes, suppressing events during active composition.
pub struct SelectionSink {
    is_composing: bool,
    last_start: i32,
    last_end: i32,
}

impl SelectionSink {
    pub fn new() -> Self {
        Self {
            is_composing: false,
            last_start: 0,
            last_end: 0,
        }
    }

    /// Update composition state; suppresses selection events while composing.
    pub fn set_composing(&mut self, composing: bool) {
        self.is_composing = composing;
    }

    /// Called when the selection range changes (from ITextStoreACP::OnSelectionChange).
    /// Returns `None` during active composition (preedit) or when selection is empty (cursor only).
    pub fn on_selection_change(
        &mut self,
        acp_start: i32,
        acp_end: i32,
        text: &str,
    ) -> Option<SelectionEvent> {
        if self.is_composing {
            return None;
        }
        if acp_start == acp_end {
            return None;
        }
        self.last_start = acp_start;
        self.last_end = acp_end;
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
    fn selection_change_fires_event() {
        let mut sink = SelectionSink::new();
        let event = sink.on_selection_change(0, 5, "hello");
        let event = event.expect("should fire event on selection change");
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
        sink.set_composing(true);
        assert!(sink.on_selection_change(0, 3, "abc").is_none());
        sink.set_composing(false);
        let event = sink.on_selection_change(0, 3, "abc");
        assert!(event.is_some(), "should fire event after composition ends");
    }
}
