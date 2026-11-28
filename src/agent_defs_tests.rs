//! Tests for `agent_defs.rs` (#1250 milestone 1). Every directory is a
//! tempdir; the real home is never read.

use super::*;

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

fn parse(text: &str) -> Result<AgentDef, String> {
    parse_agent_file(text, &p("/x/a.md"), AgentScope::Project)
}

#[test]
fn tools_as_comma_string_and_as_yaml_list_parse_the_same() {
    let as_string = parse(
        "---\nname: reviewer\ndescription: Reviews code\ntools: Read, Grep ,Glob\n---\nBody\n",
    )
    .unwrap();
    let as_list = parse(
        "---\nname: reviewer\ndescription: Reviews code\ntools: [Read, Grep, Glob]\n---\nBody\n",
    )
    .unwrap();
    let want = Some(vec!["Read".to_string(), "Grep".into(), "Glob".into()]);
    assert_eq!(as_string.tools, want);
    assert_eq!(as_list.tools, want);
    let block_list = parse(
        "---\nname: r\ndescription: d\ndisallowedTools:\n  - Bash\n  - Write\nmodel: sonnet\nmaxTurns: 7\n---\n",
    )
    .unwrap();
    assert_eq!(
        block_list.disallowed_tools,
        Some(vec!["Bash".to_string(), "Write".into()])
    );
    assert_eq!(block_list.model.as_deref(), Some("sonnet"));
    assert_eq!(block_list.max_turns, Some(7));
    assert_eq!(block_list.tools, None);
}

#[test]
fn every_skip_reason_has_its_own_row() {
    let rows: &[(&str, &str)] = &[
        ("name: a\n", "first line is not `---` (no YAML frontmatter)"),
        ("", "first line is not `---` (no YAML frontmatter)"),
        (
            "---\nname: a\ndescription: d\n",
            "frontmatter is not closed (no second `---` line)",
        ),
        ("---\ndescription: d\n---\n", "no `name`"),
        ("---\nname: \"\"\ndescription: d\n---\n", "`name` is empty"),
        (
            "---\nname: \"   \"\ndescription: d\n---\n",
            "`name` is empty",
        ),
        (
            "---\nname: \"-x\"\ndescription: d\n---\n",
            "`name` \"-x\" starts with `-`",
        ),
        (
            "---\nname: \"a:b\"\ndescription: d\n---\n",
            "`name` \"a:b\" contains `:`, which Claude Code reserves for plugin agents",
        ),
        ("---\nname: a\n---\n", "`name` \"a\" has no `description`"),
        (
            "---\nname: a\ndescription: \"\"\n---\n",
            "`name` \"a\" has no `description`",
        ),
        (
            "---\nname: 5\ndescription: d\n---\n",
            "`name` must be a string",
        ),
        (
            "---\nname: a\ndescription: d\ntools: 3\n---\n",
            "`tools` must be a comma-separated string or a list",
        ),
        (
            "---\nname: a\ndescription: d\nmaxTurns: -1\n---\n",
            "`maxTurns` -1 is not a non-negative whole number",
        ),
        (
            "---\n- just\n- a list\n---\n",
            "frontmatter is not a `key: value` mapping",
        ),
    ];
    for (text, want) in rows {
        assert_eq!(parse(text).unwrap_err(), *want, "input: {text:?}");
    }
    let long = format!("---\nname: {}\ndescription: d\n---\n", "n".repeat(257));
    assert_eq!(
        parse(&long).unwrap_err(),
        "`name` is 257 characters (the limit is 256)"
    );
    // Near-miss that must pass: exactly 256 characters.
    let ok = format!("---\nname: {}\ndescription: d\n---\n", "n".repeat(256));
    assert!(parse(&ok).is_ok());
    // Near-miss next to the `:` row: `-` and `_` inside a name still load.
    assert_eq!(
        parse("---\nname: code-review_bot\ndescription: d\n---\n")
            .unwrap()
            .name,
        "code-review_bot"
    );
}

#[test]
fn bad_yaml_quotes_the_parser_with_file_line_numbers() {
    // Line 3 of the file holds the broken flow sequence.
    let err = parse("---\nname: a\ntools: [Read, Grep\ndescription: d\n---\n").unwrap_err();
    assert!(
        err.starts_with("frontmatter is not valid YAML: "),
        "got {err}"
    );
    assert!(err.contains("line "), "parser location missing: {err}");
    // A tab-indented key on file line 3 is a classic YAML error.
    let err = parse("---\nname: a\n\tdescription: d\n---\n").unwrap_err();
    assert!(err.contains("line 3"), "want the file line (3): {err}");
}

#[test]
fn unknown_keys_are_kept_and_reported() {
    let def =
        parse("---\nname: a\ndescription: d\ncolor: blue\npermissionMode: plan\n---\n").unwrap();
    assert_eq!(def.ignored_keys, vec!["color", "permissionMode"]);
}

#[test]
fn multibyte_body_and_crlf_survive() {
    let def = parse("---\r\nname: a\r\ndescription: ✓ checks\r\n---\r\n✓ done — ok\r\n").unwrap();
    assert_eq!(def.body, "✓ done — ok");
    assert_eq!(def.description, "✓ checks");
    // A BOM before the opening fence is not a missing fence.
    assert!(parse("\u{feff}---\nname: a\ndescription: d\n---\n").is_ok());
}

#[test]
fn render_pins_a_whole_valid_entry_and_a_whole_skipped_line() {
    let def = parse_agent_file(
        "---\nname: code-reviewer\ndescription: Reviews diffs\ntools: Read, Grep\nmodel: sonnet\nmaxTurns: 5\ncolor: red\n---\nYou review.\n",
        &p("/proj/.claude/agents/code-reviewer.md"),
        AgentScope::Project,
    )
    .unwrap();
    assert_eq!(
        render_agent(&def),
        "  code-reviewer  [project] /proj/.claude/agents/code-reviewer.md\n    Reviews diffs\n    tools: read_file, search · model: sonnet · maxTurns: 5\n    ignored keys: color\n"
    );
    let skip = Skipped {
        path: p("/proj/.claude/agents/broken.md"),
        reason: "frontmatter is not closed (no second `---` line)".into(),
    };
    assert_eq!(
        render_skipped(&skip),
        "  /proj/.claude/agents/broken.md: frontmatter is not closed (no second `---` line)\n"
    );
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

fn agent(name: &str, desc: &str) -> String {
    format!("---\nname: {name}\ndescription: {desc}\n---\nbody\n")
}

#[test]
fn project_beats_user_and_yoyo_beats_claude() {
    let proj = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let py = write(
        proj.path(),
        ".yoyo/agents/r.md",
        &agent("rev", "project yoyo"),
    );
    let pc = write(
        proj.path(),
        ".claude/agents/r.md",
        &agent("rev", "project claude"),
    );
    let hc = write(
        home.path(),
        ".claude/agents/r.md",
        &agent("rev", "user claude"),
    );
    let only_user = write(
        home.path(),
        ".yoyo/agents/u.md",
        &agent("solo", "user only"),
    );
    write(proj.path(), ".claude/agents/notes.txt", "ignored silently");
    let broken = write(proj.path(), ".claude/agents/broken.md", "---\nname: b\n");
    // Recursive since Day 272 (inverted from "a nested file is not loaded"):
    // a file in a subfolder is found, with the same scope as a top-level one.
    let deep = write(
        proj.path(),
        ".claude/agents/sub/deep.md",
        &agent("deep", "nested"),
    );

    let d = discover_agent_defs(proj.path(), Some(home.path()));
    let names: Vec<(&str, &Path)> = d
        .agents
        .iter()
        .map(|a| (a.name.as_str(), a.path.as_path()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("rev", py.as_path()),
            ("deep", deep.as_path()),
            ("solo", only_user.as_path())
        ]
    );
    assert_eq!(d.agents[1].scope, AgentScope::Project);
    assert_eq!(d.agents[2].scope, AgentScope::User);
    let shadowed: Vec<(&Path, &Path)> = d
        .shadowed
        .iter()
        .map(|s| (s.def.path.as_path(), s.by.as_path()))
        .collect();
    assert_eq!(
        shadowed,
        vec![(pc.as_path(), py.as_path()), (hc.as_path(), py.as_path())]
    );
    assert_eq!(
        d.skipped,
        vec![Skipped {
            path: broken,
            reason: "frontmatter is not closed (no second `---` line)".into()
        }]
    );
    assert_eq!(d.searched.len(), 4);

    let listing = render_agents_listing(&d);
    assert!(listing.starts_with(
        "Named sub-agents (preview: discovered and validated, not yet used by yoyo)\n"
    ));
    assert!(listing.contains(&format!(
        "  rev  {}  shadowed by {}\n",
        pc.display(),
        py.display()
    )));
}

#[test]
fn nothing_found_names_all_four_dirs() {
    let proj = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let d = discover_agent_defs(proj.path(), Some(home.path()));
    let want = format!(
        "No named sub-agent files found.\nLooked in (closest first, subfolders up to 8 levels deep):\n  {}\n  {}\n  {}\n  {}\n  (no `.git` in the cwd or above it: parent directories not searched)\n",
        proj.path().join(".yoyo/agents").display(),
        proj.path().join(".claude/agents").display(),
        home.path().join(".yoyo/agents").display(),
        home.path().join(".claude/agents").display(),
    );
    assert_eq!(render_agents_listing(&d), want);
    let no_home = discover_agent_defs(proj.path(), None);
    assert!(render_agents_listing(&no_home)
        .ends_with("  (no home directory: ~/.yoyo/agents and ~/.claude/agents not searched)\n"));
}

#[test]
fn running_from_home_does_not_shadow_a_file_with_itself() {
    let home = tempfile::tempdir().unwrap();
    write(home.path(), ".claude/agents/r.md", &agent("rev", "d"));
    let d = discover_agent_defs(home.path(), Some(home.path()));
    assert_eq!(d.agents.len(), 1);
    assert_eq!(d.agents[0].scope, AgentScope::Project);
    assert!(d.shadowed.is_empty());
}

/// A tempdir with `.git` at its root and `a/b` below it. Asserts the tempdir
/// itself is not inside some other repo, so "no `.git`" rows mean what they say.
fn repo_with_subdirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    assert!(
        !root.path().ancestors().any(|a| a.join(".git").exists()),
        "the temp dir {} is inside a git repo; these rows need one outside",
        root.path().display()
    );
    std::fs::create_dir(root.path().join(".git")).unwrap();
    let a = root.path().join("a");
    let b = a.join("b");
    std::fs::create_dir_all(&b).unwrap();
    (root, a, b)
}

#[test]
fn walk_up_finds_a_repo_root_agent_from_a_subfolder() {
    let (root, _a, b) = repo_with_subdirs();
    let home = tempfile::tempdir().unwrap();
    let file = write(root.path(), ".claude/agents/r.md", &agent("rev", "root"));
    let d = discover_agent_defs(&b, Some(home.path()));
    let found: Vec<(&str, &Path, AgentScope)> = d
        .agents
        .iter()
        .map(|x| (x.name.as_str(), x.path.as_path(), x.scope))
        .collect();
    assert_eq!(found, vec![("rev", file.as_path(), AgentScope::Project)]);
    assert_eq!(d.repo_root.as_deref(), Some(root.path()));
    // b, a, root: two dirs each, closest first; then the two under home.
    let mut want = Vec::new();
    for dir in [
        b.clone(),
        b.parent().unwrap().to_path_buf(),
        root.path().into(),
    ] {
        want.push(dir.join(".yoyo/agents"));
        want.push(dir.join(".claude/agents"));
    }
    want.push(home.path().join(".yoyo/agents"));
    want.push(home.path().join(".claude/agents"));
    assert_eq!(d.searched, want);
    assert!(!render_agents_listing(&d).contains("parent directories not searched"));
}

#[test]
fn walk_up_closest_directory_wins_and_the_root_is_shadowed() {
    let (root, a, _b) = repo_with_subdirs();
    let home = tempfile::tempdir().unwrap();
    // The root's copy is `.yoyo`, which would beat `.claude` inside one
    // directory: closeness must still win across directories.
    let far = write(root.path(), ".yoyo/agents/r.md", &agent("rev", "root"));
    let near = write(&a, ".claude/agents/r.md", &agent("rev", "a"));
    let d = discover_agent_defs(&a, Some(home.path()));
    assert_eq!(d.agents.len(), 1);
    assert_eq!(d.agents[0].path, near);
    assert_eq!(d.agents[0].description, "a");
    assert_eq!(
        d.shadowed
            .iter()
            .map(|s| (s.def.path.clone(), s.by.clone()))
            .collect::<Vec<_>>(),
        vec![(far.clone(), near.clone())]
    );
    assert!(render_agents_listing(&d).contains(&format!(
        "  rev  {}  shadowed by {}\n",
        far.display(),
        near.display()
    )));
}

#[test]
fn without_a_git_dir_only_the_cwd_is_searched() {
    let parent = tempfile::tempdir().unwrap();
    assert!(!parent.path().ancestors().any(|a| a.join(".git").exists()));
    let home = tempfile::tempdir().unwrap();
    let cwd = parent.path().join("sub");
    std::fs::create_dir(&cwd).unwrap();
    write(
        parent.path(),
        ".claude/agents/r.md",
        &agent("rev", "parent"),
    );
    let d = discover_agent_defs(&cwd, Some(home.path()));
    assert!(d.agents.is_empty(), "parent file loaded: {:?}", d.agents);
    assert_eq!(d.repo_root, None);
    assert_eq!(
        d.searched,
        vec![
            cwd.join(".yoyo/agents"),
            cwd.join(".claude/agents"),
            home.path().join(".yoyo/agents"),
            home.path().join(".claude/agents"),
        ]
    );
}

#[test]
fn a_home_inside_the_walk_stays_user_scope_and_is_listed_once() {
    // The repo root is the home directory (a dotfiles repo at ~).
    let (home, a, _b) = repo_with_subdirs();
    let file = write(home.path(), ".claude/agents/r.md", &agent("rev", "home"));
    let d = discover_agent_defs(&a, Some(home.path()));
    assert_eq!(d.agents.len(), 1);
    assert_eq!(d.agents[0].path, file);
    assert_eq!(d.agents[0].scope, AgentScope::User);
    assert!(d.shadowed.is_empty());
    assert_eq!(
        d.searched,
        vec![
            a.join(".yoyo/agents"),
            a.join(".claude/agents"),
            home.path().join(".yoyo/agents"),
            home.path().join(".claude/agents"),
        ]
    );
}

#[test]
fn render_shows_resolved_tools_unresolved_entries_and_whole_tool_removal() {
    let def = parse_agent_file(
        "---\nname: r\ndescription: d\ntools: Read, WebFetch, Bash\ndisallowedTools: Bash(git push *)\n---\n",
        &p("/proj/.claude/agents/r.md"),
        AgentScope::Project,
    )
    .unwrap();
    assert_eq!(
        render_agent(&def),
        "  r  [project] /proj/.claude/agents/r.md\n    d\n    tools: read_file · disallowedTools: Bash(git push *)\n    unresolved: WebFetch (no yoyo tool), Bash (bash removed by disallowedTools)\n    note: disallowedTools Bash(git push *) removes the whole tool (yoyo has no per-command rule for sub-agents)\n"
    );
}

#[test]
fn render_marks_an_agent_whose_tools_resolve_to_nothing_as_not_usable() {
    let def = parse_agent_file(
        "---\nname: r\ndescription: d\ntools: WebFetch, Task\n---\n",
        &p("/proj/.claude/agents/r.md"),
        AgentScope::Project,
    )
    .unwrap();
    assert_eq!(
        render_agent(&def),
        "  r  [project] /proj/.claude/agents/r.md\n    d\n    unresolved: WebFetch (no yoyo tool), Task (nested delegation not supported yet)\n    not usable: none of tools [WebFetch, Task] resolves to a yoyo sub-agent tool\n"
    );
}

#[test]
fn a_def_in_a_subfolder_is_found_with_the_same_scope_as_a_top_level_one() {
    let proj = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let top = write(proj.path(), ".claude/agents/top.md", &agent("top", "t"));
    let nested = write(
        proj.path(),
        ".claude/agents/review/x.md",
        &agent("x", "reviews"),
    );
    let user_nested = write(home.path(), ".yoyo/agents/a/b/u.md", &agent("u", "u"));
    let d = discover_agent_defs(proj.path(), Some(home.path()));
    let got: Vec<(&str, &Path, AgentScope)> = d
        .agents
        .iter()
        .map(|a| (a.name.as_str(), a.path.as_path(), a.scope))
        .collect();
    assert_eq!(
        got,
        vec![
            ("x", nested.as_path(), AgentScope::Project),
            ("top", top.as_path(), AgentScope::Project),
            ("u", user_nested.as_path(), AgentScope::User),
        ]
    );
    assert!(d.skipped.is_empty(), "{:?}", d.skipped);
}

#[test]
fn eight_levels_down_is_found_and_nine_is_skipped_with_a_depth_reason() {
    let proj = tempfile::tempdir().unwrap();
    let eight = write(
        proj.path(),
        ".claude/agents/1/2/3/4/5/6/7/8/ok.md",
        &agent("ok", "eight deep"),
    );
    write(
        proj.path(),
        ".claude/agents/1/2/3/4/5/6/7/8/9/far.md",
        &agent("far", "nine deep"),
    );
    let d = discover_agent_defs(proj.path(), None);
    let names: Vec<&Path> = d.agents.iter().map(|a| a.path.as_path()).collect();
    assert_eq!(names, vec![eight.as_path()]);
    assert_eq!(
        d.skipped,
        vec![Skipped {
            path: proj.path().join(".claude/agents/1/2/3/4/5/6/7/8/9"),
            reason: "directory is more than 8 levels below the agents directory: not searched"
                .into()
        }]
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_directory_is_not_descended_into_and_is_reported() {
    let proj = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "linked.md", &agent("linked", "outside"));
    let agents = proj.path().join(".claude/agents");
    std::fs::create_dir_all(&agents).unwrap();
    let link = agents.join("shared");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    // A loop back to the agents dir itself must not hang either.
    let lp = agents.join("loop");
    std::os::unix::fs::symlink(&agents, &lp).unwrap();
    // A symlinked *file* is still read, as before this change.
    let target = write(outside.path(), "f.md", &agent("filelink", "via link"));
    let file_link = agents.join("f.md");
    std::os::unix::fs::symlink(&target, &file_link).unwrap();
    let d = discover_agent_defs(proj.path(), None);
    let names: Vec<(&str, &Path)> = d
        .agents
        .iter()
        .map(|a| (a.name.as_str(), a.path.as_path()))
        .collect();
    assert_eq!(names, vec![("filelink", file_link.as_path())]);
    let reason = "symlinked directory: not followed (yoyo only searches folders inside the agents directory)";
    assert_eq!(
        d.skipped,
        vec![
            Skipped {
                path: lp,
                reason: reason.into()
            },
            Skipped {
                path: link,
                reason: reason.into()
            },
        ]
    );
}

#[test]
fn same_name_in_two_subfolders_of_one_dir_has_a_sorted_winner_and_a_shadowed_loser() {
    let proj = tempfile::tempdir().unwrap();
    let loser = write(proj.path(), ".claude/agents/zeta/r.md", &agent("rev", "z"));
    let winner = write(proj.path(), ".claude/agents/alpha/r.md", &agent("rev", "a"));
    for _ in 0..2 {
        let d = discover_agent_defs(proj.path(), None);
        assert_eq!(d.agents.len(), 1);
        assert_eq!(d.agents[0].path, winner);
        let shadowed: Vec<(&Path, &Path)> = d
            .shadowed
            .iter()
            .map(|s| (s.def.path.as_path(), s.by.as_path()))
            .collect();
        assert_eq!(shadowed, vec![(loser.as_path(), winner.as_path())]);
        assert!(render_agents_listing(&d).contains(&format!(
            "  rev  {}  shadowed by {}\n",
            loser.display(),
            winner.display()
        )));
    }
}
