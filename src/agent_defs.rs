//! Named sub-agent definition files (#1250, milestone 1 of 4): discover,
//! validate and list. Nothing here becomes a prompt yet — that is milestone 2,
//! together with the project-trust gate.
//!
//! The format is Claude Code's (<https://code.claude.com/docs/en/sub-agents>):
//! a Markdown file whose first line is `---`, a YAML frontmatter block closed
//! by `---`, then the body (the future system prompt). The frontmatter is read
//! with `serde_yaml_ng` — the YAML crate yoagent already depends on — never by
//! hand (#1184 is the lesson about hand-parsed config shapes).
//!
//! Where it looks: `.yoyo/agents/` and `.claude/agents/` in the cwd and in
//! every parent up to and including the repo root (the nearest ancestor that
//! holds `.git`, file or directory), as Claude Code does. With no `.git` above
//! the cwd only the cwd is searched, so a stray `~/.claude/agents` is never
//! read as project scope. The home directory is never a project ancestor: its
//! dirs are user scope, `~/.yoyo/agents/` and `~/.claude/agents/`, searched
//! last. Non-recursive. Precedence on a duplicate `name`: the closest
//! directory wins, `.yoyo` beats `.claude` within one directory, and any
//! project dir beats user; the loser is listed as shadowed.

use std::path::{Path, PathBuf};

/// The frontmatter keys yoyo reads. Anything else is kept and reported as an
/// ignored key, so a user sees what yoyo does not act on.
const KNOWN_KEYS: &[&str] = &[
    "name",
    "description",
    "tools",
    "disallowedTools",
    "model",
    "maxTurns",
];

/// Longest `name` accepted.
const MAX_NAME_CHARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentScope {
    Project,
    User,
}

impl AgentScope {
    fn label(self) -> &'static str {
        match self {
            AgentScope::Project => "project",
            AgentScope::User => "user",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentDef {
    pub name: String,
    pub description: String,
    pub tools: Option<Vec<String>>,
    pub disallowed_tools: Option<Vec<String>>,
    pub model: Option<String>,
    pub max_turns: Option<u32>,
    pub body: String,
    pub path: PathBuf,
    pub scope: AgentScope,
    pub ignored_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shadowed {
    pub def: AgentDef,
    pub by: PathBuf,
}

#[derive(Debug, Default)]
pub struct Discovery {
    pub agents: Vec<AgentDef>,
    pub skipped: Vec<Skipped>,
    pub shadowed: Vec<Shadowed>,
    /// Every directory looked in, in precedence order (missing ones included).
    pub searched: Vec<PathBuf>,
    /// True when no home directory was available, so user dirs were not searched.
    pub home_missing: bool,
    /// The nearest ancestor of the start dir holding `.git`; `None` means only
    /// the start dir was searched as project scope.
    pub repo_root: Option<PathBuf>,
}

/// Split `text` into (frontmatter, body) without byte indexing: the first
/// line must be `---`, and the block ends at the next line that is `---`.
fn split_frontmatter(text: &str) -> Result<(String, String), String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    match lines.next() {
        Some(first) if first.trim_end() == "---" => {}
        _ => return Err("first line is not `---` (no YAML frontmatter)".to_string()),
    }
    let mut frontmatter = String::new();
    for line in lines.by_ref() {
        if line.trim_end() == "---" {
            let body: String = lines.collect();
            return Ok((frontmatter, body));
        }
        frontmatter.push_str(line);
    }
    Err("frontmatter is not closed (no second `---` line)".to_string())
}

/// A string field, or a comma-separated string / YAML list of strings.
fn tool_list(value: &serde_yaml_ng::Value, key: &str) -> Result<Vec<String>, String> {
    use serde_yaml_ng::Value;
    match value {
        Value::String(s) => Ok(s
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect()),
        Value::Sequence(items) => items
            .iter()
            .map(|item| match item {
                Value::String(s) => Ok(s.trim().to_string()),
                _ => Err(format!("`{key}` list entries must be strings")),
            })
            .collect(),
        _ => Err(format!(
            "`{key}` must be a comma-separated string or a list"
        )),
    }
}

fn name_problem(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return Some("`name` is empty".to_string());
    }
    if name.starts_with('-') {
        return Some(format!("`name` {name:?} starts with `-`"));
    }
    if name.contains(':') {
        return Some(format!(
            "`name` {name:?} contains `:`, which Claude Code reserves for plugin agents"
        ));
    }
    let chars = name.chars().count();
    if chars > MAX_NAME_CHARS {
        return Some(format!(
            "`name` is {chars} characters (the limit is {MAX_NAME_CHARS})"
        ));
    }
    None
}

/// Parse one agent file. `Err` carries the one-line reason the listing prints.
pub fn parse_agent_file(text: &str, path: &Path, scope: AgentScope) -> Result<AgentDef, String> {
    use serde_yaml_ng::Value;
    let (frontmatter, body) = split_frontmatter(text)?;
    // A leading newline makes the parser's line numbers file line numbers
    // (the frontmatter starts on line 2, after the opening `---`).
    let parsed: Value = serde_yaml_ng::from_str(&format!("\n{frontmatter}"))
        .map_err(|e| format!("frontmatter is not valid YAML: {e}"))?;
    let map = match parsed {
        Value::Mapping(m) => m,
        Value::Null => serde_yaml_ng::Mapping::new(),
        _ => return Err("frontmatter is not a `key: value` mapping".to_string()),
    };
    let get = |key: &str| map.get(key);
    let string_field = |key: &str| -> Result<Option<String>, String> {
        match get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(format!("`{key}` must be a string")),
        }
    };
    let name = string_field("name")?.ok_or_else(|| "no `name`".to_string())?;
    if let Some(problem) = name_problem(&name) {
        return Err(problem);
    }
    let description = match string_field("description")? {
        Some(d) if !d.trim().is_empty() => d,
        _ => return Err(format!("`name` {name:?} has no `description`")),
    };
    let tools = get("tools").map(|v| tool_list(v, "tools")).transpose()?;
    let disallowed_tools = get("disallowedTools")
        .map(|v| tool_list(v, "disallowedTools"))
        .transpose()?;
    let model = string_field("model")?;
    let max_turns = match get("maxTurns") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => Some(
            n.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| format!("`maxTurns` {n} is not a non-negative whole number"))?,
        ),
        Some(_) => return Err("`maxTurns` must be a whole number".to_string()),
    };
    let ignored_keys = map
        .keys()
        .map(|k| match k {
            Value::String(s) => s.clone(),
            other => serde_yaml_ng::to_string(other)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "?".to_string()),
        })
        .filter(|k| !KNOWN_KEYS.contains(&k.as_str()))
        .collect();
    Ok(AgentDef {
        name,
        description,
        tools,
        disallowed_tools,
        model,
        max_turns,
        body: body.trim().to_string(),
        path: path.to_path_buf(),
        scope,
        ignored_keys,
    })
}

/// The project directories, closest first: `start` and each ancestor up to and
/// including the nearest one holding `.git` (a file in a worktree or
/// submodule, a directory otherwise). No `.git` anywhere above → `start` only,
/// never a walk to `/`. A home directory met as a strict ancestor is left out:
/// its dirs are user scope and are searched as such. Returns the dirs and the
/// repo root, if one was found.
fn project_walk(start: &Path, home: Option<&Path>) -> (Vec<PathBuf>, Option<PathBuf>) {
    let Some(root) = start.ancestors().find(|a| a.join(".git").exists()) else {
        return (vec![start.to_path_buf()], None);
    };
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let home = home.map(canonical);
    let mut dirs = Vec::new();
    for dir in start.ancestors() {
        let is_home = home.as_ref().is_some_and(|h| *h == canonical(dir));
        if dir == start || !is_home {
            dirs.push(dir.to_path_buf());
        }
        if dir == root {
            break;
        }
    }
    (dirs, Some(root.to_path_buf()))
}

/// Every agent directory, in precedence order: `.yoyo` then `.claude` for each
/// project dir (closest first), then the two under home.
fn agent_dirs(project_dirs: &[PathBuf], home: Option<&Path>) -> Vec<(PathBuf, AgentScope)> {
    let mut dirs = Vec::new();
    for dir in project_dirs {
        dirs.push((dir.join(".yoyo/agents"), AgentScope::Project));
        dirs.push((dir.join(".claude/agents"), AgentScope::Project));
    }
    if let Some(home) = home {
        dirs.push((home.join(".yoyo/agents"), AgentScope::User));
        dirs.push((home.join(".claude/agents"), AgentScope::User));
    }
    dirs
}

/// Discover every `*.md` file in the agent dirs (non-recursive), walking up
/// from `start` (see [`project_walk`]). Takes the start dir and home as
/// parameters so tests never read the real cwd or home.
pub fn discover_agent_defs(start: &Path, home: Option<&Path>) -> Discovery {
    let (project_dirs, repo_root) = project_walk(start, home);
    let mut out = Discovery {
        home_missing: home.is_none(),
        repo_root,
        ..Discovery::default()
    };
    let mut scanned: Vec<PathBuf> = Vec::new();
    for (dir, scope) in agent_dirs(&project_dirs, home) {
        out.searched.push(dir.clone());
        // Running from $HOME makes a project dir and a user dir the same
        // directory; scan it once (as the project scope) instead of having
        // every file shadow itself.
        let canonical = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if scanned.contains(&canonical) {
            continue;
        }
        scanned.push(canonical);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                out.skipped.push(Skipped {
                    path: dir.clone(),
                    reason: format!("could not read directory: {e}"),
                });
                continue;
            }
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "md") && p.is_file())
            .collect();
        files.sort();
        for path in files {
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| format!("could not read: {e}"))
                .and_then(|text| parse_agent_file(&text, &path, scope));
            match parsed {
                Err(reason) => out.skipped.push(Skipped { path, reason }),
                Ok(def) => match out.agents.iter().find(|a| a.name == def.name) {
                    Some(winner) => {
                        let by = winner.path.clone();
                        out.shadowed.push(Shadowed { def, by });
                    }
                    None => out.agents.push(def),
                },
            }
        }
    }
    out
}

/// One listing line block for a loaded agent.
pub fn render_agent(def: &AgentDef) -> String {
    let mut s = format!(
        "  {}  [{}] {}\n    {}\n",
        def.name,
        def.scope.label(),
        def.path.display(),
        def.description.trim()
    );
    let join = |v: &[String]| {
        if v.is_empty() {
            "(none listed)".to_string()
        } else {
            v.join(", ")
        }
    };
    let mut details = Vec::new();
    // The tool fields as yoyo resolves them against a sub-agent child's real
    // tool set, not the raw strings: `tools: Read` printed verbatim read as if
    // yoyo understood it.
    let mut tool_notes = Vec::new();
    if def.tools.is_some() || def.disallowed_tools.is_some() {
        let available = crate::agent_tool_names::child_tool_names();
        let r = crate::agent_tool_names::resolve_agent_tools(
            def.tools.as_deref(),
            def.disallowed_tools.as_deref().unwrap_or(&[]),
            &available,
        );
        if r.usable {
            details.push(format!("tools: {}", join(&r.resolved)));
        }
        if let Some(t) = &def.disallowed_tools {
            details.push(format!("disallowedTools: {}", join(t)));
        }
        if !r.unresolved.is_empty() {
            let entries: Vec<String> = r
                .unresolved
                .iter()
                .map(|u| {
                    let field = if u.field == "tools" {
                        String::new()
                    } else {
                        format!("{} ", u.field)
                    };
                    format!("{field}{} ({})", u.entry, u.reason)
                })
                .collect();
            tool_notes.push(format!("unresolved: {}", entries.join(", ")));
        }
        for entry in &r.whole_tool_removals {
            tool_notes.push(format!(
                "note: disallowedTools {entry} removes the whole tool (yoyo has no per-command rule for sub-agents)"
            ));
        }
        if !r.usable {
            tool_notes.push(format!(
                "not usable: none of tools [{}] resolves to a yoyo sub-agent tool",
                def.tools.as_deref().unwrap_or(&[]).join(", ")
            ));
        }
    }
    if let Some(m) = &def.model {
        details.push(format!("model: {m}"));
    }
    if let Some(n) = def.max_turns {
        details.push(format!("maxTurns: {n}"));
    }
    if !details.is_empty() {
        s.push_str(&format!("    {}\n", details.join(" · ")));
    }
    for note in &tool_notes {
        s.push_str(&format!("    {note}\n"));
    }
    if !def.ignored_keys.is_empty() {
        s.push_str(&format!(
            "    ignored keys: {}\n",
            def.ignored_keys.join(", ")
        ));
    }
    s
}

/// One listing line for a skipped file.
pub fn render_skipped(skip: &Skipped) -> String {
    format!("  {}: {}\n", skip.path.display(), skip.reason)
}

/// The whole `yoyo agents` output.
pub fn render_agents_listing(d: &Discovery) -> String {
    let mut s = String::new();
    let looked_in = |s: &mut String| {
        s.push_str("Looked in (closest first, not recursive):\n");
        for dir in &d.searched {
            s.push_str(&format!("  {}\n", dir.display()));
        }
        if d.repo_root.is_none() {
            s.push_str("  (no `.git` in the cwd or above it: parent directories not searched)\n");
        }
        if d.home_missing {
            s.push_str("  (no home directory: ~/.yoyo/agents and ~/.claude/agents not searched)\n");
        }
    };
    if d.agents.is_empty() && d.skipped.is_empty() && d.shadowed.is_empty() {
        s.push_str("No named sub-agent files found.\n");
        looked_in(&mut s);
        return s;
    }
    s.push_str("Named sub-agents (preview: discovered and validated, not yet used by yoyo)\n");
    if d.agents.is_empty() {
        s.push_str("\nNo valid agent files.\n");
    } else {
        s.push('\n');
        for def in &d.agents {
            s.push_str(&render_agent(def));
        }
    }
    if !d.shadowed.is_empty() {
        s.push_str("\nShadowed:\n");
        for sh in &d.shadowed {
            s.push_str(&format!(
                "  {}  {}  shadowed by {}\n",
                sh.def.name,
                sh.def.path.display(),
                sh.by.display()
            ));
        }
    }
    if !d.skipped.is_empty() {
        s.push_str("\nSkipped:\n");
        for skip in &d.skipped {
            s.push_str(&render_skipped(skip));
        }
    }
    s.push('\n');
    looked_in(&mut s);
    s
}

/// `yoyo agents`: list what was found from the cwd up to the repo root, and
/// under the home dir.
pub fn handle_agents_command() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let home = crate::config::home_dir();
    let d = discover_agent_defs(&cwd, home.as_deref());
    print!("{}", render_agents_listing(&d));
}

#[cfg(test)]
#[path = "agent_defs_tests.rs"]
mod agent_defs_tests;
