use llmime_core::{InlinePopupCard, InlinePopupUiLayer, PopupCloseReason};

#[derive(Default)]
pub struct InlinePopupLayer;

impl InlinePopupLayer {
    pub fn new() -> Self {
        Self
    }
}

impl InlinePopupUiLayer for InlinePopupLayer {
    fn show_inline_popup(&mut self, _card: &InlinePopupCard) {
        #[cfg(target_os = "macos")]
        {
            log::debug!(
                "IMK inline popup show: anchor={}..{} candidates={}",
                _card.anchor.start,
                _card.anchor.end,
                _card.candidates.len()
            );
        }
    }

    fn close_inline_popup(&mut self, _reason: PopupCloseReason) {
        #[cfg(target_os = "macos")]
        {
            log::debug!("IMK inline popup close: {:?}", _reason);
        }
    }
}
