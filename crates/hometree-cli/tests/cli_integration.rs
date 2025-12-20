use assert_cmd::prelude::*;
use hometree_core::read_generations;
use predicates::str::contains;
use predicates::Predicate;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const TEST_HOST: &str = "hometree-host";
const TEST_USER: &str = "hometree-user";

fn base_env(temp: &TempDir) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let home = temp.path().join("home");
    let config = temp.path().join("config");
    let data = temp.path().join("data");
    let state = temp.path().join("state");
    fs::create_dir_all(&home).unwrap();
    (home, config, data, state)
}

fn cmd(temp: &TempDir) -> Command {
    let (home, config, data, state) = base_env(temp);
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("hometree"));
    cmd.env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_STATE_HOME", &state)
        .env("GIT_AUTHOR_NAME", "hometree")
        .env("GIT_AUTHOR_EMAIL", "hometree@example.com")
        .env("GIT_COMMITTER_NAME", "hometree")
        .env("GIT_COMMITTER_EMAIL", "hometree@example.com")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("HOSTNAME", TEST_HOST)
        .env("USER", TEST_USER);
    cmd
}

fn repo_dir(data: &Path) -> PathBuf {
    data.join("hometree/repo.git")
}

fn state_dir(state: &Path) -> PathBuf {
    state.join("hometree")
}

fn git_rev(repo: &Path, work_tree: &Path, spec: &str) -> String {
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(repo)
        .arg("--work-tree")
        .arg(work_tree)
        .arg("rev-parse")
        .arg(spec)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn init_creates_xdg_layout() {
    let temp = TempDir::new().unwrap();
    let (_home, config, data, state) = base_env(&temp);

    cmd(&temp).arg("init").assert().success();

    assert!(config.join("hometree/config.toml").exists());
    assert!(data.join("hometree/repo.git").exists());
    assert!(state.join("hometree").exists());
}

#[test]
fn init_track_snapshot_and_deploy_records_generation() {
    let temp = TempDir::new().unwrap();
    let (home, _config, data, state) = base_env(&temp);

    let config_file = home.join(".config/app/config.toml");
    fs::create_dir_all(config_file.parent().unwrap()).unwrap();
    fs::write(&config_file, "v1").unwrap();

    cmd(&temp).arg("init").assert().success();
    cmd(&temp)
        .args(["track", config_file.to_string_lossy().as_ref()])
        .assert()
        .success();
    cmd(&temp)
        .args(["snapshot", "-m", "first"])
        .assert()
        .success();

    cmd(&temp).args(["deploy", "HEAD"]).assert().success();

    let entries = read_generations(&state_dir(&state)).unwrap();
    assert_eq!(entries.len(), 1);

    let head = git_rev(&repo_dir(&data), &home, "HEAD");
    let entry = &entries[0];
    assert_eq!(entry.rev, head);
    assert_eq!(entry.host, TEST_HOST);
    assert_eq!(entry.user, TEST_USER);
    assert!(entry.message.is_none());
}

#[test]
fn rollback_replays_previous_generation() {
    let temp = TempDir::new().unwrap();
    let (home, _config, data, state) = base_env(&temp);
    let file_path = home.join(".config/app/config.toml");
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(&file_path, "first").unwrap();

    cmd(&temp).arg("init").assert().success();
    cmd(&temp)
        .args(["track", file_path.to_string_lossy().as_ref()])
        .assert()
        .success();
    cmd(&temp)
        .args(["snapshot", "-m", "first"])
        .assert()
        .success();

    fs::write(&file_path, "second").unwrap();
    cmd(&temp)
        .args(["track", file_path.to_string_lossy().as_ref()])
        .assert()
        .success();
    cmd(&temp)
        .args(["snapshot", "-m", "second"])
        .assert()
        .success();

    let repo = repo_dir(&data);
    let head = git_rev(&repo, &home, "HEAD");
    let first_rev = git_rev(&repo, &home, "HEAD~1");

    cmd(&temp).args(["deploy", &head]).assert().success();
    cmd(&temp).args(["deploy", &first_rev]).assert().success();

    let state_dir = state_dir(&state);
    let entries = read_generations(&state_dir).unwrap();
    assert_eq!(entries.len(), 2);

    cmd(&temp)
        .args(["rollback", "--steps", "1"])
        .assert()
        .success();

    let contents = fs::read_to_string(&file_path).unwrap();
    assert_eq!(contents, "second");

    let updated = read_generations(&state_dir).unwrap();
    assert_eq!(updated.len(), 3);
    assert_eq!(updated.last().unwrap().rev, head);
}

#[test]
fn deploy_and_rollback_flow() {
    let temp = TempDir::new().unwrap();
    let (home, _config, _data, state) = base_env(&temp);

    let file_path = home.join(".config/app/config.toml");
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(&file_path, "v1").unwrap();

    cmd(&temp).arg("init").assert().success();

    cmd(&temp)
        .args(["track", file_path.to_string_lossy().as_ref()])
        .assert()
        .success();

    cmd(&temp)
        .args(["snapshot", "-m", "first"])
        .assert()
        .success();

    fs::write(&file_path, "v2").unwrap();
    cmd(&temp)
        .args(["track", file_path.to_string_lossy().as_ref()])
        .assert()
        .success();
    cmd(&temp)
        .args(["snapshot", "-m", "second"])
        .assert()
        .success();

    cmd(&temp).args(["deploy", "HEAD"]).assert().success();
    cmd(&temp).args(["deploy", "HEAD~1"]).assert().success();

    let gens = state.join("hometree/generations.jsonl");
    let lines = fs::read_to_string(&gens).unwrap();
    assert_eq!(lines.lines().count(), 2);

    cmd(&temp)
        .args(["rollback", "--steps", "1"])
        .assert()
        .success();

    let contents = fs::read_to_string(&file_path).unwrap();
    assert_eq!(contents, "v2");
}

#[test]
fn untrack_removes_from_extra_files() {
    let temp = TempDir::new().unwrap();
    let (home, config, _data, _state) = base_env(&temp);

    let dotfile = home.join(".gitconfig");
    fs::write(&dotfile, "ok").unwrap();

    cmd(&temp).arg("init").assert().success();
    cmd(&temp)
        .args([
            "track",
            dotfile.to_string_lossy().as_ref(),
            "--allow-outside",
        ])
        .assert()
        .success();

    cmd(&temp)
        .args(["untrack", dotfile.to_string_lossy().as_ref()])
        .assert()
        .success();

    let cfg = fs::read_to_string(config.join("hometree/config.toml")).unwrap();
    assert!(contains("extra_files = []").eval(&cfg));
}
