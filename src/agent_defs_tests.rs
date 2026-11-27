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
            "`name` \"a:b\" contains `:`",
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
        "  code-reviewer  [project] /proj/.claude/agents/code-reviewer.md\n    Reviews diffs\n    tools: Read, Grep · model: sonnet · maxTurns: 5\n    ignored keys: color\n"
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
    // Non-recursive: a nested file is not loaded.
    write(
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
        vec![("rev", py.as_path()), ("solo", only_user.as_path())]
    );
    assert_eq!(d.agents[1].scope, AgentScope::User);
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
        "No named sub-agent files found.\nLooked in (cwd only, not recursive):\n  {}\n  {}\n  {}\n  {}\n",
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
