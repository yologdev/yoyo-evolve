//! The `agent` tool: the model delegates to a named agent file (#1250,
//! milestone 2 of 4).
//!
//! `src/agent_defs.rs` discovers `.yoyo/agents/*.md` / `.claude/agents/*.md`
//! (project) and `~/.yoyo/agents/*.md` / `~/.claude/agents/*.md` (user);
//! `src/agent_tool_names.rs` resolves their `tools` / `disallowedTools`. This
//! module turns those into one tool the model can call, shaped like Claude
//! Code's Agent/Task tool: `{subagent_type, prompt}`.
//!
//! **The trust gate ships with the dispatcher, in the same commit, on purpose.**
//! A project-scope body is prose the project wrote, and it becomes a child's
//! system prompt — a child that has `bash`. So a project agent is dispatchable
//! only when the project is trusted (`--trust-project`, the same switch that
//! gates project skills, hooks and permissions). The gate is
//! [`dispatch_refusal`], a pure function; the caller passes the trust state.
//! It is checked twice: at build (what the model is told exists) and again at
//! every call against the *live* trust state, so a `/cd` that revokes trust
//! also revokes the agents it had granted.
//!
//! What the model sees: `name: description` for dispatchable agents only.
//! Bodies never appear in the description (Claude Code loads a body only when
//! the agent runs), and an untrusted project agent is left out entirely,
//! because its description is project prose too.
//!
//! Product safety: the tool is registered only when at least one agent is
//! dispatchable. A user with no agent files, or with only untrusted project
//! files, gets the same tool list as before this module existed.
//!
//! Not here yet (#1250): frontmatter `model` and `maxTurns` (milestone 3;
//! every named agent runs on the `sub_agent` child model with its turn cap),
//! a REPL `/agents`, nested delegation, and the docs page (milestone 4).

use std::path::PathBuf;

use yoagent::types::{AgentTool, Content, ToolContext, ToolError, ToolResult};
use yoagent::SharedState;

use crate::agent_defs::{discover_agent_defs, AgentDef, AgentScope};
use crate::agent_tool_names::{child_tool_names, resolve_agent_tools, ToolResolution};
use crate::AgentConfig;

/// The name the model calls. Listed in `BUILTIN_TOOL_NAMES` so an MCP server
/// exposing `agent` is prefixed rather than colliding.
pub(crate) const AGENT_TOOL_NAME: &str = "agent";

/// Total bytes of the `name: description` listing in the tool description.
const LISTING_CAP_BYTES: usize = 8_000;

/// Bytes of one agent's description in the listing.
const PER_AGENT_DESCRIPTION_BYTES: usize = 400;

/// The remedy, in the words the other project-trust refusals use
/// (`cli::project_skill_refusal_message`): the flag is `--trust-project`.
const TRUST_REMEDY: &str = "Re-run yoyo with --trust-project to allow this project's agents \
                            this session.";

/// Why `def` may not run, or `None` when it may.
///
/// Pure: `project_trusted` is passed in (production passes
/// `cli::is_trust_project()`), so tests never touch the process-wide switch.
pub(crate) fn dispatch_refusal(
    def: &AgentDef,
    resolution: &ToolResolution,
    project_trusted: bool,
) -> Option<String> {
    if def.scope == AgentScope::Project && !project_trusted {
        return Some(format!(
            "Agent '{}' was not run: it is defined by this project ({}), not by the user, and \
             its body would become a sub-agent's system prompt with tool access. Project \
             agents run only in a trusted project. {TRUST_REMEDY}",
            def.name,
            def.path.display()
        ));
    }
    if !resolution.usable {
        let entries: Vec<String> = resolution
            .unresolved
            .iter()
            .filter(|u| u.field == "tools")
            .map(|u| format!("{} ({})", u.entry, u.reason))
            .collect();
        return Some(format!(
            "Agent '{}' ({}) was not run: none of its `tools` entries is a tool a yoyo \
             sub-agent has: {}. Fix the `tools` line in that file.",
            def.name,
            def.path.display(),
            entries.join(", ")
        ));
    }
    None
}

/// A def's tool fields resolved against the real sub-agent child tool set.
fn resolution_for(def: &AgentDef) -> ToolResolution {
    resolve_agent_tools(
        def.tools.as_deref(),
        def.disallowed_tools.as_deref().unwrap_or(&[]),
        &child_tool_names(),
    )
}

/// Should the `agent` tool be registered at all? Only when something can run.
pub(crate) fn should_register(defs: &[AgentDef], project_trusted: bool) -> bool {
    defs.iter()
        .any(|d| dispatch_refusal(d, &resolution_for(d), project_trusted).is_none())
}

/// Cut `s` to at most `max` bytes at a char boundary (never mid-character).
fn cut(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut b = max;
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    &s[..b]
}

/// The tool description: what the tool does, then `name: description` for
/// each agent in `listed` (already filtered to dispatchable ones). Bodies are
/// never read here.
pub(crate) fn tool_description(listed: &[&AgentDef]) -> String {
    let mut out = String::from(
        "Delegate a task to a named agent defined in an agent file (.yoyo/agents/ or \
         .claude/agents/, in this project or the user's home). Pass `subagent_type`, one of \
         the names below, and `prompt`, the full task: the agent starts with a clean context, \
         sees only this prompt, runs with its own instructions and tool set, and returns its \
         final answer.\n\nAvailable agents:\n",
    );
    let mut used = 0;
    let mut shown = 0;
    for def in listed {
        let flat = def
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let mut desc = cut(&flat, PER_AGENT_DESCRIPTION_BYTES).to_string();
        if desc.len() < flat.len() {
            desc.push('…');
        }
        let line = format!("- {}: {}\n", def.name, desc);
        if used + line.len() > LISTING_CAP_BYTES {
            break;
        }
        used += line.len();
        shown += 1;
        out.push_str(&line);
    }
    if shown < listed.len() {
        out.push_str(&format!("({} more not shown)\n", listed.len() - shown));
    }
    out
}

/// One discovered agent, with its child prebuilt when it was dispatchable at
/// build time.
struct Entry {
    def: AgentDef,
    resolution: ToolResolution,
    child: Option<Box<dyn AgentTool>>,
}

/// The `agent` tool.
pub(crate) struct AgentDispatchTool {
    description: String,
    entries: Vec<Entry>,
    /// The live trust state, read at every call (`cli::is_trust_project` in
    /// production).
    trusted: fn() -> bool,
}

impl AgentDispatchTool {
    /// `None` when no def is dispatchable under `trusted()` — the caller then
    /// registers nothing. `make_child` is called only for dispatchable defs.
    fn new(
        defs: Vec<AgentDef>,
        trusted: fn() -> bool,
        mut make_child: impl FnMut(&AgentDef, &ToolResolution) -> Option<Box<dyn AgentTool>>,
    ) -> Option<Self> {
        let now = trusted();
        if !should_register(&defs, now) {
            return None;
        }
        let mut entries = Vec::with_capacity(defs.len());
        for def in defs {
            let resolution = resolution_for(&def);
            let child = if dispatch_refusal(&def, &resolution, now).is_none() {
                make_child(&def, &resolution)
            } else {
                None
            };
            entries.push(Entry {
                def,
                resolution,
                child,
            });
        }
        let listed: Vec<&AgentDef> = entries
            .iter()
            .filter(|e| dispatch_refusal(&e.def, &e.resolution, now).is_none())
            .map(|e| &e.def)
            .collect();
        let description = tool_description(&listed);
        Some(Self {
            description,
            entries,
            trusted,
        })
    }

    /// Names a call may use right now, for error messages.
    fn available_names(&self) -> String {
        let now = (self.trusted)();
        let names: Vec<&str> = self
            .entries
            .iter()
            .filter(|e| dispatch_refusal(&e.def, &e.resolution, now).is_none())
            .map(|e| e.def.name.as_str())
            .collect();
        if names.is_empty() {
            "(none)".to_string()
        } else {
            names.join(", ")
        }
    }
}

/// Build the `agent` tool for the top-level agent, or `None` when no agent
/// file is dispatchable (the tool list then stays exactly as before).
///
/// Discovery runs once, here, from the session's working directory.
pub(crate) fn build_agent_dispatch_tool(
    config: &AgentConfig,
    shared_state: &SharedState,
) -> Option<Box<dyn AgentTool>> {
    let start = match &config.bash_cwd {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir().ok()?,
    };
    let discovery = discover_agent_defs(&start, crate::config::home_dir().as_deref());
    let tool = AgentDispatchTool::new(discovery.agents, crate::cli::is_trust_project, |def, r| {
        Some(build_child(config, def, r, shared_state))
    })?;
    Some(Box::new(tool))
}

/// The inner tool's flavor. Its name and description are never shown to the
/// model: the outer `agent` tool presents its own.
const NAMED_AGENT_FLAVOR: crate::tools::DispatchFlavor = crate::tools::DispatchFlavor {
    name: AGENT_TOOL_NAME,
    description: "A named agent from an agent file.",
};

/// One child for one def, assembled from the same pieces `sub_agent` uses:
/// `sub_agent_child_tools` (directory restrictions, `/read`·`/plan` mode, user
/// deny + hard deny on bash, `--read-only-subagents`), the same child-model
/// resolution, and the same output marker. Then narrowed to the def's
/// resolved tools, with the def's body as the system prompt (plus the
/// runtime grounding note every builder gets, `compose_system_prompt`).
///
/// `model` / `maxTurns` from the frontmatter are ignored until #1250
/// milestone 3. No fallback-model retry yet either: one attempt.
fn build_child(
    config: &AgentConfig,
    def: &AgentDef,
    resolution: &ToolResolution,
    shared_state: &SharedState,
) -> Box<dyn AgentTool> {
    let child_disallowed = if crate::cli::is_read_only_subagents() {
        crate::tools::read_only_child_disallowed(&config.disallowed_tools)
    } else {
        config.disallowed_tools.clone()
    };
    let mut child_tools = crate::tools::sub_agent_child_tools(
        &config.dir_restrictions,
        &child_disallowed,
        &config.permissions.deny,
        !config.auto_approve,
        config.bash_cwd.as_deref(),
    );
    child_tools.retain(|t| resolution.resolved.iter().any(|r| r == t.name()));
    let child_model = crate::config::effective_sub_agent_model(&config.model);
    let inner = crate::tools::sub_agent_tool_for(
        config,
        &config.provider,
        &child_model,
        &config.api_key,
        child_tools,
        shared_state,
        &NAMED_AGENT_FLAVOR,
    )
    .with_system_prompt(crate::agent_builder::compose_system_prompt(
        &def.body,
        &config.provider,
        &child_model,
    ));
    Box::new(crate::tool_wrappers::SubAgentOutputMarkerTool::new(
        Box::new(inner),
    ))
}

#[async_trait::async_trait]
impl AgentTool for AgentDispatchTool {
    fn name(&self) -> &str {
        AGENT_TOOL_NAME
    }

    fn label(&self) -> &str {
        AGENT_TOOL_NAME
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "subagent_type": {
                    "type": "string",
                    "description": "The name of the agent to run (one listed in this tool's description)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The full task for the agent; it sees nothing else from this conversation"
                }
            },
            "required": ["subagent_type", "prompt"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let name = params
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(name) = name else {
            return Err(ToolError::InvalidArgs(format!(
                "Missing required 'subagent_type'. Available agents: {}",
                self.available_names()
            )));
        };
        let Some(prompt) = params.get("prompt").and_then(|v| v.as_str()) else {
            return Err(ToolError::InvalidArgs(
                "Missing required 'prompt' parameter".to_string(),
            ));
        };
        let Some(entry) = self.entries.iter().find(|e| e.def.name == name) else {
            return Err(ToolError::Failed(format!(
                "Unknown agent '{name}'. Available agents: {}",
                self.available_names()
            )));
        };
        if let Some(refusal) = dispatch_refusal(&entry.def, &entry.resolution, (self.trusted)()) {
            return Err(ToolError::Failed(refusal));
        }
        let Some(child) = &entry.child else {
            return Err(ToolError::Failed(format!(
                "Agent '{name}' became runnable after this session's tools were built (the \
                 project was trusted later). Restart yoyo to use it."
            )));
        };
        let result = child
            .execute(serde_json::json!({ "task": prompt }), ctx)
            .await?;
        Ok(ToolResult {
            content: if result.content.is_empty() {
                vec![Content::Text {
                    text: String::new(),
                }]
            } else {
                result.content
            },
            details: result.details,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
    }

    fn agent_file(name: &str, desc: &str, extra: &str, body: &str) -> String {
        format!("---\nname: {name}\ndescription: {desc}\n{extra}---\n{body}\n")
    }

    /// Real discovery over a project dir and a home dir.
    fn discover(proj: &Path, home: &Path) -> Vec<AgentDef> {
        discover_agent_defs(proj, Some(home)).agents
    }

    fn by_name<'a>(defs: &'a [AgentDef], name: &str) -> &'a AgentDef {
        defs.iter().find(|d| d.name == name).expect(name)
    }

    /// One user agent `helper`, one project agent `proj-rev`, both bodies
    /// carrying a sentinel that must never reach the description.
    fn mixed() -> (tempfile::TempDir, tempfile::TempDir, Vec<AgentDef>) {
        let proj = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        write(
            home.path(),
            ".yoyo/agents/helper.md",
            &agent_file("helper", "user helper agent", "", "SENTINEL_USER_BODY_7f3a"),
        );
        write(
            proj.path(),
            ".claude/agents/rev.md",
            &agent_file(
                "proj-rev",
                "PROJECT_DESC_SENTINEL reviews code",
                "",
                "SENTINEL_PROJECT_BODY_9c1e",
            ),
        );
        let defs = discover(proj.path(), home.path());
        assert_eq!(defs.len(), 2, "fixture must discover both files: {defs:?}");
        (proj, home, defs)
    }

    #[test]
    fn user_scope_def_is_dispatchable_untrusted() {
        let (_p, _h, defs) = mixed();
        let d = by_name(&defs, "helper");
        assert_eq!(d.scope, AgentScope::User);
        assert_eq!(dispatch_refusal(d, &resolution_for(d), false), None);
    }

    #[test]
    fn project_scope_def_untrusted_is_refused_with_name_file_and_remedy() {
        let (_p, _h, defs) = mixed();
        let d = by_name(&defs, "proj-rev");
        assert_eq!(d.scope, AgentScope::Project);
        let refusal = dispatch_refusal(d, &resolution_for(d), false)
            .expect("an untrusted project agent must be refused");
        assert!(
            refusal.contains("Agent 'proj-rev' was not run"),
            "{refusal}"
        );
        assert!(refusal.contains(&d.path.display().to_string()), "{refusal}");
        assert!(refusal.contains("--trust-project"), "{refusal}");
    }

    #[test]
    fn project_scope_def_trusted_is_dispatchable() {
        let (_p, _h, defs) = mixed();
        let d = by_name(&defs, "proj-rev");
        assert_eq!(dispatch_refusal(d, &resolution_for(d), true), None);
    }

    #[test]
    fn unusable_tools_refuse_naming_the_unresolved_entries() {
        let proj = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        write(
            home.path(),
            ".yoyo/agents/odd.md",
            &agent_file(
                "odd",
                "no real tools",
                "tools: Frobnicate, NotebookEdit\n",
                "b",
            ),
        );
        let defs = discover(proj.path(), home.path());
        let d = by_name(&defs, "odd");
        let r = resolution_for(d);
        assert!(!r.usable);
        let refusal = dispatch_refusal(d, &r, true).expect("unusable must refuse");
        assert!(refusal.contains("Agent 'odd'"), "{refusal}");
        assert!(refusal.contains("Frobnicate (no yoyo tool)"), "{refusal}");
        assert!(refusal.contains("NotebookEdit"), "{refusal}");
    }

    #[test]
    fn description_lists_dispatchable_names_and_never_a_body() {
        let (_p, _h, defs) = mixed();
        for trusted in [false, true] {
            let listed: Vec<&AgentDef> = defs
                .iter()
                .filter(|d| dispatch_refusal(d, &resolution_for(d), trusted).is_none())
                .collect();
            let desc = tool_description(&listed);
            assert!(desc.contains("- helper: user helper agent\n"), "{desc}");
            assert_eq!(
                desc.contains("proj-rev"),
                trusted,
                "project agent listed iff trusted: {desc}"
            );
            assert_eq!(desc.contains("PROJECT_DESC_SENTINEL"), trusted, "{desc}");
            assert!(!desc.contains("SENTINEL_USER_BODY_7f3a"), "{desc}");
            assert!(!desc.contains("SENTINEL_PROJECT_BODY_9c1e"), "{desc}");
        }
        // The built tool says the same thing as the helper.
        let tool = AgentDispatchTool::new(defs, || false, |_, _| None).unwrap();
        assert!(!tool.description().contains("proj-rev"));
        assert!(!tool.description().contains("SENTINEL"));
    }

    #[test]
    fn listing_is_capped_and_cuts_multibyte_descriptions_safely() {
        let proj = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        // 3-byte chars so a byte cut at 400 would land mid-character.
        let long = "✓".repeat(1_000);
        for i in 0..40 {
            write(
                home.path(),
                &format!(".yoyo/agents/a{i:02}.md"),
                &agent_file(&format!("a{i:02}"), &long, "", "b"),
            );
        }
        let defs = discover(proj.path(), home.path());
        assert_eq!(defs.len(), 40);
        let listed: Vec<&AgentDef> = defs.iter().collect();
        let desc = tool_description(&listed);
        let listing = desc.split("Available agents:\n").nth(1).unwrap();
        assert!(listing.len() <= LISTING_CAP_BYTES + 40, "{}", listing.len());
        let shown = listing.lines().filter(|l| l.starts_with("- a")).count();
        assert!(shown > 0 && shown < 40, "shown {shown}");
        assert!(
            listing.contains(&format!("({} more not shown)", 40 - shown)),
            "{listing}"
        );
        assert!(listing.contains('…'));
    }

    #[test]
    fn register_only_when_something_is_dispatchable() {
        assert!(!should_register(&[], false));
        assert!(!should_register(&[], true));
        let proj = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        write(
            proj.path(),
            ".yoyo/agents/p.md",
            &agent_file("p", "project only", "", "b"),
        );
        let defs = discover(proj.path(), home.path());
        assert_eq!(defs.len(), 1);
        assert!(
            !should_register(&defs, false),
            "only untrusted project defs"
        );
        assert!(should_register(&defs, true));
        assert!(AgentDispatchTool::new(defs.clone(), || false, |_, _| None).is_none());
        assert!(AgentDispatchTool::new(defs, || true, |_, _| None).is_some());
    }

    fn ctx() -> ToolContext {
        ToolContext::new("test", AGENT_TOOL_NAME)
    }

    fn err_text(r: Result<ToolResult, ToolError>) -> String {
        match r {
            Ok(_) => panic!("expected a refusal, got a result"),
            Err(e) => e.to_string(),
        }
    }

    #[tokio::test]
    async fn execute_refuses_before_building_a_child() {
        let (_p, _h, defs) = mixed();
        let tool = AgentDispatchTool::new(defs, || false, |_, _| None).unwrap();

        let unknown = err_text(
            tool.execute(
                serde_json::json!({"subagent_type": "nope", "prompt": "x"}),
                ctx(),
            )
            .await,
        );
        assert!(unknown.contains("Unknown agent 'nope'"), "{unknown}");
        assert!(unknown.contains("Available agents: helper"), "{unknown}");
        assert!(!unknown.contains("proj-rev"), "{unknown}");

        let missing = err_text(
            tool.execute(serde_json::json!({"prompt": "x"}), ctx())
                .await,
        );
        assert!(
            missing.contains("Missing required 'subagent_type'"),
            "{missing}"
        );
        assert!(missing.contains("helper"), "{missing}");

        // A guessed untrusted project name gets the gate's refusal.
        let gated = err_text(
            tool.execute(
                serde_json::json!({"subagent_type": "proj-rev", "prompt": "x"}),
                ctx(),
            )
            .await,
        );
        assert!(gated.contains("--trust-project"), "{gated}");
    }

    #[tokio::test]
    async fn live_trust_revocation_refuses_a_project_agent_granted_at_build() {
        let (_p, _h, defs) = mixed();
        // Built while trusted (no real child: the gate must fire first)…
        let mut tool = AgentDispatchTool::new(defs, || true, |_, _| None).unwrap();
        assert!(tool.description().contains("proj-rev"));
        // …then trust is revoked (a `/cd` out of the trusted dir).
        tool.trusted = || false;
        let gated = err_text(
            tool.execute(
                serde_json::json!({"subagent_type": "proj-rev", "prompt": "x"}),
                ctx(),
            )
            .await,
        );
        assert!(gated.contains("Agent 'proj-rev' was not run"), "{gated}");
    }
}
