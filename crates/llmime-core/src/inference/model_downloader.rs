use std::path::{Path, PathBuf};

use futures::StreamExt;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

pub const DEFAULT_MODEL_NAME: &str = "qwen2.5-1.5b-instruct-q4_k_m";
pub const DEFAULT_MODEL_URL: &str =
    "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf";
pub const DEFAULT_MODEL_SHA256: &str =
    "9c7de6e0e5b7e01b5b8c3c5ed09e21d4f574a2f12acfad22e5e6e05e3b5e3c5a";
pub const DEFAULT_MODEL_SIZE_BYTES: u64 = 1_100_000_000;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("model directory unavailable: {0}")]
    DirUnavailable(String),
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub filename: String,
}

#[derive(Debug, Clone)]
pub struct DefaultModelConfig {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

impl Default for DefaultModelConfig {
    fn default() -> Self {
        Self {
            name: DEFAULT_MODEL_NAME.to_string(),
            url: DEFAULT_MODEL_URL.to_string(),
            sha256: DEFAULT_MODEL_SHA256.to_string(),
            size_bytes: DEFAULT_MODEL_SIZE_BYTES,
        }
    }
}

pub struct ModelDownloadManager {
    models_dir: PathBuf,
    client: reqwest::Client,
}

impl ModelDownloadManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_client(models_dir: PathBuf, client: reqwest::Client) -> Self {
        Self { models_dir, client }
    }

    pub fn model_path(&self, filename: &str) -> PathBuf {
        self.models_dir.join(filename)
    }

    pub async fn download_default(
        &self,
        cfg: &DefaultModelConfig,
        tx: Option<mpsc::Sender<DownloadProgress>>,
    ) -> Result<PathBuf, DownloadError> {
        let filename = format!("{}.gguf", cfg.name);
        self.download(&cfg.url, &filename, &cfg.sha256, tx).await
    }

    pub async fn download(
        &self,
        url: &str,
        filename: &str,
        expected_sha256: &str,
        tx: Option<mpsc::Sender<DownloadProgress>>,
    ) -> Result<PathBuf, DownloadError> {
        tokio::fs::create_dir_all(&self.models_dir)
            .await
            .map_err(|e| DownloadError::DirUnavailable(e.to_string()))?;

        let dest = self.models_dir.join(filename);
        let partial = self.models_dir.join(format!("{filename}.partial"));

        let response = self.client.get(url).send().await?;
        let total_bytes = response.content_length();

        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&partial).await?;
        let mut hasher = Sha256::new();
        let mut downloaded: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            hasher.update(&bytes);
            file.write_all(&bytes).await?;
            downloaded += bytes.len() as u64;

            if let Some(ref sender) = tx {
                let _ = sender.try_send(DownloadProgress {
                    downloaded_bytes: downloaded,
                    total_bytes,
                    filename: filename.to_string(),
                });
            }
        }
        file.flush().await?;
        drop(file);

        let actual = hex::encode(hasher.finalize());
        if actual != expected_sha256 {
            tokio::fs::remove_file(&partial).await.ok();
            return Err(DownloadError::ChecksumMismatch {
                expected: expected_sha256.to_string(),
                actual,
            });
        }

        tokio::fs::rename(&partial, &dest).await?;
        Ok(dest)
    }

    pub async fn ensure_default_model(
        &self,
        cfg: &DefaultModelConfig,
    ) -> Result<Option<PathBuf>, DownloadError> {
        let filename = format!("{}.gguf", cfg.name);
        let dest = self.model_path(&filename);
        if dest.exists() {
            return Ok(Some(dest));
        }

        eprintln!(
            "Default model not found. Download '{}' (~{:.1} GB) from HuggingFace? [y/N]",
            cfg.name,
            cfg.size_bytes as f64 / 1e9
        );

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        if input.trim().eq_ignore_ascii_case("y") {
            let (tx, mut rx) = mpsc::channel::<DownloadProgress>(32);
            tokio::spawn(async move {
                while let Some(p) = rx.recv().await {
                    let pct = p
                        .total_bytes
                        .map(|t| format!("{:.1}%", p.downloaded_bytes as f64 / t as f64 * 100.0))
                        .unwrap_or_else(|| format!("{} bytes", p.downloaded_bytes));
                    eprint!("\rDownloading {} … {}", p.filename, pct);
                }
                eprintln!();
            });
            let path = self.download_default(cfg, Some(tx)).await?;
            return Ok(Some(path));
        }
        Ok(None)
    }
}

pub fn default_models_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("llmime")
        .join("models")
}

pub fn resolve_model_path(path: Option<&Path>) -> Option<PathBuf> {
    path.filter(|p| p.exists()).map(|p| p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_manager(dir: &TempDir) -> ModelDownloadManager {
        ModelDownloadManager::new(dir.path().to_path_buf())
    }

    #[tokio::test]
    async fn test_download_success_and_checksum() {
        let server = MockServer::start().await;
        let body = b"hello world";
        let sha = hex::encode(Sha256::digest(body));

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.as_slice()))
            .mount(&server)
            .await;

        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        let url = format!("{}/model.gguf", server.uri());
        let result = mgr.download(&url, "model.gguf", &sha, None).await;
        assert!(result.is_ok(), "{result:?}");
        assert!(result.unwrap().exists());
    }

    #[tokio::test]
    async fn test_checksum_mismatch_returns_error_and_cleans_partial() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"data"))
            .mount(&server)
            .await;

        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        let url = format!("{}/model.gguf", server.uri());
        let result = mgr
            .download(&url, "model.gguf", "deadbeef00000000", None)
            .await;
        assert!(matches!(
            result,
            Err(DownloadError::ChecksumMismatch { .. })
        ));
        assert!(!dir.path().join("model.gguf.partial").exists());
    }

    #[tokio::test]
    async fn test_network_error_returns_err_not_panic() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        let result = mgr
            .download("http://127.0.0.1:1", "model.gguf", "deadbeef", None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_partial_file_cleaned_on_checksum_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"x"))
            .mount(&server)
            .await;

        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        let url = format!("{}/m.gguf", server.uri());
        let _ = mgr.download(&url, "m.gguf", "bad_sha", None).await;
        assert!(!dir.path().join("m.gguf.partial").exists());
        assert!(!dir.path().join("m.gguf").exists());
    }

    #[tokio::test]
    async fn test_progress_channel_receives_updates() {
        let server = MockServer::start().await;
        let body = b"progress test data";
        let sha = hex::encode(Sha256::digest(body));
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.as_slice()))
            .mount(&server)
            .await;

        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        let url = format!("{}/prog.gguf", server.uri());
        let (tx, mut rx) = mpsc::channel(16);
        mgr.download(&url, "prog.gguf", &sha, Some(tx))
            .await
            .unwrap();

        let mut received = false;
        while rx.try_recv().is_ok() {
            received = true;
        }
        assert!(received);
    }
}
