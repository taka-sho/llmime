use crate::session::with_session;

/// Returns the current candidate list for a session.
pub fn get_candidates(session_id: u64) -> Vec<String> {
    with_session(session_id, |s| s.candidates.clone()).unwrap_or_default()
}
