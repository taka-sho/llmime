pub const DEFAULT_MIN_CONFIDENCE_DELTA: f32 = 0.2;

/// F-113 update gate:
/// 1) surface length must be unchanged
/// 2) confidence improvement delta >= threshold
/// 3) negative confidence improvement is always rejected
pub fn should_apply_update(
    current_surface: &str,
    current_confidence: f32,
    proposed_surface: &str,
    proposed_confidence: f32,
    min_confidence_delta: f32,
) -> bool {
    if current_surface.chars().count() != proposed_surface.chars().count() {
        return false;
    }

    let delta = proposed_confidence - current_confidence;
    if delta < 0.0 {
        return false;
    }

    delta + f32::EPSILON >= min_confidence_delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_allows_update_at_delta_boundary_when_length_matches() {
        let allowed = should_apply_update("機関", 0.60, "期間", 0.80, DEFAULT_MIN_CONFIDENCE_DELTA);
        assert!(allowed, "delta=0.2 should be allowed at threshold boundary");
    }

    #[test]
    fn gate_rejects_update_below_delta_threshold() {
        let allowed = should_apply_update("機関", 0.60, "期間", 0.79, DEFAULT_MIN_CONFIDENCE_DELTA);
        assert!(!allowed, "delta=0.19 should be rejected");
    }

    #[test]
    fn gate_rejects_update_when_surface_length_changes() {
        let allowed =
            should_apply_update("機関", 0.60, "期間中", 0.95, DEFAULT_MIN_CONFIDENCE_DELTA);
        assert!(!allowed, "length mismatch must reject update");
    }

    #[test]
    fn gate_rejects_negative_confidence_improvement() {
        let allowed = should_apply_update("機関", 0.60, "期間", 0.50, DEFAULT_MIN_CONFIDENCE_DELTA);
        assert!(!allowed, "negative confidence improvement must be rejected");
    }
}
