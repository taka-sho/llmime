//! macOS IMKit 選択検知 — SelectionWatcher
//!
//! F-120: IMKCandidates の selectedRange 変化を監視し、drag end + 100ms で
//! 選択確定イベントを発火する。composition (preedit) 中は抑制。
//! Accessibility API (AXSelectedTextRange) は KVO 失敗時の fallback として使用。

use std::time::{Duration, Instant};

/// Mirrors NSRange (location + length in UTF-16 code units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NSRange {
    pub location: usize,
    pub length: usize,
}

impl NSRange {
    pub const EMPTY: Self = Self {
        location: 0,
        length: 0,
    };

    pub fn is_empty(self) -> bool {
        self.length == 0
    }
}

/// Emitted once per confirmed selection (drag end + CONFIRM_DELAY elapsed).
#[derive(Debug, Clone)]
pub struct SelectionEvent {
    pub selected_text: String,
    pub range: NSRange,
    pub timestamp: Instant,
}

struct PendingSelection {
    range: NSRange,
    text: String,
    since: Instant,
}

/// Watches IMK selection changes and emits confirmed SelectionEvents.
pub struct SelectionWatcher {
    last_selection: Option<NSRange>,
    /// true while IMKit preedit (composition) is active — suppresses events.
    in_composition: bool,
    pending: Option<PendingSelection>,
}

impl SelectionWatcher {
    /// Debounce duration: drag end → confirmed event.
    pub const CONFIRM_DELAY: Duration = Duration::from_millis(100);

    pub fn new() -> Self {
        Self {
            last_selection: None,
            in_composition: false,
            pending: None,
        }
    }

    /// Call from IMKit `compositionState` callbacks.
    ///
    /// When `active` is true (preedit started), any pending selection is
    /// discarded. When false (composition committed), watching resumes.
    pub fn set_composition(&mut self, active: bool) {
        self.in_composition = active;
        if active {
            self.pending = None;
        }
    }

    /// Notify the watcher of a new selection range from IMKCandidates KVO or
    /// AXSelectedTextRange fallback.
    ///
    /// Returns `None` — the event is not immediate. Call `poll_confirmed` to
    /// retrieve it after the debounce delay elapses.
    pub fn on_selection_change(
        &mut self,
        new_range: NSRange,
        text: &str,
    ) -> Option<SelectionEvent> {
        if self.in_composition {
            return None;
        }
        if new_range.is_empty() {
            self.pending = None;
            return None;
        }
        if self.last_selection == Some(new_range) {
            return None;
        }
        self.last_selection = Some(new_range);
        self.pending = Some(PendingSelection {
            range: new_range,
            text: text.to_string(),
            since: Instant::now(),
        });
        None
    }

    /// Check whether the debounce delay has elapsed; returns `SelectionEvent` if so.
    ///
    /// Call this on a timer (e.g., every 10 ms) from the IMKit run-loop.
    pub fn poll_confirmed(&mut self) -> Option<SelectionEvent> {
        self.poll_confirmed_at(Instant::now())
    }

    /// Testable variant — caller supplies the current time.
    pub fn poll_confirmed_at(&mut self, now: Instant) -> Option<SelectionEvent> {
        let elapsed = self.pending.as_ref()?.since;
        if now.duration_since(elapsed) >= Self::CONFIRM_DELAY {
            let p = self.pending.take().unwrap();
            Some(SelectionEvent {
                selected_text: p.text,
                range: p.range,
                timestamp: now,
            })
        } else {
            None
        }
    }
}

impl Default for SelectionWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn range(loc: usize, len: usize) -> NSRange {
        NSRange {
            location: loc,
            length: len,
        }
    }

    /// Mock IMK session: selection change → pending → event fires after delay.
    #[test]
    fn selection_change_emits_event_after_delay() {
        let mut watcher = SelectionWatcher::new();
        let r = range(5, 3);

        // Immediate call returns None (debounce not elapsed).
        assert!(watcher.on_selection_change(r, "abc").is_none());
        assert!(watcher.poll_confirmed_at(Instant::now()).is_none());

        // Simulate 100ms passing.
        let later = Instant::now() + SelectionWatcher::CONFIRM_DELAY;
        let event = watcher.poll_confirmed_at(later).expect("event must fire");
        assert_eq!(event.selected_text, "abc");
        assert_eq!(event.range, r);

        // Second poll returns None — event consumed.
        assert!(watcher
            .poll_confirmed_at(later + Duration::from_millis(1))
            .is_none());
    }

    /// Drag end + 100ms fires the event (timeout test).
    #[test]
    fn drag_end_plus_100ms_fires() {
        let mut watcher = SelectionWatcher::new();
        watcher.on_selection_change(range(0, 5), "hello");

        // 99ms — not yet.
        let t99 = Instant::now() + Duration::from_millis(99);
        assert!(watcher.poll_confirmed_at(t99).is_none());

        // Exactly 100ms — fires.
        let t100 = Instant::now() + SelectionWatcher::CONFIRM_DELAY;
        assert!(watcher.poll_confirmed_at(t100).is_some());
    }

    /// preedit (composition) active → selection changes are suppressed.
    #[test]
    fn preedit_suppresses_selection() {
        let mut watcher = SelectionWatcher::new();

        // Start composition.
        watcher.set_composition(true);
        assert!(watcher.on_selection_change(range(2, 4), "てんき").is_none());

        // No pending even after long delay.
        let far_future = Instant::now() + Duration::from_secs(10);
        assert!(watcher.poll_confirmed_at(far_future).is_none());

        // End composition — watching resumes.
        watcher.set_composition(false);
        watcher.on_selection_change(range(2, 4), "てんき");
        let t100 = Instant::now() + SelectionWatcher::CONFIRM_DELAY;
        assert!(watcher.poll_confirmed_at(t100).is_some());
    }

    /// set_composition(true) discards an in-flight pending selection.
    #[test]
    fn composition_start_discards_pending() {
        let mut watcher = SelectionWatcher::new();
        watcher.on_selection_change(range(1, 2), "ab");

        // Composition starts before delay elapses — pending must be cleared.
        watcher.set_composition(true);
        let far_future = Instant::now() + Duration::from_secs(1);
        assert!(watcher.poll_confirmed_at(far_future).is_none());
    }

    /// Empty selection (length == 0) is ignored.
    #[test]
    fn empty_range_ignored() {
        let mut watcher = SelectionWatcher::new();
        watcher.on_selection_change(NSRange::EMPTY, "");
        let far = Instant::now() + Duration::from_secs(1);
        assert!(watcher.poll_confirmed_at(far).is_none());
    }

    /// Duplicate (same range) selection is de-duplicated.
    #[test]
    fn duplicate_range_not_requeued() {
        let mut watcher = SelectionWatcher::new();
        let r = range(3, 2);
        watcher.on_selection_change(r, "ab");

        // Consume the first event.
        let t = Instant::now() + SelectionWatcher::CONFIRM_DELAY;
        watcher.poll_confirmed_at(t);

        // Same range again — no new event.
        watcher.on_selection_change(r, "ab");
        assert!(watcher
            .poll_confirmed_at(t + Duration::from_millis(200))
            .is_none());
    }
}
