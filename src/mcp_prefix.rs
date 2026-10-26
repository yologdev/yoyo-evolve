//! #1041: connect an MCP stdio server whose tool names collide with a yoyo
//! builtin (or with a tool an earlier server already registered) under a
//! prefix, instead of skipping the whole server.
//!
//! Before this module, the collision guard in
//! `agent_builder::connect_external_servers` refused such a server outright,
//! so the flagship `@modelcontextprotocol/server-filesystem` (which exposes
//! `read_file` and `write_file`) contributed ZERO tools. Now its tools reach
//! the model as `<prefix>__<tool>` via yoagent's `McpToolSource::with_prefix`,
//! while every call still goes to the server under the server's own tool name.
//! (Superseded, recorded rather than erased: until Day 240 this said
//! `McpToolAdapter::from_client_with_prefix`, wrapped in a list-once source.)
//!
//! Since Day 240 this module is also the ONE stdio connect path for every
//! server, prefixed or not ([`connect_mcp`]): each is installed as a yoagent
//! `McpToolSource`, which withdraws a server's tools at the next run once its
//! process has exited, so the model stops calling a dead server
//! <!-- yoagent-version-claim: 0.25 -->.
//!
//! Decisions, stated where the code is:
//! - **Prefix only on collision.** A server whose names collide with nothing
//!   gets [`McpConnectPlan::Plain`] and connects under its own names
//!   (`prefix: None`); prefixing is not a rename of working servers.
//!   (Superseded: until Day 240 the plain branch was `with_mcp_server_stdio`,
//!   which listed once and kept a dead server's tools.)
//! - **The whole server is prefixed**, not just the colliding names: one
//!   consistent namespace per server.
//! - **Re-checked after prefixing.** If a prefixed name is still taken (two
//!   servers sharing a prefix), the server is skipped with the existing skip
//!   message: there is no second prefix scheme.
//! - `BUILTIN_TOOL_NAMES` stays the single authority: callers pass it in,
//!   nothing here copies it.

use std::collections::HashMap;
use std::sync::Arc;

use yoagent::agent::Agent;
use yoagent::mcp::{McpClient, McpError, McpToolSource};
use yoagent::ToolSource;

use crate::agent_builder::{detect_mcp_collisions, record_failed_server};
use crate::format::{GREEN, RED, RESET, YELLOW};

/// Longest prefix yoyo uses. Providers cap tool names (Anthropic at 64
/// characters), and the prefix is only a disambiguator, so a long
/// `[mcp_servers.<name>]` is cut rather than allowed to push every tool name
/// of that server past the cap.
const MCP_PREFIX_MAX_CHARS: usize = 32;

/// Pure: turn a server name into a prefix the provider will accept in a tool
/// name: every char outside `[A-Za-z0-9_-]` becomes `_`, the result is capped
/// at [`MCP_PREFIX_MAX_CHARS`] chars (char-wise, never a byte index), and an
/// empty result becomes `mcp`.
pub(crate) fn sanitize_mcp_prefix(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(MCP_PREFIX_MAX_CHARS)
        .collect();
    if s.is_empty() {
        "mcp".to_string()
    } else {
        s
    }
}

/// The name the model sees for `tool` under `prefix` — the exact format
/// `McpToolSource::with_prefix` (and `McpToolAdapter::with_prefix`) produce.
pub(crate) fn prefixed_tool_name(prefix: &str, tool: &str) -> String {
    format!("{prefix}__{tool}")
}

/// What to do with one MCP server, decided from its pre-flight tool listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum McpConnectPlan {
    /// Nothing collides: connect exactly as before.
    Plain,
    /// Some names collide: connect every tool as `<prefix>__<tool>`.
    /// `colliding` are the server's own names that forced the prefix.
    Prefixed {
        prefix: String,
        colliding: Vec<String>,
    },
    /// Even the prefixed names collide: skip the server. Carries the
    /// prefixed names that are still taken.
    Skip(Vec<String>),
}

/// Pure: decide how to connect a server exposing `tool_names`, given the
/// builtin names and the names earlier servers in this session registered.
pub(crate) fn plan_mcp_connect(
    tool_names: &[String],
    builtins: &[&str],
    already_registered: &[String],
    prefix: &str,
) -> McpConnectPlan {
    // One detector for both checks: `detect_mcp_collisions` over the builtins
    // plus every name an earlier server registered.
    let mut taken: Vec<&str> = builtins.to_vec();
    taken.extend(already_registered.iter().map(String::as_str));
    let colliding = detect_mcp_collisions(tool_names, &taken);
    if colliding.is_empty() {
        return McpConnectPlan::Plain;
    }
    let prefixed: Vec<String> = tool_names
        .iter()
        .map(|n| prefixed_tool_name(prefix, n))
        .collect();
    let still = detect_mcp_collisions(&prefixed, &taken);
    if still.is_empty() {
        McpConnectPlan::Prefixed {
            prefix: prefix.to_string(),
            colliding,
        }
    } else {
        McpConnectPlan::Skip(still)
    }
}

/// Pure: the one stderr line saying a server was connected under a prefix,
/// naming the server, the names that forced it and the prefix. `server_cmd`
/// is repository-authored (a project `.yoyo.toml` or a flag), so it goes
/// through `cli::sanitize_for_display` like `collision_guard_skipped_message`.
/// Glyph-free when `plain` (no em dash).
pub(crate) fn prefix_applied_message(
    server_cmd: &str,
    prefix: &str,
    colliding: &[String],
    plain: bool,
) -> String {
    let shown = crate::cli::sanitize_for_display(server_cmd);
    let names = colliding
        .iter()
        .map(|n| format!("'{}'", crate::cli::sanitize_for_display(n)))
        .collect::<Vec<_>>()
        .join(", ");
    let dash = if plain { "-" } else { "\u{2014}" };
    format!(
        "mcp: '{shown}' exposes {names}, already used by yoyo {dash} connected with every tool of this server renamed to '{prefix}__<tool>'"
    )
}

/// Spawn a client for one stdio server and wrap it in yoagent's
/// [`McpToolSource`], named `<prefix>__<tool>` when `prefix` is `Some` and
/// by the server's own names when `None`. Returns the source and the names
/// the model will see.
///
/// Why a source and not a fixed list (the old `PrefixedMcpTools` here, and
/// `Agent::with_mcp_server_stdio` for unprefixed servers, both listed once
/// and kept the tools forever): `McpToolSource` is asked again at the start
/// of every run, and when the server process has exited it withdraws the
/// tools (yoagent 0.25.15, via `McpClient::exited()`), so the model stops
/// calling a dead server. It also re-lists on
/// `notifications/tools/list_changed` <!-- yoagent-version-claim: 0.25 -->.
///
/// The tools are listed once here as well, so a server that cannot list
/// fails the connect (as `McpToolAdapter::from_client` did) instead of
/// connecting with nothing, and so the names can join `registered` for the
/// next server's collision check.
pub(crate) async fn mcp_tool_source(
    command: &str,
    args: &[&str],
    env: Option<HashMap<String, String>>,
    prefix: Option<&str>,
) -> Result<(Arc<dyn ToolSource>, Vec<String>, McpClientHandle), McpError> {
    let client = McpClient::connect_stdio(command, args, env).await?;
    let names: Vec<String> = client
        .list_tools()
        .await?
        .into_iter()
        .map(|t| match prefix {
            Some(p) => prefixed_tool_name(p, &t.name),
            None => t.name,
        })
        .collect();
    let source = McpToolSource::new(client);
    // Cloned before the source is erased to `dyn ToolSource`: the only way
    // to ask later whether this server has exited (#1041).
    let handle = Arc::clone(source.client());
    let source = match prefix {
        Some(p) => source.with_prefix(p),
        None => source,
    };
    Ok((Arc::new(source), names, handle))
}

/// The shared handle to one connected server's client: what
/// `McpToolSource::client()` hands out, kept so yoyo can ask `exited()`.
pub(crate) type McpClientHandle = Arc<tokio::sync::Mutex<McpClient>>;

/// One server that connected this session, kept so a mid-session exit can be
/// told to the model and the user (#1041). Generic over the client so tests
/// drive a fake with no process.
pub(crate) struct LiveServer<C> {
    /// The id `connect_mcp` records on a connect failure (`failure_id`), so
    /// the two notes name a server the same way.
    id: String,
    /// The names the model saw (already prefixed).
    tools: Vec<String>,
    client: C,
    /// Set once its exit has been reported. Never cleared: one note per server.
    reported: bool,
}

/// A server whose exit was just observed, carrying what both notes need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExitedServer {
    pub id: String,
    pub reason: String,
    pub tools: Vec<String>,
}

/// Every server connected this session. Written by [`connect_mcp`] on
/// success, read once per prompt by [`take_mcp_exited_note`].
static LIVE_MCP_SERVERS: std::sync::Mutex<Vec<LiveServer<McpClientHandle>>> =
    std::sync::Mutex::new(Vec::new());

/// How many tool names the exit note lists before it says "and N more".
const MCP_EXITED_TOOLS_SHOWN: usize = 10;

fn register_live_server<C>(
    store: &std::sync::Mutex<Vec<LiveServer<C>>>,
    id: &str,
    tools: &[String],
    client: C,
) {
    crate::sync_util::lock_or_recover(store).push(LiveServer {
        id: id.to_string(),
        tools: tools.to_vec(),
        client,
        reported: false,
    });
}

/// Pure: the tool list for one exited server, capped at
/// [`MCP_EXITED_TOOLS_SHOWN`] with the dropped count stated.
fn exited_tool_list(tools: &[String]) -> String {
    let shown: Vec<String> = tools
        .iter()
        .take(MCP_EXITED_TOOLS_SHOWN)
        .map(|t| format!("`{}`", crate::cli::sanitize_for_display(t)))
        .collect();
    let mut list = shown.join(", ");
    let more = tools.len().saturating_sub(MCP_EXITED_TOOLS_SHOWN);
    if more > 0 {
        list.push_str(&format!(" and {more} more"));
    }
    list
}

/// Pure: the model-facing note for servers that exited mid-session. `None`
/// for an empty slice — every session whose servers stay up, and every
/// session that configured none.
///
/// Unlike a connect failure, yoyo DID learn these tools, so it names them:
/// the model saw them on an earlier turn and will now find them gone, which
/// reads as "this capability does not exist" unless something says why.
pub(crate) fn mcp_exited_note(exited: &[ExitedServer]) -> Option<String> {
    if exited.is_empty() {
        return None;
    }
    let n = exited.len();
    let plural = if n == 1 { "" } else { "s" };
    let mut out =
        format!("[yoyo: {n} MCP server{plural} that connected earlier this session exited]\n");
    for s in exited {
        let id = crate::agent_builder::capped_failure_id(&s.id);
        let reason = crate::agent_builder::capped_failure_id(&s.reason);
        out.push_str(&format!(
            "  - mcp: {id} ({reason}); its tools are no longer available: {}\n",
            exited_tool_list(&s.tools)
        ));
    }
    out.push_str(
        "This is a server that stopped running, not evidence that the capability does not \
exist. If a request would be served by one of these tools, say it is unavailable and name \
the server above, rather than substituting your own implementation.",
    );
    Some(out)
}

/// Pure: the one stderr line for an exited server. Glyph-free when `plain`.
pub(crate) fn mcp_exited_user_line(s: &ExitedServer, plain: bool) -> String {
    let id = crate::cli::sanitize_for_display(&crate::agent_builder::capped_failure_id(&s.id));
    let reason =
        crate::cli::sanitize_for_display(&crate::agent_builder::capped_failure_id(&s.reason));
    let n = s.tools.len();
    let plural = if n == 1 { "" } else { "s" };
    let mark = if plain { "" } else { "⚠ " };
    format!("{mark}mcp: server '{id}' exited ({reason}): {n} tool{plural} no longer available")
}

/// Collect the servers whose client now reports an exit and that have not
/// been reported yet, marking them reported. Keyed on STATE (the client says
/// it exited), so any cause of death is caught, once.
///
/// The store and the exit predicate are parameters so tests drive a local
/// `Mutex` and a fake client with no process (the `drain_failure_note` seam).
fn drain_exited_with<C>(
    store: &std::sync::Mutex<Vec<LiveServer<C>>>,
    is_exited: impl Fn(&C) -> Option<String>,
) -> Vec<ExitedServer> {
    let mut guard = crate::sync_util::lock_or_recover(store);
    let mut out = Vec::new();
    for s in guard.iter_mut().filter(|s| !s.reported) {
        if let Some(reason) = is_exited(&s.client) {
            s.reported = true;
            out.push(ExitedServer {
                id: s.id.clone(),
                reason,
                tools: s.tools.clone(),
            });
        }
    }
    out
}

/// Drain newly exited servers, print one stderr line for each, and return
/// the model-facing note. Runs every prompt; each server speaks once.
fn take_mcp_exited_note_with<C>(
    store: &std::sync::Mutex<Vec<LiveServer<C>>>,
    is_exited: impl Fn(&C) -> Option<String>,
) -> Option<String> {
    let exited = drain_exited_with(store, is_exited);
    let plain = crate::format::is_plain_output();
    for s in &exited {
        eprintln!("{YELLOW}{}{RESET}", mcp_exited_user_line(s, plain));
    }
    mcp_exited_note(&exited)
}

/// Production predicate: a client busy with a call in flight is "not known
/// yet" and is asked again next turn rather than blocking the prompt.
fn client_exited(client: &McpClientHandle) -> Option<String> {
    client.try_lock().ok().and_then(|c| c.exited())
}

/// The per-turn check against the session's real servers.
pub(crate) fn take_mcp_exited_note() -> Option<String> {
    take_mcp_exited_note_with(&LIVE_MCP_SERVERS, client_exited)
}

/// Pure: join two optional notes with a blank line; `None` only if both are.
pub(crate) fn join_notes(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(format!("{a}\n\n{b}")),
        (a, b) => a.or(b),
    }
}

/// What both prompt seams prepend to a turn: the one-shot connect-failure
/// note plus any server that exited since the last turn (#1041).
pub(crate) fn take_turn_external_note() -> Option<String> {
    join_notes(
        crate::agent_builder::take_external_failure_note(),
        take_mcp_exited_note(),
    )
}

/// One MCP stdio connection to make.
pub(crate) struct McpConnect<'a> {
    pub command: &'a str,
    pub args: &'a [&'a str],
    pub env: Option<HashMap<String, String>>,
    /// What the user called the server (flag text or config name).
    pub label: &'a str,
    /// The id recorded for the model-facing failure note.
    pub failure_id: &'a str,
    /// `None`: the server's own tool names (nothing collided).
    pub prefix: Option<&'a str>,
    /// The server's own names that forced the prefix (empty when `None`).
    pub colliding: &'a [String],
}

/// Connect one stdio server through [`mcp_tool_source`] and add it to
/// `agent`, prefixed or not. The one connect path for both `--mcp` and
/// `[mcp_servers.*]`, colliding or not.
///
/// Unlike `with_mcp_server_stdio`, a failure here does NOT consume the
/// agent, so no earlier connection is lost and there is nothing to rebuild
/// (and no "connections lost" note: nothing was lost); the failure is printed
/// and recorded for the model via `record_failed_server`, the same path as
/// every other connect failure (the model must not be handed an absence). On
/// success the final names join `registered` and `mcp_count` grows by one.
pub(crate) async fn connect_mcp(
    agent: Agent,
    spec: McpConnect<'_>,
    registered: &mut Vec<String>,
    mcp_count: &mut u32,
) -> Agent {
    let shown_label = crate::cli::sanitize_for_display(spec.label);
    match mcp_tool_source(spec.command, spec.args, spec.env, spec.prefix).await {
        Ok((source, names, handle)) => {
            register_live_server(&LIVE_MCP_SERVERS, spec.failure_id, &names, handle);
            registered.extend(names);
            *mcp_count += 1;
            match spec.prefix {
                Some(prefix) => {
                    let plain = crate::format::is_plain_output();
                    eprintln!(
                        "{YELLOW}  {}{RESET}",
                        prefix_applied_message(spec.label, prefix, spec.colliding, plain)
                    );
                    eprintln!("{GREEN}  ✓ mcp: {shown_label} connected (prefix '{prefix}'){RESET}");
                }
                None => eprintln!("{GREEN}  ✓ mcp: {shown_label} connected{RESET}"),
            }
            agent.with_tool_source(source)
        }
        Err(e) => {
            let e = crate::cli::sanitize_for_display(&e.to_string());
            eprintln!("{RED}  ✗ mcp: failed to connect to '{shown_label}': {e}{RESET}");
            record_failed_server("mcp", spec.failure_id);
            agent
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_builder::BUILTIN_TOOL_NAMES;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// The flagship reference server's tool set (the case #1041 is about).
    fn filesystem_server() -> Vec<String> {
        names(&[
            "read_file",
            "read_multiple_files",
            "write_file",
            "edit_file",
            "create_directory",
            "list_directory",
            "directory_tree",
            "move_file",
            "search_files",
            "get_file_info",
            "list_allowed_directories",
        ])
    }

    #[test]
    fn filesystem_server_is_prefixed_not_skipped() {
        let tools = filesystem_server();
        let plan = plan_mcp_connect(&tools, BUILTIN_TOOL_NAMES, &[], "fs");
        assert_eq!(
            plan,
            McpConnectPlan::Prefixed {
                prefix: "fs".to_string(),
                colliding: names(&["read_file", "write_file", "edit_file"]),
            }
        );
        for t in &tools {
            let final_name = prefixed_tool_name("fs", t);
            assert!(final_name.starts_with("fs__"));
            assert!(
                !BUILTIN_TOOL_NAMES.contains(&final_name.as_str()),
                "{final_name} collides with a builtin"
            );
        }
    }

    #[test]
    fn non_colliding_server_stays_plain() {
        let tools = names(&["get_weather", "get_forecast"]);
        assert_eq!(
            plan_mcp_connect(&tools, BUILTIN_TOOL_NAMES, &[], "weather"),
            McpConnectPlan::Plain
        );
    }

    #[test]
    fn collision_with_an_earlier_server_also_prefixes() {
        let tools = names(&["search_docs"]);
        let earlier = names(&["search_docs"]);
        assert_eq!(
            plan_mcp_connect(&tools, BUILTIN_TOOL_NAMES, &earlier, "docs2"),
            McpConnectPlan::Prefixed {
                prefix: "docs2".to_string(),
                colliding: names(&["search_docs"]),
            }
        );
    }

    #[test]
    fn second_server_with_same_prefix_is_skipped() {
        let tools = filesystem_server();
        let first = plan_mcp_connect(&tools, BUILTIN_TOOL_NAMES, &[], "fs");
        assert!(matches!(first, McpConnectPlan::Prefixed { .. }));
        let registered: Vec<String> = tools.iter().map(|t| prefixed_tool_name("fs", t)).collect();
        let second = plan_mcp_connect(&tools, BUILTIN_TOOL_NAMES, &registered, "fs");
        assert_eq!(second, McpConnectPlan::Skip(registered.clone()));
    }

    #[test]
    fn sanitize_mcp_prefix_cases() {
        assert_eq!(sanitize_mcp_prefix("my server!"), "my_server_");
        assert_eq!(sanitize_mcp_prefix(""), "mcp");
        assert_eq!(sanitize_mcp_prefix("fs"), "fs");
        assert_eq!(sanitize_mcp_prefix("a-b_c9"), "a-b_c9");
        // Multibyte: one `_` per char, no byte slicing, no panic.
        assert_eq!(sanitize_mcp_prefix("✓é"), "__");
        let long = "é".repeat(100);
        assert_eq!(
            sanitize_mcp_prefix(&long).chars().count(),
            MCP_PREFIX_MAX_CHARS
        );
    }

    #[test]
    fn prefix_message_names_server_collisions_and_prefix() {
        let out = prefix_applied_message(
            "npx server-filesystem /tmp",
            "mcp1",
            &names(&["read_file", "write_file"]),
            false,
        );
        assert_eq!(
            out,
            "mcp: 'npx server-filesystem /tmp' exposes 'read_file', 'write_file', already used by yoyo \u{2014} connected with every tool of this server renamed to 'mcp1__<tool>'"
        );
    }

    #[test]
    fn prefix_message_is_glyph_free_when_plain() {
        let out = prefix_applied_message("fs", "fs", &names(&["read_file"]), true);
        assert!(!out.contains('•') && !out.contains('—'), "{out}");
        // Anti-vacuous: the non-plain form really does carry the em dash.
        let loud = prefix_applied_message("fs", "fs", &names(&["read_file"]), false);
        assert!(loud.contains('—'));
    }

    #[test]
    fn prefix_message_escapes_control_bytes_in_server_cmd() {
        let hostile = "evil\x1b[2Jcmd";
        assert!(
            hostile.as_bytes().contains(&0x1b),
            "fixture must be hostile"
        );
        let out = prefix_applied_message(hostile, "fs", &names(&["read_file"]), true);
        assert!(!out.as_bytes().contains(&0x1b), "{out:?}");
    }

    /// A stub MCP stdio server: answers `initialize` and `tools/list` (one
    /// tool, `ping`) with newline-delimited JSON-RPC echoing each request's
    /// `id`, exits on `tools/call`, and is killed by a watcher once the
    /// marker file `$1` exists. The watcher also stops when the server is
    /// gone, so a server that never sees its marker leaves no orphan.
    const STUB_SERVER: &str = r#"MARK="$1"
( while [ ! -e "$MARK" ] && kill -0 $$ 2>/dev/null; do sleep 0.05; done; kill $$ 2>/dev/null ) >/dev/null 2>&1 &
while IFS= read -r line; do
  id=$(printf '%s\n' "$line" | sed -n 's/.*"id": *\([0-9][0-9]*\).*/\1/p')
  case "$line" in
    *'"tools/call"'*) exit 0 ;;
    *'"tools/list"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"ping","description":"p","inputSchema":{"type":"object","properties":{}}}]}}\n' "$id" ;;
    *'"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":true}},"serverInfo":{"name":"stub","version":"1"}}}\n' "$id" ;;
  esac
done
"#;

    fn stub_server(dir: &std::path::Path) -> (String, String) {
        let script = dir.join("server.sh");
        std::fs::write(&script, STUB_SERVER).unwrap();
        let mark = dir.join("die");
        (
            script.to_string_lossy().into_owned(),
            mark.to_string_lossy().into_owned(),
        )
    }

    async fn listed(source: &Arc<dyn ToolSource>) -> Vec<String> {
        source
            .tools()
            .await
            .iter()
            .map(|t| t.name().to_string())
            .collect()
    }

    /// The seam this module owns: what yoyo installs on the agent. A server
    /// that exits mid-session must have its tools withdrawn at the next ask
    /// (the next run), not keep advertising a dead server. Polled because the
    /// exit is seen asynchronously; a list-once source never empties and
    /// fails at the deadline.
    #[tokio::test]
    async fn a_server_that_exits_has_its_tools_withdrawn() {
        let dir = tempfile::tempdir().unwrap();
        let (script, mark) = stub_server(dir.path());
        let (source, names, _) = mcp_tool_source("sh", &[&script, &mark], None, None)
            .await
            .expect("stub server connects");
        assert_eq!(names, vec!["ping".to_string()]);
        assert_eq!(listed(&source).await, vec!["ping".to_string()]);

        std::fs::write(&mark, "").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut now = listed(&source).await;
        while !now.is_empty() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            now = listed(&source).await;
        }
        assert!(
            now.is_empty(),
            "a dead server's tools must be withdrawn, still listed: {now:?}"
        );
    }

    /// Near-miss: a server that stays alive keeps its tool across two asks,
    /// and the prefix reaches the names the model sees.
    #[tokio::test]
    async fn a_live_server_keeps_its_tools_across_asks() {
        let dir = tempfile::tempdir().unwrap();
        let (script, mark) = stub_server(dir.path());
        let (source, names, _) = mcp_tool_source("sh", &[&script, &mark], None, Some("fs"))
            .await
            .expect("stub server connects");
        assert_eq!(names, vec!["fs__ping".to_string()]);
        assert_eq!(listed(&source).await, vec!["fs__ping".to_string()]);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(listed(&source).await, vec!["fs__ping".to_string()]);
        assert!(
            !std::path::Path::new(&mark).exists(),
            "fixture never killed it"
        );
    }

    // ---- #1041: a server that exits mid-session is told, once ----

    fn exited(id: &str, reason: &str, tools: &[&str]) -> ExitedServer {
        ExitedServer {
            id: id.to_string(),
            reason: reason.to_string(),
            tools: names(tools),
        }
    }

    /// A fake client: `Some(reason)` once its shared flag is set.
    type FakeClient = Arc<std::sync::Mutex<Option<String>>>;

    fn fake_exited(c: &FakeClient) -> Option<String> {
        c.lock().unwrap().clone()
    }

    fn fake_store(
        ids: &[&str],
    ) -> (
        std::sync::Mutex<Vec<LiveServer<FakeClient>>>,
        Vec<FakeClient>,
    ) {
        let store = std::sync::Mutex::new(Vec::new());
        let mut clients = Vec::new();
        for id in ids {
            let c: FakeClient = Arc::new(std::sync::Mutex::new(None));
            register_live_server(&store, id, &names(&["ping", "pong"]), Arc::clone(&c));
            clients.push(c);
        }
        (store, clients)
    }

    #[test]
    fn exited_note_is_none_for_no_exits() {
        assert_eq!(mcp_exited_note(&[]), None);
    }

    #[test]
    fn exited_note_names_server_reason_tools_and_unavailable() {
        let note = mcp_exited_note(&[exited(
            "github (npx server-github)",
            "server exited with exit status: 1",
            &["gh__create_issue", "gh__list_prs"],
        )])
        .expect("an exit must speak");
        assert!(note.contains("github (npx server-github)"), "{note}");
        assert!(note.contains("exit status: 1"), "{note}");
        assert!(note.contains("`gh__create_issue`"), "{note}");
        assert!(note.contains("`gh__list_prs`"), "{note}");
        assert!(note.contains("no longer available"), "{note}");
        assert!(note.contains("unavailable and name the server"), "{note}");
        assert!(note.contains("1 MCP server that"), "singular: {note}");
    }

    #[test]
    fn exited_note_caps_long_tool_list_with_honest_count() {
        let many: Vec<String> = (0..25).map(|i| format!("t{i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        let note = mcp_exited_note(&[exited("big", "gone", &refs)]).unwrap();
        assert!(note.contains("`t9`"), "{note}");
        assert!(!note.contains("`t10`"), "{note}");
        assert!(note.contains("and 15 more"), "{note}");
        // Exactly at the cap: no "more" clause.
        let ten: Vec<&str> = refs[..10].to_vec();
        let at_cap = mcp_exited_note(&[exited("big", "gone", &ten)]).unwrap();
        assert!(!at_cap.contains("more"), "{at_cap}");
    }

    #[test]
    fn exited_note_survives_hostile_multibyte_id() {
        let id = "✓".repeat(300) + "\x1b[2J";
        let note = mcp_exited_note(&[exited(&id, "é".repeat(300).as_str(), &["x"])]).unwrap();
        assert!(note.contains("bytes elided"), "the long id is capped");
        let line = mcp_exited_user_line(&exited(&id, "boom", &["x"]), true);
        assert!(id.as_bytes().contains(&0x1b), "fixture must be hostile");
        assert!(!line.as_bytes().contains(&0x1b), "{line:?}");
    }

    #[test]
    fn exited_user_line_counts_tools_and_is_glyph_free_when_plain() {
        let s = exited("fs", "server exited with exit status: 0", &["a", "b", "c"]);
        assert_eq!(
            mcp_exited_user_line(&s, true),
            "mcp: server 'fs' exited (server exited with exit status: 0): 3 tools no longer available"
        );
        assert!(mcp_exited_user_line(&s, false).starts_with('⚠'));
        let one = exited("fs", "gone", &["a"]);
        assert!(mcp_exited_user_line(&one, true).ends_with("1 tool no longer available"));
    }

    #[test]
    fn drain_reports_an_exit_once_on_the_turn_it_happens() {
        let (store, clients) = fake_store(&["fs"]);
        assert_eq!(
            take_mcp_exited_note_with(&store, fake_exited),
            None,
            "turn 1: alive"
        );
        *clients[0].lock().unwrap() = Some("server exited with signal: 9".to_string());
        let turn2 = take_mcp_exited_note_with(&store, fake_exited).expect("turn 2: exited");
        assert!(
            turn2.contains("fs") && turn2.contains("signal: 9"),
            "{turn2}"
        );
        assert_eq!(
            take_mcp_exited_note_with(&store, fake_exited),
            None,
            "turn 3: never again"
        );
    }

    #[test]
    fn drain_names_only_the_server_that_died() {
        let (store, clients) = fake_store(&["alive-server", "dead-server"]);
        *clients[1].lock().unwrap() = Some("gone".to_string());
        let note = take_mcp_exited_note_with(&store, fake_exited).unwrap();
        assert!(note.contains("dead-server"), "{note}");
        assert!(!note.contains("alive-server"), "{note}");
    }

    /// Near-miss, at the emission point: an empty store and an all-alive store
    /// leave the turn text byte-identical.
    #[test]
    fn no_exits_leave_the_prompt_byte_identical() {
        let empty: std::sync::Mutex<Vec<LiveServer<FakeClient>>> =
            std::sync::Mutex::new(Vec::new());
        let (alive, _clients) = fake_store(&["fs", "gh"]);
        for store in [&empty, &alive] {
            let note = join_notes(None, take_mcp_exited_note_with(store, fake_exited));
            let out = crate::prompt::turn_prefix::apply_external_failure_note_with(
                note,
                "fix the bug".to_string(),
            );
            assert_eq!(out, "fix the bug");
        }
    }

    #[test]
    fn join_notes_keeps_both_and_drops_neither() {
        assert_eq!(join_notes(None, None), None);
        assert_eq!(join_notes(Some("a".into()), None), Some("a".into()));
        assert_eq!(join_notes(None, Some("b".into())), Some("b".into()));
        assert_eq!(
            join_notes(Some("a".into()), Some("b".into())),
            Some("a\n\nb".into())
        );
    }

    /// The production predicate against a real process: the stub server
    /// connects, exits, and `client_exited` sees it — the drain then names it.
    #[tokio::test]
    async fn a_real_server_exit_reaches_the_note() {
        let dir = tempfile::tempdir().unwrap();
        let (script, mark) = stub_server(dir.path());
        let (_source, names, handle) = mcp_tool_source("sh", &[&script, &mark], None, None)
            .await
            .expect("stub server connects");
        let store = std::sync::Mutex::new(Vec::new());
        register_live_server(&store, "stub (sh)", &names, handle);
        assert_eq!(
            take_mcp_exited_note_with(&store, client_exited),
            None,
            "alive"
        );

        std::fs::write(&mark, "").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut note = None;
        while note.is_none() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            note = take_mcp_exited_note_with(&store, client_exited);
        }
        let note = note.expect("a dead server must be reported");
        assert!(
            note.contains("stub (sh)") && note.contains("`ping`"),
            "{note}"
        );
        assert_eq!(
            take_mcp_exited_note_with(&store, client_exited),
            None,
            "once only"
        );
    }
}
