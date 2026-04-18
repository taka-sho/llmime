pub mod kenlm;

pub use kenlm::KenLMModel;

pub trait LanguageModel: Send + Sync {
    /// 単語列のN-gram確率（log10）を返す
    fn score(&self, words: &[&str]) -> f64;
    /// モデルを読み込む
    fn load(path: &std::path::Path) -> anyhow::Result<Self>
    where
        Self: Sized;
}
