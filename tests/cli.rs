use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;

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

/// Build a fake ~/.claude with one user-scope plugin owning one skill.
fn fake_home(home: &Path) {
    let install = home.join("install/demo/1.0.0");
    fs::create_dir_all(install.join("skills/foo")).unwrap();
    fs::write(
        install.join("skills/foo/SKILL.md"),
        "---\nname: foo\ndescription: a skill\n---\n",
    )
    .unwrap();
    fs::create_dir_all(install.join("agents")).unwrap();

    let pdir = home.join(".claude/plugins");
    fs::create_dir_all(&pdir).unwrap();
    let json = serde_json::json!({
        "version": 2,
        "plugins": {
            "demo@mkt": [
                {"scope": "user", "installPath": install.to_str().unwrap(), "version": "1.0.0"}
            ]
        }
    });
    fs::write(pdir.join("installed_plugins.json"), json.to_string()).unwrap();
}

fn run(home: &Path, project: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("ccplug").unwrap();
    cmd.args(args)
        .arg("--home-dir")
        .arg(home)
        .arg("--project-dir")
        .arg(project);
    cmd.assert()
}

#[test]
fn enable_writes_project_settings() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    fake_home(home.path());

    let out = run(home.path(), project.path(), &["enable", "demo", "--json"]).success();
    let v: Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["results"][0]["target"], "demo");
    assert_eq!(v["results"][0]["type"], "plugin");
    assert_eq!(v["results"][0]["action"], "enabled");
    assert_eq!(v["results"][0]["ok"], true);

    let settings: Value = serde_json::from_str(
        &fs::read_to_string(project.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["enabledPlugins"]["demo@mkt"], true);
}

#[test]
fn mixed_batch_continues_and_marks_unsupported() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    fake_home(home.path());

    // demo (plugin, ok) + demo:foo (plugin-owned skill, unsupported-v1)
    let out = run(
        home.path(),
        project.path(),
        &["disable", "demo", "demo:foo", "--json"],
    )
    .success(); // exit 0 because at least one target succeeded
    let v: Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);

    let plugin = results.iter().find(|r| r["target"] == "demo").unwrap();
    assert_eq!(plugin["ok"], true);
    assert_eq!(plugin["action"], "disabled");

    let skill = results.iter().find(|r| r["target"] == "demo:foo").unwrap();
    assert_eq!(skill["ok"], false);
    assert_eq!(skill["type"], "plugin-skill");
    assert_eq!(skill["reason"], "unsupported-v1");

    // disabling demo warns about its agents
    let warnings = v["warnings"].as_array().unwrap();
    assert!(warnings
        .iter()
        .any(|w| w.as_str().unwrap().contains("agents")));
}

#[test]
fn all_failed_exits_nonzero() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    fake_home(home.path());
    run(home.path(), project.path(), &["enable", "nope", "--json"]).code(2);
}

#[test]
fn dry_run_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    fake_home(home.path());
    run(
        home.path(),
        project.path(),
        &["enable", "demo", "--dry-run", "--json"],
    )
    .success();
    assert!(!project.path().join(".claude/settings.json").exists());
}

#[test]
fn stdin_json_array_targets() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    fake_home(home.path());

    let mut cmd = Command::cargo_bin("ccplug").unwrap();
    cmd.args(["enable", "--stdin", "--json"])
        .arg("--home-dir")
        .arg(home.path())
        .arg("--project-dir")
        .arg(project.path())
        .write_stdin(r#"["demo"]"#);
    let out = cmd.assert().success();
    let v: Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["results"][0]["target"], "demo");
}
