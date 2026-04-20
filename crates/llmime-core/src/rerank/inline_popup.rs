#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PopupCandidate {
    pub surface: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PopupCandidateView {
    pub surface: String,
    pub confidence: f32,
    pub gauge_level: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InlinePopupCard {
    pub anchor: SelectionRange,
    pub candidates: Vec<PopupCandidateView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupCloseReason {
    Escape,
    OutsideClick,
    Applied,
    Programmatic,
}

pub trait InlinePopupUiLayer {
    fn show_inline_popup(&mut self, card: &InlinePopupCard);
    fn close_inline_popup(&mut self, reason: PopupCloseReason);
}

pub struct InlinePopupController<L: InlinePopupUiLayer> {
    ui: L,
    card: Option<InlinePopupCard>,
}

impl<L: InlinePopupUiLayer> InlinePopupController<L> {
    pub const MAX_CANDIDATES: usize = 3;

    pub fn new(ui: L) -> Self {
        Self { ui, card: None }
    }

    pub fn open(&mut self, anchor: SelectionRange, candidates: Vec<PopupCandidate>) -> bool {
        let visible_candidates: Vec<PopupCandidateView> = candidates
            .into_iter()
            .take(Self::MAX_CANDIDATES)
            .map(|c| PopupCandidateView {
                surface: c.surface,
                confidence: clamp_confidence(c.confidence),
                gauge_level: confidence_to_gauge(c.confidence),
            })
            .collect();
        if visible_candidates.is_empty() {
            return false;
        }

        let card = InlinePopupCard {
            anchor,
            candidates: visible_candidates,
        };
        self.ui.show_inline_popup(&card);
        self.card = Some(card);
        true
    }

    pub fn on_escape(&mut self) -> bool {
        self.close(PopupCloseReason::Escape)
    }

    pub fn on_outside_click(&mut self) -> bool {
        self.close(PopupCloseReason::OutsideClick)
    }

    pub fn close(&mut self, reason: PopupCloseReason) -> bool {
        if self.card.is_none() {
            return false;
        }
        self.ui.close_inline_popup(reason);
        self.card = None;
        true
    }

    pub fn is_visible(&self) -> bool {
        self.card.is_some()
    }

    pub fn card(&self) -> Option<&InlinePopupCard> {
        self.card.as_ref()
    }

    pub fn into_inner(self) -> L {
        self.ui
    }
}

fn clamp_confidence(confidence: f32) -> f32 {
    confidence.clamp(0.0, 1.0)
}

fn confidence_to_gauge(confidence: f32) -> u8 {
    (clamp_confidence(confidence) * 10.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockLayer {
        shown: Vec<InlinePopupCard>,
        closed: Vec<PopupCloseReason>,
    }

    impl InlinePopupUiLayer for MockLayer {
        fn show_inline_popup(&mut self, card: &InlinePopupCard) {
            self.shown.push(card.clone());
        }

        fn close_inline_popup(&mut self, reason: PopupCloseReason) {
            self.closed.push(reason);
        }
    }

    #[test]
    fn rerank_popup_display_shows_top_three_candidates() {
        let mut controller = InlinePopupController::new(MockLayer::default());
        let opened = controller.open(
            SelectionRange { start: 2, end: 5 },
            vec![
                PopupCandidate {
                    surface: "天気".into(),
                    confidence: 0.91,
                },
                PopupCandidate {
                    surface: "転機".into(),
                    confidence: 0.73,
                },
                PopupCandidate {
                    surface: "店記".into(),
                    confidence: 0.52,
                },
                PopupCandidate {
                    surface: "添記".into(),
                    confidence: 0.25,
                },
            ],
        );

        assert!(opened);
        let card = controller.card().expect("card must be visible");
        assert_eq!(card.anchor, SelectionRange { start: 2, end: 5 });
        assert_eq!(card.candidates.len(), 3);
        assert_eq!(card.candidates[0].surface, "天気");
        assert_eq!(card.candidates[2].surface, "店記");
    }

    #[test]
    fn rerank_popup_close_on_escape() {
        let mut controller = InlinePopupController::new(MockLayer::default());
        controller.open(
            SelectionRange { start: 0, end: 2 },
            vec![PopupCandidate {
                surface: "東京".into(),
                confidence: 0.88,
            }],
        );

        assert!(controller.on_escape());
        assert!(!controller.is_visible());
        let layer = controller.into_inner();
        assert_eq!(layer.closed, vec![PopupCloseReason::Escape]);
    }

    #[test]
    fn rerank_popup_close_on_outside_click() {
        let mut controller = InlinePopupController::new(MockLayer::default());
        controller.open(
            SelectionRange { start: 0, end: 2 },
            vec![PopupCandidate {
                surface: "東京".into(),
                confidence: 0.88,
            }],
        );

        assert!(controller.on_outside_click());
        assert!(!controller.is_visible());
        let layer = controller.into_inner();
        assert_eq!(layer.closed, vec![PopupCloseReason::OutsideClick]);
    }

    #[test]
    fn rerank_popup_candidate_render_maps_confidence_to_gauge() {
        let mut controller = InlinePopupController::new(MockLayer::default());
        controller.open(
            SelectionRange { start: 1, end: 2 },
            vec![
                PopupCandidate {
                    surface: "A".into(),
                    confidence: 1.2,
                },
                PopupCandidate {
                    surface: "B".into(),
                    confidence: 0.55,
                },
                PopupCandidate {
                    surface: "C".into(),
                    confidence: -0.2,
                },
            ],
        );
        let card = controller.card().expect("card must be visible");
        assert_eq!(card.candidates[0].gauge_level, 10);
        assert_eq!(card.candidates[1].gauge_level, 6);
        assert_eq!(card.candidates[2].gauge_level, 0);
        assert_eq!(card.candidates[0].confidence, 1.0);
        assert_eq!(card.candidates[2].confidence, 0.0);
    }
}
