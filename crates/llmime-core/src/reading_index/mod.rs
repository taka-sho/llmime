pub mod lattice;
pub mod mozc;
pub mod pos_connection;

pub use lattice::{LmScorer, ViterbiConfig, ViterbiLattice};
pub use mozc::MozcReadingIndex;
pub use pos_connection::{PosClass, classify as classify_pos, connection_penalty};

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
