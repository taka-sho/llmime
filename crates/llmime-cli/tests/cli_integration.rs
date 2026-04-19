use assert_cmd::Command;

#[test]
fn version_exits_zero() {
    let mut cmd = Command::cargo_bin("llmime").unwrap();
    cmd.arg("version").assert().success();
}

#[test]
fn convert_requires_model_arg() {
    let mut cmd = Command::cargo_bin("llmime").unwrap();
    cmd.args(["convert", "かんしん"])
        .env_remove("LLMIME_MODEL")
        .env_remove("LLMIME_DICT")
        .assert()
        .failure();
}

#[test]
fn convert_top_k_flag_accepted() {
    // Parses --top-k, then fails because model file doesn't exist at default path
    let mut cmd = Command::cargo_bin("llmime").unwrap();
    cmd.args(["convert", "--top-k", "3", "てすと"])
        .env("LLMIME_DATA_DIR", "/tmp/llmime_nonexistent_test_path")
        .env_remove("LLMIME_MODEL")
        .env_remove("LLMIME_DICT")
        .assert()
        .failure()
        .stderr(predicates::str::contains("model file not found"));
}

#[test]
fn convert_missing_model_shows_helpful_error() {
    let mut cmd = Command::cargo_bin("llmime").unwrap();
    cmd.args(["convert", "てすと"])
        .env("LLMIME_DATA_DIR", "/tmp/llmime_nonexistent_test_path")
        .env_remove("LLMIME_MODEL")
        .env_remove("LLMIME_DICT")
        .assert()
        .failure()
        .stderr(predicates::str::contains("model file not found"));
}
