use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModifierState {
    pub command_or_ctrl: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionRerankRequest {
    pub selected_text: String,
    pub start: usize,
    pub end: usize,
    pub forced: bool,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectionKey {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct PendingSelection {
    key: SelectionKey,
    text: String,
    since: Instant,
}

/// Debounces selection-driven rerank triggers and applies suppression rules.
pub struct SelectionRerankTrigger {
    debounce: Duration,
    in_composition: bool,
    last_emitted: Option<SelectionKey>,
    pending: Option<PendingSelection>,
}

impl SelectionRerankTrigger {
    pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(200);

    pub fn new() -> Self {
        Self::with_debounce(Self::DEFAULT_DEBOUNCE)
    }

    pub fn with_debounce(debounce: Duration) -> Self {
        Self {
            debounce,
            in_composition: false,
            last_emitted: None,
            pending: None,
        }
    }

    pub fn set_composition(&mut self, active: bool) {
        self.in_composition = active;
        if active {
            self.pending = None;
        }
    }

    pub fn on_selection_change(
        &mut self,
        start: usize,
        end: usize,
        text: &str,
        modifiers: ModifierState,
        now: Instant,
    ) {
        if self.in_composition || modifiers.command_or_ctrl {
            return;
        }
        if start >= end || text.is_empty() {
            self.pending = None;
            return;
        }

        let key = SelectionKey { start, end };
        if self.last_emitted == Some(key) {
            return;
        }

        self.pending = Some(PendingSelection {
            key,
            text: text.to_string(),
            since: now,
        });
    }

    pub fn poll(&mut self, now: Instant) -> Option<SelectionRerankRequest> {
        let pending = self.pending.as_ref()?;
        if now.duration_since(pending.since) < self.debounce {
            return None;
        }
        let pending = self.pending.take().expect("pending must exist");
        self.last_emitted = Some(pending.key);
        Some(SelectionRerankRequest {
            selected_text: pending.text,
            start: pending.key.start,
            end: pending.key.end,
            forced: false,
            timestamp: now,
        })
    }

    /// Entry point for Cmd/Ctrl+Shift+R style forced re-evaluation.
    pub fn force_reevaluate(
        &mut self,
        start: usize,
        end: usize,
        text: &str,
        now: Instant,
    ) -> Option<SelectionRerankRequest> {
        if start >= end || text.is_empty() {
            return None;
        }
        let key = SelectionKey { start, end };
        self.pending = None;
        self.last_emitted = Some(key);
        Some(SelectionRerankRequest {
            selected_text: text.to_string(),
            start,
            end,
            forced: true,
            timestamp: now,
        })
    }
}

impl Default for SelectionRerankTrigger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rerank_selection_debounce_waits_200ms_before_emitting() {
        let mut trigger = SelectionRerankTrigger::new();
        let t0 = Instant::now();
        trigger.on_selection_change(1, 3, "東京", ModifierState::default(), t0);
        assert!(trigger.poll(t0 + Duration::from_millis(199)).is_none());
        let req = trigger
            .poll(t0 + Duration::from_millis(200))
            .expect("request should be emitted");
        assert!(!req.forced);
        assert_eq!(req.selected_text, "東京");
    }

    #[test]
    fn rerank_selection_suppresses_empty_duplicate_and_composition() {
        let mut trigger = SelectionRerankTrigger::new();
        let t0 = Instant::now();

        trigger.on_selection_change(2, 2, "", ModifierState::default(), t0);
        assert!(trigger.poll(t0 + Duration::from_secs(1)).is_none());

        trigger.on_selection_change(4, 7, "abc", ModifierState::default(), t0);
        assert!(trigger.poll(t0 + Duration::from_millis(250)).is_some());

        trigger.on_selection_change(4, 7, "abc", ModifierState::default(), t0);
        assert!(trigger.poll(t0 + Duration::from_secs(1)).is_none());

        trigger.set_composition(true);
        trigger.on_selection_change(8, 11, "def", ModifierState::default(), t0);
        assert!(trigger.poll(t0 + Duration::from_secs(1)).is_none());
    }

    #[test]
    fn rerank_selection_suppresses_when_command_or_ctrl_is_pressed() {
        let mut trigger = SelectionRerankTrigger::new();
        let t0 = Instant::now();
        trigger.on_selection_change(
            0,
            3,
            "abc",
            ModifierState {
                command_or_ctrl: true,
            },
            t0,
        );
        assert!(trigger.poll(t0 + Duration::from_secs(1)).is_none());
    }

    #[test]
    fn rerank_shortcut_force_reevaluate_bypasses_duplicate_suppression() {
        let mut trigger = SelectionRerankTrigger::new();
        let t0 = Instant::now();

        trigger.on_selection_change(3, 5, "天気", ModifierState::default(), t0);
        assert!(trigger.poll(t0 + Duration::from_millis(220)).is_some());

        let forced = trigger
            .force_reevaluate(3, 5, "天気", t0 + Duration::from_millis(230))
            .expect("forced request should always be returned");
        assert!(forced.forced);
        assert_eq!(forced.selected_text, "天気");
    }
}
