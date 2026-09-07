//! A **repository** must not be able to make git execute a program on our host.
//!
//! ## The mechanism (measured, not inferred)
//!
//! git reads `core.fsmonitor` from the repository's own `.git/config` and
//! **executes** the program it names on any operation that refreshes the index.
//! A repo that arrives as *files with `.git` intact* — a zip, a shared drive, a
//! sync folder; anything but a `git clone`, which does not copy local config —
//! therefore runs code the moment an agent gathers startup context.
//!
//! Disclosed 2026-09-02 (Manifold Security, "The git you didn't run"): 8 findings
//! across 7 agents, Goose CVE-2026-72718 at CVSS 7.0. Reproduced here on
//! **git 2.55.0** before any fix was written: a `.git/config` carrying
//! `[core] fsmonitor = touch <sentinel>` creates the sentinel under
//! `git status --porcelain`, `git ls-files` **and** `git diff --cached` — the exact
//! three commands `src/context.rs` runs on every prompt (`:49`, `:79`, `:84`).
//!
//! yoyo's five-gate trust boundary does not help and cannot: `--trust-project`,
//! the trust store and the trust prompt gate *agent-layer* config (MCP servers,
//! `permissions.allow`, shell hooks, `goal_verify`, `notify_command`). Git runs
//! underneath all of it, before any prompt, and it is not a model-generated tool
//! call, so the permission layer never sees it.
//!
//! ## Why this file drives the BINARY rather than calling the function
//!
//! `yoyo-agent` is a binary-only crate, so a `tests/*.rs` integration test cannot
//! `use` `git::run_git_in_dir`. It could have gone in `src/git.rs`'s own
//! `#[cfg(test)]` module — but that file sits **31 lines past
//! `MAX_MODULE_LINES = 2000`** inside the 50-line grace band, and a scratch-repo
//! fixture would push it past the fatal boundary, reverting the whole task
//! including the security fix it is testing.
//!
//! Driving `CARGO_BIN_EXE_yoyo` is the stronger test anyway: it exercises the
//! **real** chokepoint through the **real** binary in a **real** hostile repo,
//! rather than a copy of the argv assembled by the test. `yoyo tree` is used
//! because it shells `git ls-files` (one of the three reproduced commands),
//! needs no API key, and is deterministic. It spawns the already-built test
//! binary and **never** `cargo` (#832: a nested cargo rebuilds over the shared
//! `target/debug/yoyo` path every `CARGO_BIN_EXE_*` consumer resolves to).
//!
//! ## Stated limit
//!
//! This is a **named-key** defence. `core.fsmonitor` is neutralised;
//! `core.sshCommand`, `core.pager`, `core.editor`, `core.askPass`,
//! `credential.helper`, `diff.external`, `*.textconv`, `alias.*`,
//! `core.hooksPath` and `uploadpack.packObjectsHook` are **not**. "Could not
//! check" must not read as "checked; clean".

use std::path::Path;
use std::process::Command;

/// Build a scratch git repo in a tempdir. **Never this repo** — `run_git`'s
/// `#[cfg(test)]` destructive guard exists because tests once mutated the live
/// checkout, and #780 spent two whole tasks removing tests that moved the
/// process CWD. Every `git` here is spawned directly and with `-C`, which is
/// what a test-region site is *supposed* to do.
fn scratch_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "probe@example.com"],
        vec!["config", "user.name", "Probe"],
    ] {
        let out = Command::new("git")
            .arg("-C")
            .arg(p)
            .args(&args)
            .output()
            .expect("spawn git");
        assert!(out.status.success(), "git {:?} failed", args);
    }
    std::fs::write(p.join("a.txt"), "hello\n").expect("write a.txt");
    for args in [vec!["add", "a.txt"], vec!["commit", "-q", "-m", "init"]] {
        let out = Command::new("git")
            .arg("-C")
            .arg(p)
            .args(&args)
            .output()
            .expect("spawn git");
        assert!(out.status.success(), "git {:?} failed", args);
    }
    dir
}

/// Append the hostile `core.fsmonitor` stanza and return the sentinel path the
/// helper would create. The helper is `touch`, so the sentinel's *existence* is
/// the whole signal — git also passes the fsmonitor protocol version and token
/// as arguments, so `touch` creates extra files beside it; that is noise, and
/// only the named path is asserted on.
fn arm_hostile_fsmonitor(repo: &Path) -> std::path::PathBuf {
    let sentinel = repo.join("SENTINEL");
    let stanza = format!("[core]\n\tfsmonitor = touch {}\n", sentinel.display());
    let cfg = repo.join(".git").join("config");
    let mut text = std::fs::read_to_string(&cfg).expect("read .git/config");
    text.push_str(&stanza);
    std::fs::write(&cfg, text).expect("write .git/config");
    sentinel
}

/// Run the yoyo binary with its process CWD inside `repo`.
fn yoyo_in(repo: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .current_dir(repo)
        .args(args)
        .output()
        .expect("spawn yoyo")
}

#[test]
fn a_repository_cannot_execute_code_through_core_fsmonitor() {
    let repo = scratch_repo();
    let p = repo.path();
    let sentinel = arm_hostile_fsmonitor(p);

    // ---- ANTI-VACUOUS, ASSERTED FIRST ----------------------------------
    // A fixture that silently failed to arm would pass this test by having
    // nothing to trigger. Two checks, because either alone can lie: the config
    // really names the sentinel, AND a *plain* git in this same repo really
    // does execute it. Without the second, a git version that ignored
    // `core.fsmonitor` entirely would render this whole file vacuous green.
    let cfg = std::fs::read_to_string(p.join(".git").join("config")).expect("read config");
    assert!(
        cfg.contains("fsmonitor") && cfg.contains(&sentinel.display().to_string()),
        "fixture did not arm: .git/config does not name the sentinel:\n{cfg}"
    );
    let _ = std::fs::remove_file(&sentinel);
    let out = Command::new("git")
        .arg("-C")
        .arg(p)
        .args(["ls-files"])
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "control git ls-files failed");
    assert!(
        sentinel.exists(),
        "CONTROL FAILED: plain `git ls-files` did not run the fsmonitor helper in this \
         repo, so this test cannot prove anything. Either the fixture is broken or this \
         git version does not honour core.fsmonitor — say which before trusting a pass."
    );

    // ---- THE PROPERTY ---------------------------------------------------
    std::fs::remove_file(&sentinel).expect("clear sentinel");
    let out = yoyo_in(p, &["tree"]);
    assert!(
        !sentinel.exists(),
        "SECURITY: a repository executed code on the host. `yoyo tree` ran \
         `git ls-files` in a repo whose .git/config names a core.fsmonitor helper, \
         and the helper ran. The chokepoint in src/git.rs must inject \
         `-c core.fsmonitor=`. stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn an_ordinary_repository_is_byte_identical() {
    // The near-miss guard, and the entire regression surface: every user whose
    // repo carries no hostile config. Asserted with `assert_eq!` on the whole
    // stdout rather than a `contains`, because a discriminator tested only on
    // the side that fires is vacuous green — a fix that broke `git ls-files`
    // outright would pass the security test above and fail nobody else's.
    let repo = scratch_repo();
    let out = yoyo_in(repo.path(), &["tree"]);
    assert!(
        out.status.success(),
        "yoyo tree failed on an ordinary repo: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "a.txt",
        "`yoyo tree` output moved for an ordinary repo — the chokepoint's new \
         global changed behaviour for someone who was never under attack"
    );
}

#[test]
fn the_hostile_config_does_not_break_ordinary_output() {
    // Third direction, and it is not the same claim as either test above:
    // neutralising the helper must still return the *right answer*. A fix that
    // suppressed the helper by making git fail would satisfy "sentinel absent"
    // while silently blinding every consumer of the file list.
    let repo = scratch_repo();
    let p = repo.path();
    arm_hostile_fsmonitor(p);
    let out = yoyo_in(p, &["tree"]);
    assert!(
        out.status.success(),
        "yoyo tree failed on a repo with a hostile fsmonitor: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "a.txt",
        "neutralising core.fsmonitor changed what `git ls-files` reports"
    );
}
