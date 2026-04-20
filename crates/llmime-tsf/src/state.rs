use std::sync::{Arc, Mutex};

/// UTF-16 preedit buffer and cursor state shared between TSF components.
#[derive(Default)]
pub struct CompositionState {
    /// Current preedit (uncommitted) text in UTF-16.
    pub preedit: Vec<u16>,
    /// ACP cursor position (end of preedit = preedit.len() as i32).
    pub cursor: i32,
    /// Latch set by key_sink when Ctrl/Cmd+Shift+R is detected.
    pub force_rerank_requested: bool,
}

impl CompositionState {
    pub fn is_composing(&self) -> bool {
        !self.preedit.is_empty()
    }

    pub fn append_char(&mut self, ch: u16) {
        self.preedit.push(ch);
        self.cursor = self.preedit.len() as i32;
    }

    pub fn backspace(&mut self) {
        if !self.preedit.is_empty() {
            self.preedit.pop();
            self.cursor = self.preedit.len() as i32;
        }
    }

    pub fn commit(&mut self) -> Vec<u16> {
        let committed = std::mem::take(&mut self.preedit);
        self.cursor = 0;
        committed
    }

    pub fn cancel(&mut self) {
        self.preedit.clear();
        self.cursor = 0;
    }

    pub fn request_force_rerank(&mut self) {
        self.force_rerank_requested = true;
    }

    pub fn take_force_rerank_request(&mut self) -> bool {
        std::mem::take(&mut self.force_rerank_requested)
    }
}

pub type SharedState = Arc<Mutex<CompositionState>>;

pub fn new_shared_state() -> SharedState {
    Arc::new(Mutex::new(CompositionState::default()))
}

#[cfg(test)]
mod tests {
    use super::CompositionState;

    #[test]
    fn force_rerank_request_is_latched_until_taken() {
        let mut state = CompositionState::default();
        assert!(!state.take_force_rerank_request());
        state.request_force_rerank();
        assert!(state.take_force_rerank_request());
        assert!(!state.take_force_rerank_request());
    }
}
