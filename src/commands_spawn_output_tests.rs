//! #1211: `/spawn -o <path>` never replaces an existing file unless `--force`,
//! and the check runs before the sub-agent does, so a refusal costs no tokens.

use super::{
    handle_spawn, parse_spawn_args, read_spawn_args, spawn_output_start_refusal,
    write_spawn_output, SpawnTracker,
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

/// #1213: where every word of a `-o` / `--system` line lands when a value is
/// quoted. (input, -o path, --system prompt, task)
#[test]
fn a_quoted_output_path_is_one_path() {
    type Row<'a> = (&'a str, Option<&'a str>, Option<&'a str>, &'a str);
    let rows: &[Row] = &[
        (
            r#"/spawn -o "out/a b.md" do x"#,
            Some("out/a b.md"),
            None,
            "do x",
        ),
        (
            "/spawn -o 'out/a b.md' do x",
            Some("out/a b.md"),
            None,
            "do x",
        ),
        // Unquoted: unchanged from before #1213.
        ("/spawn -o out/a.md do x", Some("out/a.md"), None, "do x"),
        // Whitespace inside the quotes is kept as typed.
        (r#"/spawn -o "a  b.md" do x"#, Some("a  b.md"), None, "do x"),
        // Text glued to the close quote is part of the same value.
        (
            r#"/spawn -o "out/a b".md do x"#,
            Some("out/a b.md"),
            None,
            "do x",
        ),
        // A quote inside a bare word is just a character, as before.
        ("/spawn -o don't.md do x", Some("don't.md"), None, "do x"),
        // Both quoted flags on one line, in either order.
        (
            r#"/spawn --system "Be brief" -o "out/a b.md" do x"#,
            Some("out/a b.md"),
            Some("Be brief"),
            "do x",
        ),
        (
            r#"/spawn -o "out/a b.md" --system 'Be brief' do x"#,
            Some("out/a b.md"),
            Some("Be brief"),
            "do x",
        ),
        // The task text after the flags is tokenised exactly as before.
        (
            r#"/spawn -o x.md say "hi there""#,
            Some("x.md"),
            None,
            r#"say "hi there""#,
        ),
        (
            r#"/spawn -o "a b.md" say   "hi   there""#,
            Some("a b.md"),
            None,
            r#"say "hi there""#,
        ),
        // `-o` with nothing after it is task text, as before.
        ("/spawn -o", None, None, "-o"),
    ];
    for (input, path, system, task) in rows {
        let a = parse_spawn_args(input).expect(input);
        assert_eq!(
            (
                a.output_path.as_deref(),
                a.system_prompt.as_deref(),
                a.task.as_str()
            ),
            (*path, *system, *task),
            "{input}"
        );
    }
}

/// #1213: an opening quote with no close is refused, naming the flag and the
/// value, and the example it offers parses back to one value.
#[test]
fn an_unclosed_quote_is_refused_by_name() {
    let rows = [
        (
            r#"/spawn -o "out/a b.md"#,
            r#"-o "out/a b.md: the " is never closed, so yoyo can't tell where the file name ends. Nothing was run. Close the quote, e.g. /spawn -o "my notes.md" <task>"#,
        ),
        (
            "/spawn --bg -o 'out/a b.md do x",
            r#"-o 'out/a b.md do x: the ' is never closed, so yoyo can't tell where the file name ends. Nothing was run. Close the quote, e.g. /spawn -o "my notes.md" <task>"#,
        ),
        (
            r#"/spawn --system "Be brief do x"#,
            r#"--system "Be brief do x: the " is never closed, so yoyo can't tell where the system prompt ends. Nothing was run. Close the quote, e.g. /spawn --system "You are a reviewer" <task>"#,
        ),
    ];
    for (input, want) in rows {
        assert_eq!(
            read_spawn_args(input).err().as_deref(),
            Some(want),
            "{input}"
        );
    }
    // The examples round-trip: each is one value and the task stays the task.
    let a = parse_spawn_args(r#"/spawn -o "my notes.md" do x"#).unwrap();
    assert_eq!(
        (a.output_path.as_deref(), a.task.as_str()),
        (Some("my notes.md"), "do x")
    );
    let a = parse_spawn_args(r#"/spawn --system "You are a reviewer" do x"#).unwrap();
    assert_eq!(
        (a.system_prompt.as_deref(), a.task.as_str()),
        (Some("You are a reviewer"), "do x")
    );
    // An empty quoted -o names no file, so it is refused rather than run.
    assert_eq!(
        read_spawn_args(r#"/spawn -o "" do x"#).err().as_deref(),
        Some(
            r#"-o "" names no file. Nothing was run. Give it a file name, e.g. /spawn -o "my notes.md" <task>"#
        )
    );
}

/// #1213: the refusal comes before anything is registered or written.
#[tokio::test]
async fn an_unclosed_quote_spawns_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().display();
    let input = format!(r#"/spawn -o "{d}/a b.md do x"#);
    let tracker = SpawnTracker::new();
    let mut usage = yoagent::Usage::default();
    let out = handle_spawn(&input, &config(), &mut usage, "m", &[], &tracker).await;
    assert_eq!(out, None);
    assert!(
        tracker.snapshot().is_empty(),
        "an unclosed quote was spawned"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

/// #1213: the directory refusal's example quotes a path with a space, so it
/// parses back to the path it names.
#[test]
fn directory_hint_quotes_a_path_with_a_space() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("my dir");
    std::fs::create_dir(&sub).unwrap();
    let s = sub.display().to_string();
    let input = format!(r#"/spawn -o "{s}" do x"#);
    let args = parse_spawn_args(&input).unwrap();
    assert_eq!(args.output_path.as_deref(), Some(s.as_str()));
    let msg = spawn_output_start_refusal(&input, &args).unwrap();
    assert_eq!(
        msg,
        format!(
            r#"{s} is a directory. Nothing was run. Give -o a file name, e.g. -o "{s}/out.md""#
        )
    );
    let example = msg.split("e.g. ").nth(1).unwrap();
    let again = parse_spawn_args(&format!("/spawn {example} do x")).unwrap();
    assert_eq!(again.output_path, Some(format!("{s}/out.md")));
    assert_eq!(again.task, "do x");
}

/// #1224: `-o` with `--parallel` is refused before anything runs, naming `-o`
/// and the path, in any word order and even with `--force` (so the user never
/// sees the force-without-path refusal for this line). The remedy round-trips:
/// removing the words it names leaves a line that runs the same tasks.
#[test]
fn parallel_with_output_file_is_refused_naming_o() {
    let plain = "-o x.md can't be used with --parallel: the tasks would all need their own file. Nothing was run. Remove -o x.md to run the tasks without saving, or save each one with /spawn -o <file> <task>";
    let forced = "-o x.md can't be used with --parallel: the tasks would all need their own file. Nothing was run. Remove -o x.md and --force to run the tasks without saving, or save each one with /spawn -o <file> <task>";
    let rows: &[(&str, &str, &[&str])] = &[
        ("/spawn --parallel -o x.md a --- b", plain, &["-o", "x.md"]),
        ("/spawn -o x.md --parallel a --- b", plain, &["-o", "x.md"]),
        (
            "/spawn --parallel -o x.md --force a --- b",
            forced,
            &["-o", "x.md", "--force"],
        ),
        (
            "/spawn --force --parallel -o x.md a --- b",
            forced,
            &["-o", "x.md", "--force"],
        ),
    ];
    for (input, want, remove) in rows {
        assert_eq!(
            read_spawn_args(input).err().as_deref(),
            Some(*want),
            "{input}"
        );
        // Round trip: drop exactly the words the remedy names.
        let fixed: Vec<&str> = input
            .split_whitespace()
            .filter(|w| !remove.contains(w))
            .collect();
        let fixed = fixed.join(" ");
        let a = parse_spawn_args(&fixed).expect("the remedy parses");
        assert_eq!(a.output_path, None, "{fixed}");
        assert_eq!(
            a.parallel_tasks,
            Some(vec!["a".to_string(), "b".to_string()]),
            "{fixed}"
        );
        assert_eq!(spawn_output_start_refusal(&fixed, &a), None, "{fixed}");
    }
    // A quoted path with a space is named the way -o reads it back.
    assert_eq!(
        read_spawn_args(r#"/spawn --parallel -o "my notes.md" a --- b"#)
            .err()
            .as_deref(),
        Some(
            r#"-o "my notes.md" can't be used with --parallel: the tasks would all need their own file. Nothing was run. Remove -o "my notes.md" to run the tasks without saving, or save each one with /spawn -o <file> <task>"#
        )
    );
    // Near miss: --parallel without -o still parses as before.
    let a = parse_spawn_args("/spawn --parallel a --- b").unwrap();
    assert_eq!(
        a.parallel_tasks,
        Some(vec!["a".to_string(), "b".to_string()])
    );
    assert_eq!(a.output_path, None);
}
