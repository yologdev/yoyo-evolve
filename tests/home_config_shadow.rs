//! Two user-level config files (`~/.yoyo.toml` and `~/.config/yoyo/config.toml`)
//! warn once on stderr at startup (Day 253). yoyo reads one config file, first
//! found wins, so the XDG file's settings are silently lost; before this,
//! `yoyo -p` and `yoyo version -v` printed nothing about it.
//!
//! Drives the real binary through `yoyo version -v`, which loads config via
//! `config::load_config_file` (the startup site) and calls no model. Never
//! invokes cargo.

use std::path::Path;
use std::process::{Command, Stdio};

const WARNING: &str = "two user-level yoyo config files";

/// Run `yoyo version -v` in `cwd` with HOME at `home` and XDG_CONFIG_HOME
/// removed, so the XDG path is derived from HOME as for most users. Provider
/// keys and model overrides are stripped so the runner's environment cannot
/// change the result. Returns stderr.
fn version_stderr(cwd: &Path, home: &Path) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    for (name, _) in std::env::vars() {
        if name == "API_KEY" || name.ends_with("_API_KEY") {
            cmd.env_remove(name);
        }
    }
    let out = cmd
        .args(["version", "-v"])
        .current_dir(cwd)
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("MODEL")
        .env_remove("YOYO_MODEL")
        .stdin(Stdio::null())
        .output()
        .expect("spawn yoyo");
    assert!(out.status.success(), "yoyo version -v failed: {out:?}");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn home_with(home_file: bool, xdg_file: bool) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    if home_file {
        std::fs::write(home.path().join(".yoyo.toml"), "").unwrap();
    }
    if xdg_file {
        let dir = home.path().join(".config").join("yoyo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "thinking = \"low\"\n").unwrap();
    }
    home
}

#[test]
fn both_home_config_files_warn_exactly_once_naming_both() {
    let proj = tempfile::tempdir().unwrap();
    let home = home_with(true, true);
    let err = version_stderr(proj.path(), home.path());
    assert_eq!(err.matches(WARNING).count(), 1, "stderr: {err}");
    let read = home.path().join(".yoyo.toml");
    let ignored = home.path().join(".config/yoyo/config.toml");
    let line = err.lines().find(|l| l.contains(WARNING)).unwrap();
    assert!(
        line.contains(&format!(
            "{} takes precedence, so {} is ignored",
            read.display(),
            ignored.display()
        )),
        "stderr: {err}"
    );
}

#[test]
fn one_home_config_file_does_not_warn() {
    let proj = tempfile::tempdir().unwrap();
    for (h, x) in [(true, false), (false, true)] {
        let home = home_with(h, x);
        let err = version_stderr(proj.path(), home.path());
        assert!(!err.contains(WARNING), "home={h} xdg={x} stderr: {err}");
    }
}

#[test]
fn project_config_over_one_home_file_is_the_intended_layering_and_silent() {
    let proj = tempfile::tempdir().unwrap();
    std::fs::write(proj.path().join(".yoyo.toml"), "").unwrap();
    let home = home_with(false, true);
    let err = version_stderr(proj.path(), home.path());
    assert!(!err.contains(WARNING), "stderr: {err}");
}
