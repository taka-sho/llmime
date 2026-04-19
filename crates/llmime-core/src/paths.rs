use std::path::PathBuf;

pub struct LlmimePaths {
    pub data_dir: PathBuf,
    pub models_dir: PathBuf,
    pub mozc_dir: PathBuf,
    pub db_path: PathBuf,
    pub user_dict_path: PathBuf,
    pub config_dir: PathBuf,
}

impl LlmimePaths {
    /// 実行時パス解決。LLMIME_DATA_DIR 環境変数で override 可。
    pub fn resolve() -> Self {
        let base = std::env::var_os("LLMIME_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::data_dir()
                    .expect("Cannot resolve data dir")
                    .join("llmime")
            });
        Self {
            models_dir: base.join("models"),
            mozc_dir: base.join("vendor").join("mozc_oss"),
            db_path: base.join("llmime.db"),
            user_dict_path: base.join("user_dict.sqlite"),
            config_dir: dirs::config_dir()
                .expect("Cannot resolve config dir")
                .join("llmime"),
            data_dir: base,
        }
    }

    /// 開発時（リポジトリ直下）パス。テスト・CLI --dev フラグ用。
    pub fn dev_paths(repo_root: PathBuf) -> Self {
        Self {
            models_dir: repo_root.join("models"),
            mozc_dir: repo_root.join("vendor").join("mozc_oss"),
            db_path: repo_root.join("llmime.db"),
            user_dict_path: repo_root.join("user_dict.sqlite"),
            config_dir: repo_root.join("config"),
            data_dir: repo_root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_uses_env_override() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("LLMIME_DATA_DIR", "/tmp/llmime_test");
        let paths = LlmimePaths::resolve();
        std::env::remove_var("LLMIME_DATA_DIR");
        assert_eq!(paths.data_dir, Path::new("/tmp/llmime_test"));
    }

    #[test]
    fn resolve_models_dir_is_subdir_of_data() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("LLMIME_DATA_DIR", "/tmp/llmime_models_test");
        let paths = LlmimePaths::resolve();
        std::env::remove_var("LLMIME_DATA_DIR");
        assert_eq!(
            paths.models_dir,
            Path::new("/tmp/llmime_models_test/models")
        );
    }

    #[test]
    fn resolve_mozc_dir_is_correct_subpath() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("LLMIME_DATA_DIR", "/tmp/llmime_mozc_test");
        let paths = LlmimePaths::resolve();
        std::env::remove_var("LLMIME_DATA_DIR");
        assert_eq!(
            paths.mozc_dir,
            Path::new("/tmp/llmime_mozc_test/vendor/mozc_oss")
        );
    }

    #[test]
    fn resolve_db_path_is_correct() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("LLMIME_DATA_DIR", "/tmp/llmime_db_test");
        let paths = LlmimePaths::resolve();
        std::env::remove_var("LLMIME_DATA_DIR");
        assert_eq!(paths.db_path, Path::new("/tmp/llmime_db_test/llmime.db"));
    }

    #[test]
    fn dev_paths_uses_repo_root() {
        let root = PathBuf::from("/workspace/llmime");
        let paths = LlmimePaths::dev_paths(root.clone());
        assert_eq!(paths.data_dir, root);
        assert_eq!(paths.models_dir, root.join("models"));
        assert_eq!(paths.mozc_dir, root.join("vendor").join("mozc_oss"));
        assert_eq!(paths.db_path, root.join("llmime.db"));
        assert_eq!(paths.config_dir, root.join("config"));
    }
}
