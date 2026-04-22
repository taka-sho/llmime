use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    LmStudio,
    Jan,
    Custom,
}

#[derive(Debug, Clone)]
pub struct ModelCandidate {
    pub path: PathBuf,
    pub filename: String,
    pub size_bytes: u64,
    pub source: ModelSource,
}

/// Preferred model name fragments (case-insensitive) — higher score = better default
const PREFERRED_PATTERNS: &[&str] = &["qwen2.5", "qwen", "gemma"];

fn preference_score(filename: &str) -> u8 {
    let lower = filename.to_lowercase();
    for (i, pattern) in PREFERRED_PATTERNS.iter().enumerate() {
        if lower.contains(pattern) {
            return (PREFERRED_PATTERNS.len() - i) as u8;
        }
    }
    0
}

fn scan_dir(dir: &PathBuf, source: ModelSource, candidates: &mut Vec<ModelCandidate>) {
    if !dir.exists() {
        return;
    }
    scan_recursive(dir, &source, candidates);
}

fn scan_recursive(dir: &PathBuf, source: &ModelSource, candidates: &mut Vec<ModelCandidate>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_recursive(&path, source, candidates);
        } else if path.extension().is_some_and(|e| e == "gguf") {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_owned();
            let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            candidates.push(ModelCandidate {
                path,
                filename,
                size_bytes,
                source: source.clone(),
            });
        }
    }
}

/// Scan well-known local model directories for GGUF files.
///
/// Results are sorted: preferred models (Qwen, Gemma) first, then by size ascending.
pub fn scan_local_models(extra_paths: &[PathBuf]) -> Vec<ModelCandidate> {
    let mut candidates: Vec<ModelCandidate> = Vec::new();

    if let Some(home) = dirs::home_dir() {
        // LM Studio
        let lm_studio = home
            .join("Library")
            .join("Application Support")
            .join("LM Studio")
            .join("models");
        scan_dir(&lm_studio, ModelSource::LmStudio, &mut candidates);

        // Jan
        let jan = home
            .join("Library")
            .join("Application Support")
            .join("Jan")
            .join("models");
        scan_dir(&jan, ModelSource::Jan, &mut candidates);
    }

    // Custom paths from config
    for path in extra_paths {
        scan_dir(path, ModelSource::Custom, &mut candidates);
    }

    // Sort: higher preference score first, then smaller size first
    candidates.sort_by(|a, b| {
        let pa = preference_score(&a.filename);
        let pb = preference_score(&b.filename);
        pb.cmp(&pa).then_with(|| a.size_bytes.cmp(&b.size_bytes))
    });

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_gguf(dir: &std::path::Path, name: &str, size: u64) -> PathBuf {
        let path = dir.join(name);
        let data = vec![0u8; size as usize];
        fs::write(&path, &data).unwrap();
        path
    }

    #[test]
    fn test_scan_empty_dir() {
        let dir = TempDir::new().unwrap();
        let results = scan_local_models(&[dir.path().to_path_buf()]);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_scan_nonexistent_dir() {
        let results = scan_local_models(&[PathBuf::from("/nonexistent/path/that/does/not/exist")]);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_scan_finds_gguf_files() {
        let dir = TempDir::new().unwrap();
        make_gguf(dir.path(), "model-a.gguf", 100);
        make_gguf(dir.path(), "model-b.gguf", 200);
        fs::write(dir.path().join("readme.txt"), b"hello").unwrap();

        let results = scan_local_models(&[dir.path().to_path_buf()]);
        assert_eq!(results.len(), 2);
        for r in &results {
            assert!(r.filename.ends_with(".gguf"));
            assert_eq!(r.source, ModelSource::Custom);
        }
    }

    #[test]
    fn test_scan_recursive() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        make_gguf(dir.path(), "top.gguf", 50);
        make_gguf(&sub, "nested.gguf", 30);

        let results = scan_local_models(&[dir.path().to_path_buf()]);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_preferred_models_sorted_first() {
        let dir = TempDir::new().unwrap();
        make_gguf(dir.path(), "unknown-model.gguf", 10);
        make_gguf(dir.path(), "qwen2.5-1.5b-q4.gguf", 20);
        make_gguf(dir.path(), "gemma-2b.gguf", 15);

        let results = scan_local_models(&[dir.path().to_path_buf()]);
        assert_eq!(results.len(), 3);
        assert!(results[0].filename.contains("qwen2.5"));
        assert!(results[2].filename.contains("unknown"));
    }

    #[test]
    fn test_size_sort_within_same_priority() {
        let dir = TempDir::new().unwrap();
        make_gguf(dir.path(), "qwen-large.gguf", 200);
        make_gguf(dir.path(), "qwen-small.gguf", 50);

        let results = scan_local_models(&[dir.path().to_path_buf()]);
        assert_eq!(results.len(), 2);
        assert!(results[0].size_bytes <= results[1].size_bytes);
    }
}
