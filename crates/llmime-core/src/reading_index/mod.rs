pub mod lattice;
pub mod mozc;

pub use lattice::{LmScorer, ViterbiConfig, ViterbiLattice};
pub use mozc::MozcReadingIndex;

#[derive(Debug, Clone, PartialEq)]
pub struct ReadingEntry {
    pub surface: String,
    pub reading: String,
    pub pos: String,
    pub cost: i32,
}

pub trait ReadingIndex: Send + Sync {
    fn lookup(&self, reading: &str) -> Vec<ReadingEntry>;
    fn prefix_search(&self, reading: &str) -> Vec<(usize, ReadingEntry)>;
}
