use assert_cmd::Command;

#[test]
fn version_exits_zero() {
    let mut cmd = Command::cargo_bin("llmime").unwrap();
    cmd.arg("version").assert().success();
}

#[test]
fn convert_outputs_todo() {
    let mut cmd = Command::cargo_bin("llmime").unwrap();
    cmd.args(["convert", "こんにちは"])
        .assert()
        .success()
        .stdout(predicates::str::contains("TODO:"));
}

#[test]
fn convert_top_k_flag_parsed() {
    let mut cmd = Command::cargo_bin("llmime").unwrap();
    cmd.args(["convert", "--top-k", "3", "てすと"])
        .assert()
        .success();
}

#[test]
fn convert_format_flag_parsed() {
    let mut cmd = Command::cargo_bin("llmime").unwrap();
    cmd.args(["convert", "--format", "json", "てすと"])
        .assert()
        .success();
}
