//! #1211: `/spawn -o <path>` never replaces an existing file unless `--force`,
//! and the check runs before the sub-agent does, so a refusal costs no tokens.

use super::{
    handle_spawn, parse_spawn_args, spawn_output_start_refusal, write_spawn_output, SpawnTracker,
};

/// Where every argv word of a `/spawn` line lands.
fn landing(input: &str) -> (Option<String>, bool, bool, String) {
    let a = parse_spawn_args(input).expect("parses");
    (a.output_path, a.force, a.background, a.task)
}

#[test]
fn every_word_lands_somewhere() {
    // (input, -o path, force, bg, task): no word is dropped.
    let rows: &[(&str, Option<&str>, bool, bool, &str)] = &[
        ("/spawn -o a.md do x", Some("a.md"), false, false, "do x"),
        (
            "/spawn -o a.md --force do x",
            Some("a.md"),
            true,
            false,
            "do x",
        ),
        (
            "/spawn --force -o a.md do x",
            Some("a.md"),
            true,
            false,
            "do x",
        ),
        (
            "/spawn --bg -o a.md --force do x",
            Some("a.md"),
            true,
            true,
            "do x",
        ),
        // A --force after the task text is task text, like every other front flag.
        (
            "/spawn -o a.md do x --force",
            Some("a.md"),
            false,
            false,
            "do x --force",
        ),
        // --force without -o parses; the start refusal names it (below).
        ("/spawn --force do x", None, true, false, "do x"),
    ];
    for (input, path, force, bg, task) in rows {
        let got = landing(input);
        assert_eq!(
            got,
            (path.map(str::to_string), *force, *bg, task.to_string()),
            "{input}"
        );
    }
}

#[test]
fn force_without_output_file_is_refused() {
    for input in ["/spawn --force do x", "/spawn --parallel --force a --- b"] {
        let args = parse_spawn_args(input).unwrap();
        let msg = spawn_output_start_refusal(input, &args).expect(input);
        assert_eq!(msg, "--force only applies to -o <file>. Nothing was run.");
    }
}

#[test]
fn existing_file_is_refused_with_a_remedy_that_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.md");
    std::fs::write(&file, "mine").unwrap();
    let f = file.display().to_string();
    for (input, task, bg) in [
        (format!("/spawn -o {f} do x"), "do x", false),
        (format!("/spawn --bg -o {f} do x"), "do x", true),
        (
            format!("/spawn --model m -o {f} fix the bug"),
            "fix the bug",
            false,
        ),
    ] {
        let args = parse_spawn_args(&input).unwrap();
        let msg = spawn_output_start_refusal(&input, &args).expect(&input);
        let rest = input.strip_prefix("/spawn ").unwrap();
        assert_eq!(
            msg,
            format!(
                "{f} already exists. Nothing was run. To replace it: /spawn --force {rest} (or choose another file name)"
            )
        );
        // Round trip: the printed remedy parses to the same path and task, forced.
        let remedy = msg
            .split("To replace it: ")
            .nth(1)
            .unwrap()
            .strip_suffix(" (or choose another file name)")
            .unwrap();
        let again = parse_spawn_args(remedy).expect(remedy);
        assert_eq!(again.output_path.as_deref(), Some(f.as_str()), "{remedy}");
        assert!(again.force, "{remedy}");
        assert_eq!(again.task, task, "{remedy}");
        assert_eq!(again.background, bg, "{remedy}");
        assert_eq!(spawn_output_start_refusal(remedy, &again), None, "{remedy}");
    }
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
}

#[test]
fn directory_is_refused_without_offering_force() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().display().to_string();
    for input in [
        format!("/spawn -o {d} do x"),
        format!("/spawn -o {d} --force do x"),
    ] {
        let args = parse_spawn_args(&input).unwrap();
        let msg = spawn_output_start_refusal(&input, &args).expect(&input);
        assert_eq!(
            msg,
            format!("{d} is a directory. Nothing was run. Give -o a file name, e.g. -o {d}/out.md")
        );
        assert!(!msg.contains("--force"), "{msg}");
    }
}

#[test]
fn absent_file_and_forced_file_pass_the_start_check() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("new.md");
    let f = file.display().to_string();
    let input = format!("/spawn -o {f} do x");
    assert_eq!(
        spawn_output_start_refusal(&input, &parse_spawn_args(&input).unwrap()),
        None
    );
    std::fs::write(&file, "old").unwrap();
    let input = format!("/spawn -o {f} --force do x");
    assert_eq!(
        spawn_output_start_refusal(&input, &parse_spawn_args(&input).unwrap()),
        None
    );
    // Plain -o with no file at all is unaffected.
    assert_eq!(
        spawn_output_start_refusal("/spawn do x", &parse_spawn_args("/spawn do x").unwrap()),
        None
    );
}

#[tokio::test]
async fn refused_spawn_registers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.md");
    std::fs::write(&file, "mine").unwrap();
    let input = format!("/spawn -o {} do x", file.display());
    let tracker = SpawnTracker::new();
    let mut usage = yoagent::Usage::default();
    let out = handle_spawn(&input, &config(), &mut usage, "m", &[], &tracker).await;
    assert_eq!(out, None);
    assert!(
        tracker.snapshot().is_empty(),
        "a refused spawn was registered"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
}

#[test]
fn write_keeps_a_file_that_appeared_during_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.md");
    let f = file.display().to_string();
    // Absent at the start check, then created while the sub-agent ran.
    std::fs::write(&file, "appeared").unwrap();
    let err = write_spawn_output(&f, "agent output", false).unwrap_err();
    assert_eq!(
        err,
        format!("{f} appeared while the sub-agent ran, so it was not replaced")
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"appeared");
}

#[test]
fn write_creates_a_new_file_and_force_replaces() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.md");
    let f = file.display().to_string();
    write_spawn_output(&f, "first", false).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "first");
    write_spawn_output(&f, "second", true).unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "second");
}

fn config() -> crate::AgentConfig {
    use crate::cli;
    crate::AgentConfig {
        model: "m".to_string(),
        api_key: "test-key".to_string(),
        provider: "anthropic".to_string(),
        // A dead local address: nothing here may reach a real provider.
        base_url: Some("http://127.0.0.1:9".to_string()),
        skills: yoagent::skills::SkillSet::empty(),
        system_prompt: "Test.".to_string(),
        thinking: yoagent::ThinkingLevel::Off,
        max_tokens: None,
        temperature: None,
        max_turns: None,
        auto_approve: true,
        auto_commit: false,
        permissions: cli::PermissionConfig::default(),
        dir_restrictions: cli::DirectoryRestrictions::default(),
        context_strategy: cli::ContextStrategy::default(),
        context_window: None,
        shell_hooks: vec![],
        fallback_provider: None,
        fallback_model: None,
        auto_watch: true,
        allowed_tools: vec![],
        disallowed_tools: vec![],
        no_tools: false,
        lite: false,
        bash_cwd: None,
    }
}

#[test]
fn a_failed_save_keeps_the_result_collectable() {
    let tracker = SpawnTracker::new();
    let id = tracker.register_with_bg("bg task", Some("a.md".into()), true);
    tracker.complete_with_handoff(id, "the answer".into(), None);
    tracker.fail(id, "output NOT saved: a.md appeared".into());
    let t = tracker.snapshot().pop().unwrap();
    assert_eq!(
        t.status.to_string(),
        "failed: output NOT saved: a.md appeared"
    );
    let got = tracker.try_collect(id).unwrap().unwrap();
    assert_eq!((got.0.as_str(), got.1.as_str()), ("bg task", "the answer"));
    // A failure with no result is still an error to collect.
    let id2 = tracker.register_with_bg("other", None, true);
    tracker.fail(id2, "boom".into());
    assert_eq!(
        tracker.try_collect(id2).unwrap_err(),
        format!("spawn #{id2} failed: boom")
    );
}
