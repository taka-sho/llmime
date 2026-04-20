use super::DEFAULT_MIN_CONFIDENCE_DELTA;

pub const DEFAULT_HIGHLIGHT_CONFIDENCE_DELTA: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateUxFeedback {
    None,
    Silent,
    Highlight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationCommand {
    Start {
        animation_id: u64,
        cancel_previous: Option<u64>,
    },
    Cancel {
        animation_id: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationPlan {
    pub feedback: UpdateUxFeedback,
    pub command: Option<AnimationCommand>,
}

/// Controls rerank update UX behavior:
/// - F-114: highlight when confidence delta >= 0.3
/// - F-115: silent update when 0.2 <= delta < 0.3
/// - reduced motion: disables highlight animation, keeps silent updates
#[derive(Debug, Clone)]
pub struct AnimationScheduler {
    active_animation_id: Option<u64>,
    next_animation_id: u64,
    min_confidence_delta: f32,
    highlight_confidence_delta: f32,
    reduced_motion: bool,
}

impl Default for AnimationScheduler {
    fn default() -> Self {
        Self::new(false)
    }
}

impl AnimationScheduler {
    pub fn new(reduced_motion: bool) -> Self {
        Self {
            active_animation_id: None,
            next_animation_id: 1,
            min_confidence_delta: DEFAULT_MIN_CONFIDENCE_DELTA,
            highlight_confidence_delta: DEFAULT_HIGHLIGHT_CONFIDENCE_DELTA,
            reduced_motion,
        }
    }

    pub fn with_thresholds(
        reduced_motion: bool,
        min_confidence_delta: f32,
        highlight_confidence_delta: f32,
    ) -> Self {
        Self {
            min_confidence_delta,
            highlight_confidence_delta,
            ..Self::new(reduced_motion)
        }
    }

    pub fn set_reduced_motion(&mut self, reduced_motion: bool) {
        self.reduced_motion = reduced_motion;
    }

    pub fn active_animation_id(&self) -> Option<u64> {
        self.active_animation_id
    }

    pub fn plan_for_delta(&mut self, confidence_delta: f32) -> AnimationPlan {
        let feedback = self.feedback_for_delta(confidence_delta);
        match feedback {
            UpdateUxFeedback::Highlight => {
                let animation_id = self.next_animation_id;
                self.next_animation_id = self.next_animation_id.saturating_add(1);
                let cancel_previous = self.active_animation_id.replace(animation_id);
                AnimationPlan {
                    feedback,
                    command: Some(AnimationCommand::Start {
                        animation_id,
                        cancel_previous,
                    }),
                }
            }
            UpdateUxFeedback::Silent | UpdateUxFeedback::None => AnimationPlan {
                feedback,
                command: None,
            },
        }
    }

    pub fn cancel_active(&mut self) -> Option<AnimationCommand> {
        self.active_animation_id
            .take()
            .map(|animation_id| AnimationCommand::Cancel { animation_id })
    }

    pub fn complete(&mut self, animation_id: u64) -> bool {
        if self.active_animation_id == Some(animation_id) {
            self.active_animation_id = None;
            true
        } else {
            false
        }
    }

    fn feedback_for_delta(&self, confidence_delta: f32) -> UpdateUxFeedback {
        if confidence_delta + f32::EPSILON < self.min_confidence_delta {
            return UpdateUxFeedback::None;
        }
        if self.reduced_motion {
            return UpdateUxFeedback::Silent;
        }
        if confidence_delta + f32::EPSILON >= self.highlight_confidence_delta {
            UpdateUxFeedback::Highlight
        } else {
            UpdateUxFeedback::Silent
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_highlight_and_starts_animation_for_large_delta() {
        let mut scheduler = AnimationScheduler::default();

        let plan = scheduler.plan_for_delta(0.35);

        assert_eq!(plan.feedback, UpdateUxFeedback::Highlight);
        assert_eq!(
            plan.command,
            Some(AnimationCommand::Start {
                animation_id: 1,
                cancel_previous: None,
            })
        );
        assert_eq!(scheduler.active_animation_id(), Some(1));
    }

    #[test]
    fn suppresses_simultaneous_animation_by_cancelling_previous() {
        let mut scheduler = AnimationScheduler::default();
        let first = scheduler.plan_for_delta(0.31);
        assert_eq!(
            first.command,
            Some(AnimationCommand::Start {
                animation_id: 1,
                cancel_previous: None,
            })
        );

        let second = scheduler.plan_for_delta(0.4);
        assert_eq!(
            second.command,
            Some(AnimationCommand::Start {
                animation_id: 2,
                cancel_previous: Some(1),
            })
        );
        assert_eq!(scheduler.active_animation_id(), Some(2));
    }

    #[test]
    fn uses_silent_update_for_mid_delta_and_no_animation_command() {
        let mut scheduler = AnimationScheduler::default();
        let plan = scheduler.plan_for_delta(0.25);

        assert_eq!(plan.feedback, UpdateUxFeedback::Silent);
        assert_eq!(plan.command, None);
        assert_eq!(scheduler.active_animation_id(), None);
    }

    #[test]
    fn reduced_motion_disables_highlight_animation() {
        let mut scheduler = AnimationScheduler::new(true);
        let plan = scheduler.plan_for_delta(0.8);

        assert_eq!(plan.feedback, UpdateUxFeedback::Silent);
        assert_eq!(plan.command, None);
        assert_eq!(scheduler.active_animation_id(), None);
    }

    #[test]
    fn cancel_and_complete_only_apply_to_active_animation() {
        let mut scheduler = AnimationScheduler::default();
        let start = scheduler.plan_for_delta(0.35);
        assert_eq!(
            start.command,
            Some(AnimationCommand::Start {
                animation_id: 1,
                cancel_previous: None,
            })
        );

        assert!(!scheduler.complete(42), "stale completion must be ignored");
        assert_eq!(scheduler.active_animation_id(), Some(1));

        let cancel = scheduler.cancel_active();
        assert_eq!(cancel, Some(AnimationCommand::Cancel { animation_id: 1 }));
        assert_eq!(scheduler.active_animation_id(), None);
        assert!(
            !scheduler.complete(1),
            "cancelled animation is no longer active"
        );
    }
}
