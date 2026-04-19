#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),
    #[error("backend unavailable: {0}")]
    Unavailable(String),
    #[error("upstream error: {0}")]
    Upstream(#[source] anyhow::Error),
    #[error("input too long: {0} tokens")]
    InputTooLong(usize),
    #[error("consent required for cloud inference")]
    ConsentRequired,
    #[error("inference cancelled")]
    Cancelled,
}
