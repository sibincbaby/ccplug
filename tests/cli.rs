use assert_cmd::Command;

#[test]
fn help_lists_subcommands() {
    let out = Command::cargo_bin("ccplug")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    for sub in ["list", "status", "enable", "disable"] {
        assert!(text.contains(sub), "help missing `{sub}`");
    }
}
