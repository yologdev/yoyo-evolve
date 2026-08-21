//! Interactive REPL loop and related helpers (tab-completion, multi-line input).

use std::time::Instant;

use crate::cli::*;
use crate::commands::{self, auto_compact_if_needed, command_arg_completions, KNOWN_COMMANDS};
use crate::commands_goal::run_goal_verify_after_prompt;
use crate::conversations::build_add_content_blocks;
use crate::dispatch::CommandResult;
use crate::format::*;
use crate::git::*;
use crate::prompt::{run_prompt_auto_retry, run_prompt_auto_retry_with_content, PromptOutcome};
use crate::session::{format_turn_changes, SessionChanges, TurnHistory, TurnSnapshot};
use crate::watch::{get_watch_command, run_watch_after_prompt};
use crate::AgentConfig;

/// Configuration for the REPL session, bundling the positional arguments
/// that don't need mutable borrow semantics.
pub struct ReplConfig {
    pub mcp_count: u32,
    pub openapi_count: u32,
    pub continue_session: bool,
    pub update_available: Option<String>,
    pub mcp_cli_servers: Vec<String>,
    pub mcp_server_configs: Vec<McpServerConfig>,
}

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::Editor;
use yoagent::*;

/// Rustyline helper that provides tab-completion for `/` slash commands.
pub struct YoyoHelper;

impl Completer for YoyoHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let prefix = &line[..pos];

        // Slash command completion: starts with '/' and no space yet
        if prefix.starts_with('/') && !prefix.contains(' ') {
            let mut matches: Vec<Pair> = KNOWN_COMMANDS
                .iter()
                .filter(|cmd| cmd.starts_with(prefix))
                .map(|cmd| {
                    let cmd_name = &cmd[1..]; // strip leading /
                    let desc = crate::help::command_short_description(cmd_name).unwrap_or("");
                    if desc.is_empty() {
                        Pair {
                            display: cmd.to_string(),
                            replacement: cmd.to_string(),
                        }
                    } else {
                        Pair {
                            display: format!("{cmd:<14} {desc}"),
                            replacement: cmd.to_string(),
                        }
                    }
                })
                .collect();

            // Add custom commands from .yoyo/commands/ and ~/.yoyo/commands/
            for name in crate::commands::custom_command_names() {
                let slash_name = format!("/{name}");
                if slash_name.starts_with(prefix) {
                    matches.push(Pair {
                        display: format!("{slash_name:<14} (custom)"),
                        replacement: slash_name,
                    });
                }
            }

            return Ok((0, matches));
        }

        // Argument-aware completion: command + space + partial arg
        if prefix.starts_with('/') {
            if let Some(space_pos) = prefix.find(' ') {
                let cmd = &prefix[..space_pos];
                let arg_part = &prefix[space_pos + 1..];
                // Only complete the first argument (no nested spaces)
                if !arg_part.contains(' ') {
                    let candidates = command_arg_completions(cmd, arg_part);
                    if !candidates.is_empty() {
                        let pairs = candidates
                            .into_iter()
                            .map(|c| Pair {
                                display: c.clone(),
                                replacement: c,
                            })
                            .collect();
                        return Ok((space_pos + 1, pairs));
                    }
                }
            }
        }

        // File path completion: extract the last whitespace-delimited word
        let word_start = prefix.rfind(char::is_whitespace).map_or(0, |i| i + 1);
        let word = &prefix[word_start..];
        if word.is_empty() {
            return Ok((pos, Vec::new()));
        }

        let matches = complete_file_path(word)
            .into_iter()
            .map(|p| Pair {
                display: p.clone(),
                replacement: p,
            })
            .collect();
        Ok((word_start, matches))
    }
}

/// Complete a partial file path by listing directory entries that match.
/// Appends `/` to directory names for easy continued completion.
pub fn complete_file_path(partial: &str) -> Vec<String> {
    use std::path::Path;

    let path = Path::new(partial);

    // Determine the directory to scan and the filename prefix to match
    let (dir, file_prefix) =
        if partial.ends_with('/') || partial.ends_with(std::path::MAIN_SEPARATOR) {
            // User typed "src/" — list everything inside src/
            (partial.to_string(), String::new())
        } else if let Some(parent) = path.parent() {
            let parent_str = if parent.as_os_str().is_empty() {
                ".".to_string()
            } else {
                parent.to_string_lossy().to_string()
            };
            let file_prefix = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            (parent_str, file_prefix)
        } else {
            (".".to_string(), partial.to_string())
        };

    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let dir_prefix = if dir == "." && !partial.contains('/') {
        String::new()
    } else if partial.ends_with('/') || partial.ends_with(std::path::MAIN_SEPARATOR) {
        partial.to_string()
    } else {
        let parent = path.parent().unwrap_or(Path::new(""));
        if parent.as_os_str().is_empty() {
            String::new()
        } else {
            format!("{}/", parent.display())
        }
    };

    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(&file_prefix) {
            continue;
        }
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let candidate = if is_dir {
            format!("{}{}/", dir_prefix, name)
        } else {
            format!("{}{}", dir_prefix, name)
        };
        matches.push(candidate);
    }
    matches.sort();
    matches
}

impl Hinter for YoyoHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<String> {
        // Only hint when cursor is at the end of the line
        if pos != line.len() {
            return None;
        }
        // Only hint for slash commands
        if !line.starts_with('/') {
            return None;
        }
        let typed = &line[1..]; // strip the leading /
        if typed.is_empty() {
            return None; // Don't hint on bare "/"
        }
        // If user typed a command + space, show argument hints
        if typed.contains(' ') {
            if let Some((cmd_part, arg_part)) = typed.split_once(' ') {
                if arg_part.is_empty() {
                    // User just typed "/cmd " — show available args
                    if let Some(hint) = crate::commands::command_arg_hint(cmd_part) {
                        return Some(hint.to_string());
                    }
                }
            }
            return None;
        }
        // Find the first matching command
        for cmd in KNOWN_COMMANDS {
            let cmd_name = &cmd[1..]; // strip leading /
            if cmd_name.starts_with(typed) && cmd_name != typed {
                // Show the rest of the command + description
                let rest = &cmd_name[typed.len()..];
                if let Some(desc) = crate::help::command_short_description(cmd_name) {
                    return Some(format!("{rest} — {desc}"));
                } else {
                    return Some(rest.to_string());
                }
            }
        }
        // If user typed a complete command name, show its description
        for cmd in KNOWN_COMMANDS {
            let cmd_name = &cmd[1..];
            if cmd_name == typed {
                if let Some(desc) = crate::help::command_short_description(cmd_name) {
                    return Some(format!(" — {desc}"));
                }
            }
        }
        // Check custom commands for hints
        for name in crate::commands::custom_command_names() {
            if name.starts_with(typed) && name != typed {
                let rest = &name[typed.len()..];
                return Some(format!("{rest} (custom)"));
            }
        }
        if crate::commands::is_custom_command(typed) {
            return Some(" (custom)".to_string());
        }
        None
    }
}

impl Highlighter for YoyoHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        // Show hints in dim text
        std::borrow::Cow::Owned(format!("\x1b[2m{hint}\x1b[0m"))
    }
}

impl Validator for YoyoHelper {}

impl rustyline::Helper for YoyoHelper {}

/// Check if a line needs continuation (backslash at end, or opens a code fence).
pub fn needs_continuation(line: &str) -> bool {
    line.ends_with('\\') || line.starts_with("```")
}

/// Parse a `!` shell-passthrough line.
///
/// Returns `Some(cmd)` when the trimmed line starts with `!`: the command is
/// everything after the `!`, trimmed. A bare `!` (or `!` + whitespace only)
/// returns `Some("")` so the caller can print a usage hint. Lines not starting
/// with `!` return `None`. Uses only char-safe operations (no byte indexing).
pub fn parse_bang_command(line: &str) -> Option<&str> {
    line.trim().strip_prefix('!').map(str::trim)
}

/// Parse a `!?` follow-through line.
///
/// Returns `Some(question)` when the trimmed line starts with `!?`: the
/// question is everything after the `!?`, trimmed (bare `!?` returns
/// `Some("")` so the caller can substitute a default question). Lines not
/// starting with `!?` — including plain `!` passthrough lines — return `None`.
/// Callers MUST check this BEFORE [`parse_bang_command`], otherwise `!?`
/// would be executed as the shell command `?`.
pub fn parse_bang_query(line: &str) -> Option<&str> {
    line.trim().strip_prefix("!?").map(str::trim)
}

/// Result of the most recent `!` passthrough command, kept so `!?` can feed
/// it into the conversation on demand.
#[derive(Clone, Debug)]
pub struct BangResult {
    pub command: String,
    /// `None` when the command was terminated by a signal (or the exit
    /// status was unavailable).
    pub exit_code: Option<i32>,
    /// Combined stdout+stderr tail, capped by [`tail_for_capture`].
    pub output_tail: String,
}

/// The last `!` command's captured result. Read (non-consuming) by `!?` so
/// the user can ask follow-up questions more than once.
static LAST_BANG_RESULT: std::sync::Mutex<Option<BangResult>> = std::sync::Mutex::new(None);

/// Snapshot of the last `!` command's result, if any.
pub fn last_bang_result() -> Option<BangResult> {
    crate::sync_util::lock_or_recover(&LAST_BANG_RESULT).clone()
}

/// Max bytes of `!` output kept for `!?` follow-through (~8KB tail).
const BANG_CAPTURE_MAX_BYTES: usize = 8 * 1024;
/// Max lines of `!` output kept for `!?` follow-through.
const BANG_CAPTURE_MAX_LINES: usize = 200;
/// In-flight capture buffer high-water mark; when exceeded, the front is
/// drained down to roughly the final tail size so a chatty command can't
/// grow memory unboundedly while it runs.
const BANG_CAPTURE_HIGH_WATER: usize = 32 * 1024;

/// Return the last `max_lines` lines of `s`, further capped to the last
/// `max_bytes` bytes. UTF-8 safe: line cuts land just after `\n` (always a
/// char boundary) and the byte cut is advanced to the nearest char boundary
/// (see the byte-indexing rule in CLAUDE.md — #250).
pub fn tail_for_capture(s: &str, max_bytes: usize, max_lines: usize) -> String {
    if max_bytes == 0 || max_lines == 0 {
        return String::new();
    }
    // Line cap: walk newlines from the end. A trailing '\n' terminates the
    // last line rather than starting a new one, so skip it when counting.
    let body = s.strip_suffix('\n').unwrap_or(s);
    let mut start = 0;
    let mut newlines = 0;
    for (i, b) in body.as_bytes().iter().enumerate().rev() {
        if *b == b'\n' {
            newlines += 1;
            if newlines == max_lines {
                start = i + 1;
                break;
            }
        }
    }
    let tail = &s[start..];
    // Byte cap on what's left.
    if tail.len() <= max_bytes {
        return tail.to_string();
    }
    let mut cut = tail.len() - max_bytes;
    while cut < tail.len() && !tail.is_char_boundary(cut) {
        cut += 1;
    }
    tail[cut..].to_string()
}

/// Build the conversation prompt for a `!?` follow-through. A bare `!?`
/// (empty `question`) gets a default question shaped by the exit status:
/// failures ask for an explanation and fix, successes ask for a summary.
pub fn build_bang_followup_prompt(res: &BangResult, question: &str) -> String {
    let exit_desc = match res.exit_code {
        Some(0) => "exited successfully (exit 0)".to_string(),
        Some(c) => format!("failed with exit code {c}"),
        None => "was terminated by a signal".to_string(),
    };
    let question = if question.is_empty() {
        match res.exit_code {
            Some(0) => "Summarize this output and point out anything notable.",
            _ => "This command failed. Explain what went wrong and how to fix it.",
        }
    } else {
        question
    };
    let output = if res.output_tail.trim().is_empty() {
        "(no output)".to_string()
    } else {
        format!("```\n{}\n```", res.output_tail.trim_end())
    };
    format!(
        "I ran this shell command in my terminal (via `!` passthrough):\n\n    $ {}\n\nIt {}. Output (tail):\n\n{}\n\n{}",
        res.command, exit_desc, output, question
    )
}

/// Tee a child stream to the terminal while accumulating a bounded capture
/// buffer. Terminal write failures are deliberately ignored (a closed pipe
/// on our side shouldn't kill the capture); capture continues regardless.
fn tee_stream<R: std::io::Read, W: std::io::Write>(
    mut src: R,
    mut dst: W,
    capture: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
) {
    let mut buf = [0u8; 8192];
    loop {
        match src.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if dst.write_all(&buf[..n]).is_ok() {
                    let _ = dst.flush();
                }
                let mut cap = crate::sync_util::lock_or_recover(&capture);
                cap.extend_from_slice(&buf[..n]);
                if cap.len() > BANG_CAPTURE_HIGH_WATER {
                    let drop_bytes = cap.len() - BANG_CAPTURE_MAX_BYTES;
                    cap.drain(..drop_bytes);
                }
            }
        }
    }
}

/// Run a shell command directly for `!` passthrough. Output streams live to
/// the terminal (tee'd, not redirected away) while a bounded tail is captured
/// for `!?` follow-through. On non-zero exit, prints the exit code plus a
/// one-line `!?` hint; success stays silent. Never panics on spawn failure —
/// prints the error and returns.
fn run_bang_command(cmd: &str) {
    use std::process::Stdio;

    #[cfg(not(windows))]
    let spawned = std::process::Command::new("sh")
        .args(["-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    #[cfg(windows)]
    let spawned = std::process::Command::new("cmd")
        .args(["/C", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}failed to run command: {e}{RESET}");
            return;
        }
    };

    let capture = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        let cap = std::sync::Arc::clone(&capture);
        readers.push(std::thread::spawn(move || {
            tee_stream(out, std::io::stdout(), cap)
        }));
    }
    if let Some(err) = child.stderr.take() {
        let cap = std::sync::Arc::clone(&capture);
        readers.push(std::thread::spawn(move || {
            tee_stream(err, std::io::stderr(), cap)
        }));
    }

    let status = child.wait();
    for r in readers {
        let _ = r.join(); // reader threads never panic; join keeps output ordered before the exit line
    }

    let captured = {
        let cap = crate::sync_util::lock_or_recover(&capture);
        String::from_utf8_lossy(&cap).into_owned()
    };
    let exit_code = match &status {
        Ok(s) => s.code(),
        Err(_) => None,
    };
    *crate::sync_util::lock_or_recover(&LAST_BANG_RESULT) = Some(BangResult {
        command: cmd.to_string(),
        exit_code,
        output_tail: tail_for_capture(&captured, BANG_CAPTURE_MAX_BYTES, BANG_CAPTURE_MAX_LINES),
    });

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            let code = s
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string());
            eprintln!(
                "{DIM}command failed (exit {code}) — type !? to ask yoyo about the output{RESET}"
            );
        }
        Err(e) => {
            eprintln!("{RED}failed to run command: {e}{RESET}");
        }
    }
}

/// Collect multi-line input using rustyline (for interactive REPL mode).
/// Same logic as `collect_multiline` but uses rustyline's readline for continuation prompts.
pub fn collect_multiline_rl(
    first_line: &str,
    rl: &mut Editor<YoyoHelper, DefaultHistory>,
) -> String {
    let mut buf = String::new();
    let cont_prompt = format!("{DIM}  ...{RESET} ");

    if first_line.starts_with("```") {
        // Code fence mode: collect until closing ```
        buf.push_str(first_line);
        buf.push('\n');
        while let Ok(line) = rl.readline(&cont_prompt) {
            buf.push_str(&line);
            buf.push('\n');
            if line.trim() == "```" {
                break;
            }
        }
    } else {
        // Backslash continuation mode
        let mut current = first_line.to_string();
        loop {
            if current.ends_with('\\') {
                current.truncate(current.len() - 1);
                buf.push_str(&current);
                buf.push('\n');
                match rl.readline(&cont_prompt) {
                    Ok(line) => {
                        current = line;
                    }
                    _ => break,
                }
            } else {
                buf.push_str(&current);
                break;
            }
        }
    }

    buf
}

/// Run the architect-mode two-phase turn: plan with a strong model, then
/// implement with a cheaper editor model. Returns the editor's `PromptOutcome`
/// (or a default if the architect returned an empty plan).
async fn run_architect_turn(
    agent_config: &mut AgentConfig,
    effective_input: &str,
    session_total: &mut Usage,
    session_changes: &SessionChanges,
) -> PromptOutcome {
    let arch_model = commands::architect_model().unwrap_or_else(|| agent_config.model.clone());
    let editor_model = commands::default_editor_model(&arch_model);

    eprintln!("{DIM}  🏗️ architect mode: planning with {arch_model}...{RESET}");

    // Phase 1: Get the plan from the architect (no tools, text-only)
    let architect_input = format!("{}\n\n{}", commands::ARCHITECT_PROMPT, effective_input);
    let mut arch_agent = agent_config.build_architect_agent(&arch_model);
    let mut rx = arch_agent.prompt(&architect_input).await;

    let mut md_renderer = MarkdownRenderer::new();
    let mut plan_text = String::new();
    let mut started = false;

    loop {
        match rx.recv().await {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => {
                if !started {
                    eprintln!();
                    eprint!("{DIM}[architect]{RESET} ");
                    started = true;
                }
                plan_text.push_str(&delta);
                let rendered = md_renderer.render_delta(&delta);
                if !rendered.is_empty() {
                    print!("{rendered}");
                }
            }
            Some(AgentEvent::MessageEnd { .. }) => {
                let tail = md_renderer.flush();
                if !tail.is_empty() {
                    print!("{tail}");
                }
            }
            Some(AgentEvent::AgentEnd { .. }) => break,
            None => break,
            _ => {}
        }
    }

    arch_agent.finish().await;

    // Show architect cost
    let arch_messages = arch_agent.messages();
    let mut arch_usage = Usage::default();
    for msg in arch_messages {
        if let AgentMessage::Llm(yoagent::types::Message::Assistant { usage, .. }) = msg {
            arch_usage.input += usage.input;
            arch_usage.output += usage.output;
            arch_usage.cache_read += usage.cache_read;
            arch_usage.cache_write += usage.cache_write;
        }
    }
    let arch_tokens = arch_usage.input + arch_usage.output;
    if arch_tokens > 0 {
        let cost = estimate_cost(&arch_usage, &arch_model);
        if let Some(c) = cost {
            eprintln!(
                "\n{DIM}  [architect] {} tokens, ${:.4}{RESET}",
                arch_tokens, c
            );
        } else {
            eprintln!("\n{DIM}  [architect] {} tokens{RESET}", arch_tokens);
        }
    } else {
        eprintln!();
    }

    if plan_text.trim().is_empty() {
        eprintln!("{YELLOW}  architect returned empty plan — skipping editor phase{RESET}\n");
        return PromptOutcome::default();
    }

    // Phase 2: Feed the plan to the editor model for implementation
    eprintln!("{DIM}  🔧 implementing with {editor_model}...{RESET}\n");

    let editor_prompt = format!(
        "Implement the following plan exactly. The plan was written by an architect model \
         analyzing the user's request.\n\n\
         ## Original request\n\n{effective_input}\n\n\
         ## Architect's plan\n\n{plan_text}"
    );

    // Build a separate editor agent with the cheaper model
    let mut editor_agent = agent_config.build_editor_agent(&editor_model);
    let editor_outcome = run_prompt_auto_retry(
        &mut editor_agent,
        &editor_prompt,
        session_total,
        &editor_model,
        session_changes,
    )
    .await;

    // Show editor cost
    editor_agent.finish().await;
    let editor_messages = editor_agent.messages();
    let mut editor_usage = Usage::default();
    for msg in editor_messages {
        if let AgentMessage::Llm(yoagent::types::Message::Assistant { usage, .. }) = msg {
            editor_usage.input += usage.input;
            editor_usage.output += usage.output;
            editor_usage.cache_read += usage.cache_read;
            editor_usage.cache_write += usage.cache_write;
        }
    }
    let editor_tokens = editor_usage.input + editor_usage.output;
    if editor_tokens > 0 {
        let cost = estimate_cost(&editor_usage, &editor_model);
        if let Some(c) = cost {
            eprintln!("{DIM}  [editor] {} tokens, ${:.4}{RESET}", editor_tokens, c);
        } else {
            eprintln!("{DIM}  [editor] {} tokens{RESET}", editor_tokens);
        }
    }

    editor_outcome
}

/// Bundles the parameters for [`handle_post_prompt`] into a single struct,
/// eliminating a 13-parameter function signature.
struct PostPromptContext<'a> {
    outcome: &'a PromptOutcome,
    agent: &'a mut yoagent::agent::Agent,
    agent_config: &'a mut AgentConfig,
    session_total: &'a mut Usage,
    session_changes: &'a SessionChanges,
    turn_history: &'a mut TurnHistory,
    turn_snap: TurnSnapshot,
    changes_before: &'a [String],
    /// Monotonic edit count captured BEFORE this turn ran. `changes_before` is
    /// path-deduped and feeds the summary body; this answers "did this turn
    /// edit anything" (#774).
    edits_before: usize,
    last_error: &'a mut Option<String>,
    prompt_start: Instant,
    effective_input: &'a str,
    turn_count: usize,
    turns_since_slash_command: usize,
    spawn_tracker: &'a commands::SpawnTracker,
}

/// Post-prompt handling: bell, error tracking, fallback retry, turn snapshots,
/// watch-after-prompt, auto-commit, and auto-compact.
async fn handle_post_prompt(mut ctx: PostPromptContext<'_>) {
    crate::format::maybe_ring_bell(ctx.prompt_start.elapsed());
    *ctx.last_error = ctx.outcome.last_tool_error.clone();

    // Notify the user if the context was auto-compacted due to overflow
    if ctx.outcome.was_overflow {
        eprintln!("{YELLOW}  ℹ Context was auto-compacted (overflow detected){RESET}");
    }

    // Fallback provider: if the API failed and a fallback is configured, switch and retry
    if ctx.outcome.last_api_error.is_some() {
        let old_provider = ctx.agent_config.provider.clone();
        let fallback_name = ctx.agent_config.fallback_provider.clone();
        if ctx.agent_config.try_switch_to_fallback() {
            let fallback = fallback_name.as_deref().unwrap_or("unknown");
            eprintln!(
                "\n{YELLOW}  ⚡ Primary provider '{}' failed. Switching to fallback '{}'...{RESET}",
                old_provider, fallback
            );

            // Rebuild agent with the new provider
            *ctx.agent = ctx.agent_config.build_agent();

            eprintln!(
                "{DIM}  now using: {} / {}{RESET}\n",
                ctx.agent_config.provider, ctx.agent_config.model
            );

            // Retry the same prompt with the fallback provider
            let retry_outcome = run_prompt_auto_retry(
                ctx.agent,
                ctx.effective_input,
                ctx.session_total,
                &ctx.agent_config.model,
                ctx.session_changes,
            )
            .await;
            *ctx.last_error = retry_outcome.last_tool_error.clone();

            // If fallback also failed, restore original provider info for display
            // but keep the fallback agent since the original was already broken
            if retry_outcome.last_api_error.is_some() {
                eprintln!(
                    "{RED}  fallback provider '{}' also failed.{RESET}",
                    fallback
                );
                eprintln!(
                    "{DIM}  original provider was '{}'. Use /provider to switch manually.{RESET}",
                    old_provider
                );
            }
        }
    }

    // After the turn, find newly modified files and update the snapshot
    let changes_after: Vec<String> = ctx
        .session_changes
        .snapshot()
        .iter()
        .map(|c| c.path.clone())
        .collect();
    for path in &changes_after {
        if !ctx.changes_before.contains(path) {
            // This file was touched for the first time in this turn
            if ctx.turn_snap.originals.contains_key(path.as_str()) {
                // Already snapshotted (e.g., was in git diff) — keep the original
            } else if std::path::Path::new(path).exists() {
                // File was created during this turn
                ctx.turn_snap.record_created(path);
            }
        }
    }
    // Also check for new files from git that weren't in session_changes
    if let Ok(diff_files) = crate::git::run_git(&["diff", "--name-only"]) {
        for f in diff_files.lines().filter(|l| !l.is_empty()) {
            if !ctx.turn_snap.originals.contains_key(f) {
                ctx.turn_snap.snapshot_file(f);
            }
        }
    }
    ctx.turn_history.push(ctx.turn_snap);

    // Edits, not distinct files: turn 2 re-editing the same file still worked.
    let files_modified = ctx.session_changes.edit_count() > ctx.edits_before;

    // Show a compact summary of files changed in this turn
    if files_modified && !is_quiet() {
        let summary = format_turn_changes(ctx.changes_before, ctx.session_changes);
        if !summary.is_empty() {
            eprintln!("{DIM}{summary}{RESET}");
        }
    }

    // ── Contextual hint: teach users about relevant commands ─────────
    if !is_quiet() {
        let ctx_max = crate::cli_config::effective_context_tokens();
        let usage_ratio = if ctx_max > 0 {
            ctx.session_total.total_tokens as f64 / ctx_max as f64
        } else {
            0.0
        };
        let hint_ctx = crate::format::HintContext {
            turn_count: ctx.turn_count,
            files_modified,
            has_watch: get_watch_command().is_some(),
            had_tool_error: ctx.outcome.last_tool_error.is_some(),
            context_usage_ratio: usage_ratio,
            turns_since_slash_command: ctx.turns_since_slash_command,
        };
        if let Some(hint) = crate::format::contextual_hint(&hint_ctx) {
            eprintln!("{DIM}  {hint}{RESET}");
        }

        // Surface any background spawns that finished since the last turn.
        for line in ctx.spawn_tracker.newly_finished_background() {
            eprintln!("{DIM}  {line}{RESET}");
        }
    }

    if files_modified {
        let watch_result = run_watch_after_prompt(
            ctx.agent,
            ctx.session_total,
            &ctx.agent_config.model,
            ctx.session_changes,
        )
        .await;
        if !watch_result.passed {
            *ctx.last_error = watch_result.last_tool_error;
        }
    }

    // ── Auto goal-verify: check if the goal is met after each turn ──────
    run_goal_verify_after_prompt();

    // ── Auto-commit: stage and commit if flag is on and files changed ─────
    if ctx.agent_config.auto_commit && files_modified {
        let _ = run_git(&["add", "-A"]);
        if let Some(diff) = get_staged_diff() {
            if !diff.trim().is_empty() {
                let msg = generate_commit_message(&diff);
                let (ok, output) = run_git_commit(&msg);
                if ok {
                    eprintln!("{GREEN}  ✓ Auto-committed: {}{RESET}", output.trim());
                } else {
                    eprintln!("{DIM}  (auto-commit failed: {}){RESET}", output.trim());
                }
            }
        }
    }

    // Auto-compact when context window is getting full
    auto_compact_if_needed(ctx.agent);
}

/// Print the startup banner and all session configuration info.
fn print_startup_info(agent_config: &AgentConfig, repl_config: &ReplConfig) {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    print_banner();
    if agent_config.provider != "anthropic" {
        println!("{DIM}  provider: {}{RESET}", agent_config.provider);
    }
    println!("{DIM}  model: {}{RESET}", agent_config.model);
    if let Some(ref url) = agent_config.base_url {
        println!("{DIM}  base_url: {url}{RESET}");
    }
    if agent_config.thinking != ThinkingLevel::Off {
        println!("{DIM}  thinking: {:?}{RESET}", agent_config.thinking);
    }
    if let Some(temp) = agent_config.temperature {
        println!("{DIM}  temperature: {temp:.1}{RESET}");
    }
    if !agent_config.skills.is_empty() {
        let total = agent_config.skills.len();
        let auto = crate::cli::auto_discovered_skill_count();
        if auto > 0 && auto < total {
            println!("{DIM}  skills: {total} loaded ({auto} auto-discovered){RESET}");
        } else if auto > 0 {
            println!("{DIM}  skills: {total} loaded (auto-discovered){RESET}");
        } else {
            println!("{DIM}  skills: {total} loaded{RESET}");
        }
    }
    if repl_config.mcp_count > 0 {
        println!(
            "{DIM}  mcp: {} server(s) connected{RESET}",
            repl_config.mcp_count
        );
    }
    if repl_config.openapi_count > 0 {
        println!(
            "{DIM}  openapi: {} spec(s) loaded{RESET}",
            repl_config.openapi_count
        );
    }
    if is_verbose() {
        println!("{DIM}  verbose: on{RESET}");
    }
    if !agent_config.auto_approve {
        println!("{DIM}  tools: confirmation required (use --yes to skip){RESET}");
    }
    if !agent_config.permissions.is_empty() {
        println!(
            "{DIM}  permissions: {} allow, {} deny pattern(s){RESET}",
            agent_config.permissions.allow.len(),
            agent_config.permissions.deny.len()
        );
    }
    if let Some(ref fallback) = agent_config.fallback_provider {
        println!("{DIM}  fallback: {fallback}{RESET}");
    }
    if let Some(branch) = git_branch() {
        println!("{DIM}  git:   {branch}{RESET}");
    }
    println!("{DIM}  cwd:   {cwd}{RESET}\n");

    // Show update notification if a newer version is available
    if let Some(ref latest) = repl_config.update_available {
        println!(
            "  {YELLOW}⬆ Update available: v{latest} (you have v{VERSION}) — https://github.com/yologdev/yoyo-evolve/releases{RESET}\n"
        );
    }

    // Hint about previous session if one exists and --continue wasn't used
    if !repl_config.continue_session {
        if let Some(hint) = crate::banner::session_resume_hint() {
            println!("{hint}\n");
        }
    }

    // Auto-enable watch mode if a project type is detected and config allows it
    if get_watch_command().is_none() && agent_config.auto_watch {
        crate::watch::arm_auto_watch("/watch off or auto_watch = false", false);
    } else if get_watch_command().is_none() && !agent_config.auto_watch {
        crate::watch::hint_auto_watch_available();
    }
}

/// Returns when the user exits (via /quit, /exit, Ctrl-D, etc.).
pub async fn run_repl(
    agent_config: &mut AgentConfig,
    agent: &mut yoagent::agent::Agent,
    repl_config: ReplConfig,
) {
    print_startup_info(agent_config, &repl_config);

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    // Set up rustyline editor with slash-command tab-completion
    let config = rustyline::config::Builder::new()
        .completion_type(rustyline::config::CompletionType::List)
        .completion_prompt_limit(50)
        .build();
    let mut rl = Editor::with_config(config).expect("Failed to initialize readline");
    rl.set_helper(Some(YoyoHelper));
    if let Some(history_path) = history_file_path() {
        if rl.load_history(&history_path).is_err() {
            // First run or history file doesn't exist yet — that's fine
        }
    }

    let mut session_total = Usage::default();
    let session_start = Instant::now();
    let mut turn_count: usize = 0;
    let mut turns_since_slash_command: usize = 0;
    let mut last_input: Option<String> = None;
    let mut last_error: Option<String> = None;
    let mut bookmarks = commands::Bookmarks::new();
    let session_changes = SessionChanges::new();
    let mut turn_history = TurnHistory::new();
    let spawn_tracker = commands::SpawnTracker::new();
    let bg_tracker = commands::BackgroundJobTracker::new();
    let mut undo_context: Option<String> = None;
    let mut checkpoint_store = commands::CheckpointStore::new();

    // Load config for auto-continue settings (re-read each turn for live updates)
    let repl_file_config = crate::config::load_config_file().0;

    // Double-Ctrl+C exit (#689): armed by one Ctrl+C at the idle prompt,
    // cleared by any other event. Consecutive-press semantics, no timers.
    let mut ctrl_c_armed = false;

    loop {
        // Build mode indicators
        let mode_indicators = {
            let mut indicators = String::new();
            if commands::is_plan_mode() {
                indicators.push_str(&format!("{BOLD}{YELLOW}📋{RESET} "));
            }
            if commands::is_read_mode() {
                indicators.push_str(&format!("{BOLD}{YELLOW}🔍{RESET} "));
            }
            if commands::is_architect_mode() {
                indicators.push_str(&format!("{BOLD}{YELLOW}🏗️{RESET} "));
            }
            indicators
        };

        let prompt = if let Some(branch) = git_branch() {
            format!("{BOLD}{GREEN}{branch}{RESET} {mode_indicators}{BOLD}{GREEN}🐙 › {RESET}")
        } else {
            format!("{mode_indicators}{BOLD}{GREEN}🐙 › {RESET}")
        };

        let line = match rl.readline(&prompt) {
            Ok(l) => {
                ctrl_c_armed = false;
                l
            }
            Err(ReadlineError::Interrupted) => {
                if ctrl_c_armed {
                    // Second consecutive Ctrl+C: exit like Ctrl+D / /quit
                    // (same break, so session auto-save etc. still runs).
                    println!();
                    break;
                }
                ctrl_c_armed = true;
                // Ctrl+C: cancel current line, hint at the exit, re-prompt
                println!("{DIM}(press Ctrl+C again to exit){RESET}");
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: exit
                break;
            }
            Err(_) => break,
        };

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Add to readline history
        let _ = rl.add_history_entry(&line);

        // Multi-line input: collect continuation lines
        let input = if needs_continuation(input) {
            collect_multiline_rl(input, &mut rl)
        } else {
            input.to_string()
        };
        let input = input.trim();

        // `!?` follow-through: feed the last `!` command's output into the
        // conversation. Checked BEFORE the generic `!` passthrough, otherwise
        // `!?` would be executed as the shell command `?`.
        let bang_followup: Option<String> = match parse_bang_query(input) {
            Some(question) => match last_bang_result() {
                Some(res) => Some(build_bang_followup_prompt(&res, question)),
                None => {
                    println!(
                        "{DIM}no `!` command has run yet — run one first (e.g. `!cargo test`), then ask with !?{RESET}"
                    );
                    continue;
                }
            },
            None => None,
        };
        let input = match &bang_followup {
            Some(prompt) => prompt.as_str(),
            None => input,
        };

        // `!` shell passthrough: run the command directly, no API call, no cost.
        if let Some(cmd) = parse_bang_command(input) {
            if cmd.is_empty() {
                println!("{DIM}! <cmd> — run a shell command directly{RESET}");
            } else {
                run_bang_command(cmd);
            }
            continue;
        }

        let mut dispatch_ctx = crate::dispatch::DispatchContext {
            input,
            agent,
            agent_config,
            session_total: &mut session_total,
            session_changes: &session_changes,
            turn_history: &mut turn_history,
            bg_tracker: &bg_tracker,
            spawn_tracker: &spawn_tracker,
            undo_context: &mut undo_context,
            last_input: &mut last_input,
            last_error: &mut last_error,
            bookmarks: &mut bookmarks,
            checkpoint_store: &mut checkpoint_store,
            session_start,
            turn_count,
            cwd: &cwd,
            mcp_cli_servers: &repl_config.mcp_cli_servers,
            mcp_server_configs: &repl_config.mcp_server_configs,
            mcp_count: repl_config.mcp_count,
            openapi_count: repl_config.openapi_count,
        };
        let cmd_result = crate::dispatch::dispatch_command(&mut dispatch_ctx).await;
        match cmd_result {
            CommandResult::Quit => break,
            CommandResult::Continue => {
                turns_since_slash_command = 0;
                continue;
            }
            CommandResult::SendToAgent(prompt) => {
                turns_since_slash_command = 0;
                last_input = Some(prompt);
                // fall through to agent prompt handling
            }
            CommandResult::NotACommand => {
                turns_since_slash_command += 1;
                last_input = Some(input.to_string());
                // fall through to agent prompt handling
            }
        }

        // Snapshot files before the agent turn for per-turn undo
        let changes_before: Vec<String> = session_changes
            .snapshot()
            .iter()
            .map(|c| c.path.clone())
            .collect();
        let edits_before = session_changes.edit_count();
        // #794 half (a): the auto-continue gate needs "did this turn run any
        // tool", which the edit counter cannot answer.
        let tools_before = session_changes.tool_call_count();
        let mut turn_snap = TurnSnapshot::new();
        for path in &changes_before {
            turn_snap.snapshot_file(path);
        }
        // Also snapshot any files in the git working tree diff
        if let Ok(diff_files) = crate::git::run_git(&["diff", "--name-only"]) {
            for f in diff_files.lines().filter(|l| !l.is_empty()) {
                turn_snap.snapshot_file(f);
            }
        }

        // Expand @file mentions (e.g. "explain @src/main.rs") into file content
        let (cleaned_text, file_results) = commands::expand_file_mentions(input);

        // If teach mode is active, prepend the teaching instruction to the user message
        let effective_input = if commands::is_teach_mode() {
            format!("{}\n\n{}", commands::TEACH_MODE_PROMPT, cleaned_text)
        } else {
            cleaned_text.clone()
        };

        // If plan mode is active, prepend the planning constraint to the user message
        let effective_input = if commands::is_plan_mode() {
            format!("{}\n\n{}", commands::PLAN_MODE_PROMPT, effective_input)
        } else {
            effective_input
        };

        // If read mode is active, prepend the read-only constraint to the user message
        let effective_input = if commands::is_read_mode() {
            format!("{}\n\n{}", commands::READ_MODE_PROMPT, effective_input)
        } else {
            effective_input
        };

        // If /undo was run since the last turn, prepend context so the agent
        // knows files were reverted and its previous references may be stale.
        let effective_input = if let Some(ctx) = undo_context.take() {
            format!("{ctx}\n\n{effective_input}")
        } else {
            effective_input
        };

        // ── Auto-context: inject relevant files when no @mentions are present ──
        let (effective_input, auto_context_files) = if file_results.is_empty()
            && !commands::is_architect_mode()
            && !crate::cli_config::is_safe_mode()
        {
            // Collect recent message text so we don't re-inject files already seen
            let recent_msgs: Vec<String> = agent
                .messages()
                .iter()
                .rev()
                .take(10)
                .filter_map(|m| {
                    if let yoagent::AgentMessage::Llm(yoagent::types::Message::User {
                        content,
                        ..
                    }) = m
                    {
                        Some(
                            content
                                .iter()
                                .filter_map(|c| {
                                    if let yoagent::types::Content::Text { text } = c {
                                        Some(text.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("\n"),
                        )
                    } else {
                        None
                    }
                })
                .collect();

            let auto_files = commands::auto_context_for_prompt(&effective_input, &recent_msgs);

            if let Some(formatted) = commands::format_auto_context(&auto_files, &effective_input) {
                // Print a brief note to stderr
                let paths: Vec<&str> = auto_files.iter().map(|(p, _)| p.as_str()).collect();
                eprintln!(
                    "  📎 Auto-attached: {} (use @file to control manually)",
                    paths.join(", ")
                );

                (formatted, auto_files)
            } else {
                (effective_input, Vec::new())
            }
        } else {
            (effective_input, Vec::new())
        };
        let _ = &auto_context_files; // suppress unused warning when not logging

        // ── Parallelizable-prompt hint: if the prompt looks like 3+ independent
        // tasks, suggest /spawn once per session. Detection only — never blocks,
        // never changes execution. (First increment of issue #341.)
        static SPAWN_HINT_SHOWN: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !is_quiet() && !SPAWN_HINT_SHOWN.load(std::sync::atomic::Ordering::Relaxed) {
            if let Some(tasks) = crate::commands_spawn::detect_parallelizable_tasks(input) {
                SPAWN_HINT_SHOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                eprintln!(
                    "{DIM}  ⑂ this looks like {} independent tasks — /spawn can run them in parallel worktrees{RESET}",
                    tasks.len()
                );
            }
        }

        let prompt_start = Instant::now();
        turn_count += 1;

        // ── Architect mode: plan with strong model, implement with editor ──
        let outcome = if commands::is_architect_mode() {
            run_architect_turn(
                agent_config,
                &effective_input,
                &mut session_total,
                &session_changes,
            )
            .await
        } else if !file_results.is_empty() {
            // Print summaries like /add does
            for result in &file_results {
                match result {
                    commands::AddResult::Text { summary, .. } => println!("{summary}"),
                    commands::AddResult::Image { summary, .. } => println!("{summary}"),
                }
            }
            let word = crate::format::pluralize(file_results.len(), "file", "files");
            println!(
                "{}  ({} {word} inlined from @mentions){}\n",
                DIM,
                file_results.len(),
                RESET
            );

            // Build content blocks: user text first, then file contents
            let mut content_blocks = vec![yoagent::types::Content::Text {
                text: effective_input.clone(),
            }];
            content_blocks.extend(build_add_content_blocks(&file_results));

            run_prompt_auto_retry_with_content(
                agent,
                content_blocks,
                &mut session_total,
                &agent_config.model,
                &session_changes,
                &effective_input,
            )
            .await
        } else {
            run_prompt_auto_retry(
                agent,
                &effective_input,
                &mut session_total,
                &agent_config.model,
                &session_changes,
            )
            .await
        };
        handle_post_prompt(PostPromptContext {
            outcome: &outcome,
            agent,
            agent_config,
            session_total: &mut session_total,
            session_changes: &session_changes,
            turn_history: &mut turn_history,
            turn_snap,
            changes_before: &changes_before,
            edits_before,
            last_error: &mut last_error,
            prompt_start,
            effective_input: &effective_input,
            turn_count,
            turns_since_slash_command,
            spawn_tracker: &spawn_tracker,
        })
        .await;

        // ── Auto-continue: if the model stopped mid-work, send a follow-up ──
        {
            let max_continues =
                get_max_auto_continues(&repl_file_config, crate::commands::is_plan_apply_active());
            let mut auto_continue_count: u32 = 0;
            let mut last_text = outcome.text.clone();
            let mut last_closing = outcome.text_since_last_tool.clone();
            let mut had_error =
                outcome.last_tool_error.is_some() || outcome.last_api_error.is_some();

            while auto_continue_count < max_continues
                && !had_error
                && should_auto_continue(
                    &last_text,
                    // #808: the silence branch judges CLOSING text, not the
                    // whole turn — a pre-tool narration must not mask a
                    // post-tool abstention.
                    &last_closing,
                    agent.follow_up_queue_len(),
                    // #794 half (a): a real tool-call count, not the edit
                    // counter — a turn of read-only tools then silence is the
                    // abstention case this gate exists for.
                    session_changes.tool_call_count() > tools_before,
                    crate::cli::is_continue_on_silence(),
                )
                && !crate::prompt_budget::session_budget_exhausted(30)
            {
                auto_continue_count += 1;
                eprintln!(
                    "\n{DIM}  ⚡ auto-continuing ({auto_continue_count}/{max_continues} \
                     — more work pending)...{RESET}"
                );

                // Surface WHAT queued follow-up work is pending (yoagent 0.9
                // follow_up_queue_snapshot). Silent no-op when the queue is
                // empty or unpopulated (product-safe for providers that don't
                // use the follow-up queue).
                let fu_snapshot = agent.follow_up_queue_snapshot();
                if let Some(hint) =
                    format_followup_hint(fu_snapshot.len(), &first_followup_text(&fu_snapshot))
                {
                    eprintln!("{DIM}  {hint}{RESET}");
                }

                // Snapshot state for the continuation turn
                let cont_edits_before = session_changes.edit_count();
                let cont_changes_before: Vec<String> = session_changes
                    .snapshot()
                    .iter()
                    .map(|c| c.path.clone())
                    .collect();
                let mut cont_turn_snap = TurnSnapshot::new();
                for path in &cont_changes_before {
                    cont_turn_snap.snapshot_file(path);
                }
                if let Ok(diff_files) = crate::git::run_git(&["diff", "--name-only"]) {
                    for f in diff_files.lines().filter(|l| !l.is_empty()) {
                        cont_turn_snap.snapshot_file(f);
                    }
                }

                let cont_prompt = "Continue with the remaining work. Pick up where you left off.";
                let cont_start = Instant::now();
                turn_count += 1;

                let cont_outcome = run_prompt_auto_retry(
                    agent,
                    cont_prompt,
                    &mut session_total,
                    &agent_config.model,
                    &session_changes,
                )
                .await;

                // Update tracking for the next iteration
                last_text = cont_outcome.text.clone();
                last_closing = cont_outcome.text_since_last_tool.clone();
                had_error =
                    cont_outcome.last_tool_error.is_some() || cont_outcome.last_api_error.is_some();

                handle_post_prompt(PostPromptContext {
                    outcome: &cont_outcome,
                    agent,
                    agent_config,
                    session_total: &mut session_total,
                    session_changes: &session_changes,
                    turn_history: &mut turn_history,
                    turn_snap: cont_turn_snap,
                    changes_before: &cont_changes_before,
                    edits_before: cont_edits_before,
                    last_error: &mut last_error,
                    prompt_start: cont_start,
                    effective_input: cont_prompt,
                    turn_count,
                    turns_since_slash_command,
                    spawn_tracker: &spawn_tracker,
                })
                .await;
            }

            // ── Terminal-state marker (issue #631) ──
            // Say out loud whether this turn believes it finished or handed
            // back mid-work. Silent-and-ambiguous is what users were reading
            // as "maybe type continue?"; a stated belief can be wrong, but it
            // can also be argued with.
            //
            // `did_work` predicate: this turn recorded >=1 file edit (a
            // monotonic count, so re-editing one file across several turns
            // still counts — #774). Imprecise for read-only turns (a paused
            // exploration turn prints nothing), but the error is toward
            // silence, which is the product-safe direction.
            if !is_quiet() {
                let did_work = session_changes.edit_count() > edits_before;
                let queue_pending = agent.follow_up_queue_len();
                let final_text_chars = last_text.trim().chars().count();
                let end = classify_turn_end(
                    &last_text,
                    queue_pending,
                    did_work,
                    had_error,
                    auto_continue_count,
                    max_continues,
                );
                // Observation before claim (discussion #378): under --verbose,
                // show the raw signals the verdict came from — including on
                // turns where the verdict itself declines to speak.
                if crate::cli::is_verbose() {
                    let debug_line = format_turn_end_debug(
                        end,
                        queue_pending,
                        did_work,
                        had_error,
                        auto_continue_count,
                        max_continues,
                        final_text_chars,
                    );
                    eprintln!("{DIM}  {debug_line}{RESET}");
                }
                if let Some(line) = format_turn_end(
                    end,
                    did_work,
                    auto_continue_count,
                    max_continues,
                    crate::format::is_plain_output(),
                ) {
                    eprintln!("{DIM}  {line}{RESET}");
                }
            }
        }

        // Clear plan-apply flag after prompt + auto-continue finishes
        if crate::commands::is_plan_apply_active() {
            crate::commands::set_plan_apply_active(false);
        }

        // ── Auto-checkpoint: silently save a conversation snapshot every 5 turns ──
        if commands::auto_checkpoint(agent, turn_count) {
            eprintln!("{DIM}  (auto-checkpoint saved){RESET}");
        }
    }

    // Save readline history
    if let Some(history_path) = history_file_path() {
        if let Err(e) = rl.save_history(&history_path) {
            eprintln!("{DIM}  ⚠ could not save history: {e}{RESET}");
        }
    }

    // Auto-save session on exit (always — crash recovery for everyone)
    commands::auto_save_on_exit(agent);

    // Opt-in: snapshot risk predictions on exit so the meter accumulates
    // during any session where enabled (off by default — product-safe).
    // Dedups by git hash → at most one snapshot per HEAD.
    if crate::commands_risk::risk_autosnapshot_enabled() {
        crate::commands_risk::auto_risk_snapshot();
    }

    // Show session summary (files, tokens, cost, duration)
    if let Some(summary) = commands::format_exit_summary(
        &session_changes,
        &session_total,
        &agent_config.model,
        session_start,
    ) {
        println!("\n{summary}");
        println!("{DIM}  bye 👋{RESET}\n");
    } else {
        println!("\n{DIM}  bye 👋{RESET}\n");
    }
}

/// Default maximum number of automatic follow-up prompts per user turn.
const DEFAULT_MAX_AUTO_CONTINUES: u32 = 5;

/// Compute the effective auto-continue limit for the current turn.
///
/// Takes into account:
/// 1. If `auto_continue` is disabled in config → returns 0
/// 2. If `max_auto_continues` is set in config → uses that value
/// 3. If a `/plan apply` is active → uses `max(config_limit, 10)` so the
///    agent can work through a full plan without hitting the normal cap
pub(crate) fn get_max_auto_continues(
    config: &std::collections::HashMap<String, String>,
    in_plan_apply: bool,
) -> u32 {
    use crate::config::{parse_auto_continue_from_config, parse_max_auto_continues_from_config};

    if !parse_auto_continue_from_config(config) {
        return 0;
    }

    let base = parse_max_auto_continues_from_config(config).unwrap_or(DEFAULT_MAX_AUTO_CONTINUES);

    if in_plan_apply {
        base.max(10)
    } else {
        base
    }
}

/// Decide whether the auto-continue loop should run another turn.
///
/// `queue_pending` is the yoagent follow-up-queue length; when > 0 there is
/// authoritative pending work (user-injected messages queued while the agent
/// worked) and we continue regardless of the text heuristic. An empty queue
/// tells us nothing about whether the model finished, so we fall back to the
/// `looks_incomplete` text heuristic — never treat empty-queue as "done".
///
/// `ran_tools` + `continue_on_silence` add the opt-in `--continue-on-silence`
/// branch (issue #631): a turn that *did work* and then said nothing is the
/// abstention case — "no answer" — which today gets absorbed by the convenient
/// neighbour, "finished". With the flag on we make it an explicit third value
/// and continue. With the flag off this function is byte-identical to the old
/// two-argument version, which is the product-safety guarantee: providers that
/// legitimately finish quietly must not be looped by default.
///
/// `ran_tools` means "this turn started at least one tool call, of any kind" —
/// it is `SessionChanges::tool_call_count()`, not `edit_count()`. Until #794
/// half (a) both call sites passed the edit counter, so the parameter really
/// meant "this turn wrote a file": the branch fired for *wrote code, then went
/// quiet* and could never fire for *ran ten read-only tools, then went quiet*,
/// which is the abstention shape it was built for.
///
/// Deliberately pure — the flag is read at the call site, never from global
/// state in here, so the whole truth table stays unit-testable. The caller also
/// owns the "no error" precondition (the loop already gates on `!had_error`)
/// and the bounded budget (`get_max_auto_continues`).
///
/// `text` vs `text_since_last_tool` (#808): the two text arguments answer
/// different questions and feed different branches. `text` is the WHOLE turn
/// and feeds `looks_incomplete`, which scans the tail for unfinished-work
/// shapes — unchanged, product-default behavior. `text_since_last_tool` is
/// only what the model said AFTER its last tool result, and feeds the silence
/// branch — because the abstention this branch exists for is *closing*
/// silence, and an agent that narrates before every tool batch has a long
/// `text` on every turn. Measured before the split: the branch fired 0 times
/// in 11 opted-in sessions while the loop's A1/A2 phases abstained 16 times,
/// every one with a narration like "Let me trace the mechanism." followed by
/// one tool call and then nothing.
pub(crate) fn should_auto_continue(
    text: &str,
    text_since_last_tool: &str,
    queue_pending: usize,
    ran_tools: bool,
    continue_on_silence: bool,
) -> bool {
    if queue_pending > 0 || looks_incomplete(text) {
        return true;
    }
    // Opt-in: tools ran, then CLOSING silence. A plain chat turn that returns
    // nothing (no tools) is NOT looped — that's a model declining to speak,
    // not work left on the table.
    continue_on_silence
        && ran_tools
        && text_since_last_tool.trim().chars().count() < MIN_SUMMARY_CHARS
}

/// Why a turn ended, as far as yoyo can tell.
///
/// This is a *stated belief*, not ground truth — a belief that is printed can
/// be argued with; a silent one can't (issue #631).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TurnEnd {
    /// Nothing pending, no error, and the turn closed with real prose.
    Done,
    /// The turn stopped while work still looked outstanding: an error, a
    /// non-empty follow-up queue, or tool activity with no closing summary.
    Paused,
    /// The auto-continue budget ran out with work still queued.
    BudgetSpent,
}

/// Minimum length (chars, trimmed) of closing text we accept as a "summary".
/// Shorter than this after tool activity reads as "stopped mid-work" — the
/// model went quiet rather than wrapping up.
const MIN_SUMMARY_CHARS: usize = 20;

/// Classify why the turn ended, from data already in hand at the end of the
/// auto-continue loop. Pure — no I/O, no globals — so it can be table-tested.
///
/// Provider-agnostic: `queue_pending == 0` is the norm for providers that
/// don't populate a follow-up queue, and never on its own reads as "paused".
pub(crate) fn classify_turn_end(
    final_text: &str,
    queue_pending: usize,
    used_tools: bool,
    had_error: bool,
    continues_used: u32,
    max_continues: u32,
) -> TurnEnd {
    // Budget exhaustion is only meaningful when work is demonstrably still
    // queued — otherwise "used all continues" just means a long, finished turn.
    if max_continues > 0 && continues_used >= max_continues && queue_pending > 0 {
        return TurnEnd::BudgetSpent;
    }
    if had_error || queue_pending > 0 {
        return TurnEnd::Paused;
    }
    // Tool activity that ends with (near-)silence is the shape the user
    // described: work happened, no wrap-up followed.
    if used_tools && final_text.trim().chars().count() < MIN_SUMMARY_CHARS {
        return TurnEnd::Paused;
    }
    TurnEnd::Done
}

/// Render the one-line terminal-state marker, or `None` when nothing should
/// be printed.
///
/// `did_work` is the "this turn actually did something" gate — a plain
/// conversational reply should not be stamped `✓ done`. When `plain` is set
/// (screen-reader mode) the glyphs are dropped for words.
pub(crate) fn format_turn_end(
    end: TurnEnd,
    did_work: bool,
    continues_used: u32,
    max_continues: u32,
    plain: bool,
) -> Option<String> {
    if !did_work {
        return None;
    }
    Some(match end {
        TurnEnd::Done => {
            let mark = if plain { "done:" } else { "✓ done —" };
            format!("{mark} nothing queued")
        }
        TurnEnd::Paused => {
            let mark = if plain {
                "stopped: no summary"
            } else {
                "⏸ stopped with no summary"
            };
            format!("{mark} — type \"continue\" if this looks unfinished")
        }
        TurnEnd::BudgetSpent => {
            let mark = if plain { "stopped:" } else { "⏸" };
            format!(
                "{mark} auto-continue budget spent ({continues_used}/{max_continues}) \
                 — type \"continue\" to resume"
            )
        }
    })
}

/// Render the observation-shaped counterpart to [`format_turn_end`]: the raw
/// signals the verdict was computed from, plus the verdict they produced.
///
/// @danstis asked for exactly this (discussion #378) after declining to trust a
/// bare verdict: *"I'm hesitant to have a complete message if the detection of
/// done is not captured correctly"*. A claim you can't audit is worse than no
/// claim — so under `--verbose` we print the inputs beside the conclusion and
/// let the reader disagree with the classifier.
///
/// Facts only: no advice, no glyphs, no ANSI, pure ASCII — one string that is
/// correct in both normal and screen-reader (`format::is_plain_output()`) modes.
///
/// The parameter list mirrors [`classify_turn_end`]'s inputs exactly (plus the
/// verdict); the call site feeds both from the same variables so the two can't
/// drift apart.
pub(crate) fn format_turn_end_debug(
    end: TurnEnd,
    queue_pending: usize,
    used_tools: bool,
    had_error: bool,
    continues_used: u32,
    max_continues: u32,
    final_text_chars: usize,
) -> String {
    let yn = |b: bool| if b { "yes" } else { "no" };
    let verdict = match end {
        TurnEnd::Done => "done",
        TurnEnd::Paused => "paused",
        TurnEnd::BudgetSpent => "budget_spent",
    };
    format!(
        "turn state: queue={queue_pending} tools={} error={} \
         continues={continues_used}/{max_continues} final_text={final_text_chars}ch -> {verdict}",
        yn(used_tools),
        yn(had_error),
    )
}

/// Format a dim one-line hint naming the next queued follow-up item.
///
/// `count` is the number of items pending in the follow-up queue; `first` is
/// the text of the next queued item. Returns `None` (print nothing) when the
/// queue is empty (`count == 0`) or the first item is blank — this keeps the
/// hint a product-safe no-op for providers that don't populate the queue.
///
/// The first item is truncated char-boundary-safely (via `truncate_with_ellipsis`)
/// so a multi-byte first character never panics.
pub(crate) fn format_followup_hint(count: usize, first: &str) -> Option<String> {
    let first = first.trim();
    if count == 0 || first.is_empty() {
        return None;
    }
    let preview = truncate_with_ellipsis(first, 60);
    Some(format!("↻ continuing — {count} pending: {preview}"))
}

/// Extract a short preview of the next queued follow-up item from a yoagent
/// follow-up-queue snapshot. Returns the trimmed text of the first LLM message's
/// first text content block, or an empty string if the snapshot has no usable
/// text (e.g. providers that don't populate the queue).
fn first_followup_text(snapshot: &[yoagent::AgentMessage]) -> String {
    for msg in snapshot {
        if let Some(yoagent::Message::User { content, .. }) = msg.as_llm() {
            for block in content {
                if let yoagent::Content::Text { text } = block {
                    let t = text.trim();
                    if !t.is_empty() {
                        return t.to_string();
                    }
                }
            }
        }
    }
    String::new()
}

/// Heuristic check: does the agent's response text suggest it stopped mid-work
/// and intends to continue? Conservative — only triggers on clear signals.
pub(crate) fn looks_incomplete(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.len() < 20 {
        return false;
    }

    // Check the tail of the response (last ~300 chars) for continuation signals.
    let tail_start = safe_byte_index(text, text.len().saturating_sub(300));
    let tail = &text[tail_start..];
    let tail_lower = tail.to_lowercase();

    // ── Completion guard ──
    // When the tail contains strong completion signals, weaker patterns (action
    // phrases, ellipsis with work signals, ordinal progression) are likely being
    // used in a summary/recap context rather than indicating unfinished work.
    // Strong patterns (explicit continuation, unclosed fences, step N of M) still
    // override completion signals because they're structurally unambiguous.
    let has_completion_signal = tail_has_completion_signal(&tail_lower);

    // ── Pattern 1: explicit continuation phrases near the end ──
    let continuation_phrases = [
        "next, i'll",
        "next i'll",
        "i'll now ",
        "let me continue",
        "let me proceed",
        "i'll continue",
        "moving on to",
        "let me move on",
        "i'll move on",
        "now let's",
        "now i'll",
        "now i need to",
        "let me now",
        "i still need to",
        "i need to continue",
    ];
    for phrase in &continuation_phrases {
        if tail_lower.contains(phrase) {
            return true;
        }
    }

    // ── Pattern 2: "remaining" + steps/tasks/items near the end ──
    if tail_lower.contains("remaining") {
        for word in &["step", "task", "item", "change", "file", "fix", "issue"] {
            if tail_lower.contains(word) {
                return true;
            }
        }
    }

    // ── Pattern 3: response ends with "..." suggesting continuation ──
    // Only trigger if the tail also contains signals of ongoing work.
    // LLMs commonly end prose with "..." (e.g. "handles errors, logging, etc...")
    // without intending to continue — that's a rhetorical trailing off, not a cutoff.
    // Skip when completion signals are present (the "..." is a summary flourish).
    if !has_completion_signal && (text.ends_with("...") || text.ends_with("…")) {
        let fence_count = text.matches("```").count();
        let has_work_signal = tail_lower.contains("remaining")
            || tail_lower.contains("need to")
            || tail_lower.contains("i'll ")
            || tail_lower.contains("i\u{2019}ll ") // smart apostrophe
            || tail_lower.contains("let me ")
            || tail_lower.contains("next")
            || tail_lower.contains("more things")
            || tail_lower.contains("more to")
            || tail_lower.contains("moving on")
            || tail_lower.contains("continue")
            || fence_count % 2 == 1;
        if has_work_signal {
            return true;
        }
    }

    // ── Pattern 4: numbered steps where later steps haven't been addressed ──
    // Look for "Step N:" or "N." patterns and check if the last mentioned step
    // is less than the total count announced.
    let mut max_announced: u32 = 0;
    let mut max_reached: u32 = 0;
    for line in text.lines() {
        let trimmed = line.trim().to_lowercase();
        // Match patterns like "Step 3:" or "3. " or "**Step 3**"
        if let Some(n) = extract_step_number(&trimmed) {
            if n > max_announced {
                max_announced = n;
            }
        }
    }
    // Now check which steps actually have content after them
    // (simplification: the last step number mentioned in the text is the last reached)
    for line in text.lines().rev() {
        let trimmed = line.trim().to_lowercase();
        if let Some(n) = extract_step_number(&trimmed) {
            max_reached = n;
            break;
        }
    }
    if max_announced >= 3 && max_reached > 0 && max_reached < max_announced {
        // Announced N steps but only reached step M < N
        return true;
    }

    // ── Pattern 5: unclosed code blocks ──
    // If the number of ``` fences in the full text is odd, the model was cut off mid-code.
    {
        let fence_count = text.matches("```").count();
        if fence_count % 2 == 1 {
            return true;
        }
    }

    // ── Pattern 6: action intent phrases near the end ──
    // Skip when completion signals are present — the action phrase is being used
    // to introduce a recap/summary rather than signal unfinished work.
    if !has_completion_signal {
        let action_phrases = [
            "let me update",
            "let me fix",
            "let me modify",
            "let me add",
            "let me create",
            "let me write",
            "let me implement",
            "let me handle",
            "i'll update",
            "i'll fix",
            "i'll modify",
            "i'll add",
            "i'll create",
            "i'll implement",
            "i'll handle",
        ];
        for phrase in &action_phrases {
            if tail_lower.contains(phrase) {
                return true;
            }
        }
    }

    // ── Pattern 7: ordinal progression without completion ──
    // "first" used as a step enumeration marker (not just a discourse marker)
    // requires corroborating intent signals: "then", "next", "i'll", "need to", etc.
    if (tail_lower.contains("first,") || tail_lower.contains("first "))
        && !tail_lower.contains("finally")
        && !tail_lower.contains("lastly")
        && !tail_lower.contains("last,")
    {
        // Require a forward-looking signal to distinguish "First, I'll fix X, then Y"
        // from "First, here's what I changed" (discourse marker, no future intent).
        let has_forward_intent = tail_lower.contains("then ")
            || tail_lower.contains("next")
            || tail_lower.contains("need to")
            || tail_lower.contains("i'll ")
            || tail_lower.contains("i\u{2019}ll ")
            || tail_lower.contains("let me ")
            || tail_lower.contains("second");

        if has_forward_intent {
            if !tail_lower.contains("second") {
                return true;
            }
            if !tail_lower.contains("third") && !tail_lower.contains("finally") {
                return true;
            }
        }
    }

    // ── Pattern 8: explicit "step X of Y" where X < Y ──
    // Matches "step 2 of 5", "Step 3/7", "step 1 of 4 done" etc.
    if let Some(incomplete) = step_x_of_y_incomplete(&tail_lower) {
        if incomplete {
            return true;
        }
    }

    false
}

/// Extract a step number from patterns like "step 3:", "3. ", "**step 3**"
/// Check if the tail contains strong completion signals.
///
/// These indicate the agent has finished work and is summarising/recapping,
/// so weaker incomplete-patterns (action phrases, ellipsis with work signals,
/// ordinal progression) should be suppressed — the action phrase is being used
/// in a summary context, not as a genuine work-in-progress signal.
fn tail_has_completion_signal(tail_lower: &str) -> bool {
    const COMPLETION_PHRASES: &[&str] = &[
        "all done",
        "that's all",
        "that\u{2019}s all",
        "all complete",
        "is complete",
        "is done",
        "are done",
        "have been applied",
        "have been completed",
        "everything is working",
        "everything works",
        "all changes",
        "finished all",
        "completed all",
        "implementation is complete",
        "all set",
        "that's it",
        "that\u{2019}s it",
        "successfully",
    ];
    COMPLETION_PHRASES.iter().any(|p| tail_lower.contains(p))
}

fn extract_step_number(line: &str) -> Option<u32> {
    // "step N:" or "**step N**" or "step N."
    if let Some(rest) = line
        .strip_prefix("step ")
        .or_else(|| line.strip_prefix("**step "))
    {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            return digits.parse().ok();
        }
    }
    // "N. " at the start of a line (numbered list)
    let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 2 {
        let rest = &line[digits.len()..];
        if rest.starts_with(". ") || rest.starts_with(") ") {
            return digits.parse().ok();
        }
    }
    None
}

/// Check for "step X of Y" or "step X/Y" patterns where X < Y, indicating
/// the model stopped before completing all steps.
fn step_x_of_y_incomplete(text: &str) -> Option<bool> {
    // Search for patterns like "step 2 of 5", "step 3/7", "step 1 of 4 complete"
    let mut found = false;
    let mut search_from = 0;
    while search_from < text.len() {
        // Use str::find which is UTF-8 safe and returns valid char boundaries
        let Some(pos) = text[search_from..].find("step ") else {
            break;
        };
        let abs_pos = search_from + pos;
        let after_step = &text[abs_pos + 5..]; // "step " is 5 ASCII bytes, always safe
                                               // Parse X
        let x_digits: String = after_step
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !x_digits.is_empty() {
            if let Ok(x) = x_digits.parse::<u32>() {
                let rest = &after_step[x_digits.len()..]; // ASCII digits, len == byte count
                                                          // Look for " of " or "/"
                let y_str = if let Some(r) = rest.strip_prefix(" of ") {
                    Some(r)
                } else {
                    rest.strip_prefix('/')
                };
                if let Some(y_rest) = y_str {
                    let y_digits: String =
                        y_rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !y_digits.is_empty() {
                        if let Ok(y) = y_digits.parse::<u32>() {
                            if x < y {
                                return Some(true);
                            }
                            found = true;
                        }
                    }
                }
            }
        }
        // Advance past this "step " match. Move forward by at least 1 char to avoid
        // infinite loop on the same position.
        search_from = abs_pos + 1;
    }
    if found {
        Some(false) // Found step X of Y but X >= Y
    } else {
        None // No pattern found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check if any candidate has the given replacement text.
    fn has_replacement(candidates: &[Pair], replacement: &str) -> bool {
        candidates.iter().any(|c| c.replacement == replacement)
    }

    #[test]
    fn test_prompt_has_octopus() {
        // Verify the styled prompt contains the octopus emoji
        let prompt_no_branch = format!("{BOLD}{GREEN}🐙 › {RESET}");
        assert!(
            prompt_no_branch.contains('🐙'),
            "Prompt should contain octopus emoji"
        );

        let prompt_with_branch = format!("{BOLD}{GREEN}main{RESET} {BOLD}{GREEN}🐙 › {RESET}");
        assert!(
            prompt_with_branch.contains('🐙'),
            "Branch prompt should contain octopus emoji"
        );
    }

    #[test]
    fn test_needs_continuation_backslash() {
        assert!(needs_continuation("hello \\"));
        assert!(needs_continuation("line ends with\\"));
        assert!(!needs_continuation("normal line"));
        assert!(!needs_continuation("has \\ in middle"));
    }

    #[test]
    fn test_needs_continuation_code_fence() {
        assert!(needs_continuation("```rust"));
        assert!(needs_continuation("```"));
        assert!(!needs_continuation("some text ```"));
        assert!(!needs_continuation("normal"));
    }

    #[test]
    fn test_needs_continuation_does_not_swallow_bang() {
        // Pin that `!` lines never enter multi-line collection
        assert!(!needs_continuation("!echo hi"));
        assert!(!needs_continuation("!git status"));
    }

    #[test]
    fn test_parse_bang_command_basic() {
        assert_eq!(parse_bang_command("!ls -la"), Some("ls -la"));
        assert_eq!(parse_bang_command("  !git status"), Some("git status"));
        assert_eq!(parse_bang_command("!echo hi  "), Some("echo hi"));
    }

    #[test]
    fn test_parse_bang_command_empty_command() {
        // Bare `!` or `!` + whitespace → Some("") so the caller prints a usage hint
        assert_eq!(parse_bang_command("!"), Some(""));
        assert_eq!(parse_bang_command("!   "), Some(""));
    }

    #[test]
    fn test_parse_bang_command_not_bang() {
        assert_eq!(parse_bang_command("hello"), None);
        assert_eq!(parse_bang_command("/help"), None);
        assert_eq!(parse_bang_command(""), None);
        assert_eq!(parse_bang_command("   "), None);
        // `!` in the middle is not passthrough
        assert_eq!(parse_bang_command("wow!"), None);
    }

    #[test]
    fn test_parse_bang_command_edge_spellings() {
        // `!!` and `!=` are still commands — pass through as-is
        assert_eq!(parse_bang_command("!!"), Some("!"));
        assert_eq!(parse_bang_command("!="), Some("="));
    }

    #[test]
    fn test_parse_bang_command_multibyte() {
        // Must not panic on multi-byte chars (char-safe ops only, no byte indexing)
        assert_eq!(parse_bang_command("!echo héllo ✓"), Some("echo héllo ✓"));
        assert_eq!(parse_bang_command("!héllo"), Some("héllo"));
    }

    #[test]
    fn test_parse_bang_query_positive() {
        assert_eq!(parse_bang_query("!?"), Some(""));
        assert_eq!(parse_bang_query("!?   "), Some(""));
        assert_eq!(
            parse_bang_query("!? why did the second test fail"),
            Some("why did the second test fail")
        );
        assert_eq!(parse_bang_query("  !? question  "), Some("question"));
    }

    #[test]
    fn test_parse_bang_query_negative() {
        // Paired near-misses (Day 122 lesson: test both sides of the boundary)
        assert_eq!(parse_bang_query("! ls"), None);
        assert_eq!(parse_bang_query("!ls"), None);
        assert_eq!(parse_bang_query("!"), None);
        assert_eq!(parse_bang_query("?"), None);
        assert_eq!(parse_bang_query("hello"), None);
        assert_eq!(parse_bang_query(""), None);
    }

    #[test]
    fn test_parse_bang_query_ordering_matters() {
        // `!?` also matches the generic `!` parse (as command `?`), which is
        // why the REPL loop MUST check parse_bang_query first. This test
        // documents the hazard so the ordering isn't "simplified" away.
        assert_eq!(parse_bang_command("!?"), Some("?"));
        assert_eq!(parse_bang_query("!?"), Some(""));
    }

    #[test]
    fn test_tail_for_capture_under_caps_unchanged() {
        assert_eq!(tail_for_capture("a\nb\nc\n", 1024, 10), "a\nb\nc\n");
        assert_eq!(tail_for_capture("no newline", 1024, 10), "no newline");
        assert_eq!(tail_for_capture("", 1024, 10), "");
    }

    #[test]
    fn test_tail_for_capture_line_cap_keeps_tail() {
        let s = "one\ntwo\nthree\nfour\n";
        assert_eq!(tail_for_capture(s, 1024, 2), "three\nfour\n");
        // Without trailing newline the last partial line still counts
        let s = "one\ntwo\nthree\nfour";
        assert_eq!(tail_for_capture(s, 1024, 2), "three\nfour");
    }

    #[test]
    fn test_tail_for_capture_byte_cap_multibyte_boundary() {
        // ✓ is 3 bytes; a cut landing mid-char must advance to a boundary
        // instead of panicking (CLAUDE.md byte-indexing rule, #250).
        let s = "✓".repeat(100); // 300 bytes, one line
        for cap in 1..=10 {
            let tail = tail_for_capture(&s, cap, 10);
            assert!(tail.len() <= cap);
            assert!(tail.chars().all(|c| c == '✓'));
        }
        // Exact boundary keeps whole chars
        assert_eq!(tail_for_capture(&s, 6, 10), "✓✓");
    }

    #[test]
    fn test_tail_for_capture_zero_caps() {
        assert_eq!(tail_for_capture("abc", 0, 10), "");
        assert_eq!(tail_for_capture("abc", 10, 0), "");
    }

    #[test]
    fn test_build_bang_followup_prompt_failure_with_question() {
        let res = BangResult {
            command: "cargo test".to_string(),
            exit_code: Some(101),
            output_tail: "test foo ... FAILED".to_string(),
        };
        let prompt = build_bang_followup_prompt(&res, "why did foo fail");
        assert!(prompt.contains("$ cargo test"));
        assert!(prompt.contains("failed with exit code 101"));
        assert!(prompt.contains("test foo ... FAILED"));
        assert!(prompt.contains("why did foo fail"));
    }

    #[test]
    fn test_build_bang_followup_prompt_bare_defaults() {
        let failed = BangResult {
            command: "npm test".to_string(),
            exit_code: Some(2),
            output_tail: "boom".to_string(),
        };
        let prompt = build_bang_followup_prompt(&failed, "");
        assert!(prompt.contains("This command failed. Explain what went wrong and how to fix it."));

        // Bare `!?` after a *successful* command asks for a summary instead
        let ok = BangResult {
            command: "ls".to_string(),
            exit_code: Some(0),
            output_tail: "src\ndocs".to_string(),
        };
        let prompt = build_bang_followup_prompt(&ok, "");
        assert!(prompt.contains("exited successfully (exit 0)"));
        assert!(prompt.contains("Summarize this output"));
        assert!(!prompt.contains("This command failed"));
    }

    #[test]
    fn test_build_bang_followup_prompt_signal_and_empty_output() {
        let res = BangResult {
            command: "sleep 100".to_string(),
            exit_code: None,
            output_tail: "   ".to_string(),
        };
        let prompt = build_bang_followup_prompt(&res, "what happened");
        assert!(prompt.contains("was terminated by a signal"));
        assert!(prompt.contains("(no output)"));
        assert!(!prompt.contains("```"));
    }

    #[test]
    fn test_yoyo_helper_completes_slash_commands() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Typing "/" should suggest all commands
        let (start, candidates) = helper.complete("/", 1, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(!candidates.is_empty());
        assert!(has_replacement(&candidates, "/help"));
        assert!(has_replacement(&candidates, "/quit"));

        // Typing "/he" should suggest "/help" and "/health"
        let (start, candidates) = helper.complete("/he", 3, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "/help"));
        assert!(has_replacement(&candidates, "/health"));
        assert!(!has_replacement(&candidates, "/quit"));

        // Typing "/model " (with space) should return model completions
        let (start, candidates) = helper.complete("/model ", 7, &ctx).unwrap();
        assert_eq!(start, 7);
        assert!(
            !candidates.is_empty(),
            "Should offer model name completions after /model "
        );
        assert!(
            candidates.iter().any(|c| c.replacement.contains("claude")),
            "Should include Claude models"
        );

        // "/model cl" should filter to Claude models
        let (start, candidates) = helper.complete("/model cl", 9, &ctx).unwrap();
        assert_eq!(start, 7);
        for c in &candidates {
            assert!(
                c.replacement.starts_with("cl"),
                "All completions should start with 'cl': {:?}",
                c.replacement
            );
        }

        // Regular text that doesn't match any files returns no completions
        let (_, candidates) = helper.complete("zzz_nonexistent_xyz", 19, &ctx).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_file_path_completion_current_dir() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "Cargo" should match Cargo.toml (and possibly Cargo.lock)
        let (start, candidates) = helper.complete("Cargo", 5, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "Cargo.toml"));
    }

    #[test]
    fn test_file_path_completion_with_directory_prefix() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "src/ma" should match "src/main.rs"
        let (start, candidates) = helper.complete("src/ma", 6, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "src/main.rs"));
    }

    #[test]
    fn test_file_path_completion_no_completions_for_empty() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Empty input should return no completions
        let (_, candidates) = helper.complete("", 0, &ctx).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_file_path_completion_after_text() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "read the src/ma" should complete "src/ma" as the last word
        let input = "read the src/ma";
        let (start, candidates) = helper.complete(input, input.len(), &ctx).unwrap();
        assert_eq!(start, 9); // "read the " is 9 chars
        assert!(has_replacement(&candidates, "src/main.rs"));
    }

    #[test]
    fn test_file_path_completion_directories_have_slash() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "sr" should match "src/" (directory with trailing slash)
        let (start, candidates) = helper.complete("sr", 2, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "src/"));
    }

    #[test]
    fn test_file_path_slash_commands_still_work() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Slash commands should still complete normally
        let (start, candidates) = helper.complete("/he", 3, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(has_replacement(&candidates, "/help"));
        assert!(has_replacement(&candidates, "/health"));
    }

    #[test]
    fn test_arg_completion_think_levels() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/think " should offer thinking level completions
        let (start, candidates) = helper.complete("/think ", 7, &ctx).unwrap();
        assert_eq!(start, 7);
        assert!(has_replacement(&candidates, "off"));
        assert!(has_replacement(&candidates, "high"));

        // "/think m" should filter to medium/minimal
        let (start, candidates) = helper.complete("/think m", 8, &ctx).unwrap();
        assert_eq!(start, 7);
        assert!(has_replacement(&candidates, "medium"));
        assert!(has_replacement(&candidates, "minimal"));
        assert!(!has_replacement(&candidates, "off"));
    }

    #[test]
    fn test_arg_completion_git_subcommands() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/git " should offer git subcommand completions
        let (start, candidates) = helper.complete("/git ", 5, &ctx).unwrap();
        assert_eq!(start, 5);
        assert!(has_replacement(&candidates, "status"));
        assert!(has_replacement(&candidates, "branch"));

        // "/git s" should filter to status and stash
        let (start, candidates) = helper.complete("/git s", 6, &ctx).unwrap();
        assert_eq!(start, 5);
        assert!(has_replacement(&candidates, "status"));
        assert!(has_replacement(&candidates, "stash"));
        assert!(!has_replacement(&candidates, "log"));
    }

    #[test]
    fn test_arg_completion_pr_subcommands() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/pr " should offer PR subcommand completions
        let (start, candidates) = helper.complete("/pr ", 4, &ctx).unwrap();
        assert_eq!(start, 4);
        assert!(has_replacement(&candidates, "create"));
        assert!(has_replacement(&candidates, "checkout"));
    }

    #[test]
    fn test_arg_completion_provider_names() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/provider " should offer provider name completions
        let (start, candidates) = helper.complete("/provider ", 10, &ctx).unwrap();
        assert_eq!(start, 10);
        assert!(has_replacement(&candidates, "anthropic"));
        assert!(has_replacement(&candidates, "openai"));
        assert!(has_replacement(&candidates, "google"));

        // "/provider o" should filter to providers starting with 'o'
        let (start, candidates) = helper.complete("/provider o", 11, &ctx).unwrap();
        assert_eq!(start, 10);
        assert!(has_replacement(&candidates, "openai"));
        assert!(has_replacement(&candidates, "openrouter"));
        assert!(has_replacement(&candidates, "ollama"));
        assert!(!has_replacement(&candidates, "anthropic"));
    }

    #[test]
    fn test_arg_completion_falls_through_to_file_path() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/docs Cargo" should fall through to file path completion since /docs
        // has no custom argument completions
        let (start, candidates) = helper.complete("/docs Cargo", 11, &ctx).unwrap();
        assert_eq!(start, 6); // after "/docs "
        assert!(has_replacement(&candidates, "Cargo.toml"));
    }

    #[test]
    fn test_arg_completion_no_nested_spaces() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/git status " (second space) should NOT trigger arg completion again,
        // it should fall through to file path completion
        let input = "/git status sr";
        let (start, candidates) = helper.complete(input, input.len(), &ctx).unwrap();
        // Should be file path completing "sr" → "src/"
        assert_eq!(start, 12); // after "/git status "
        assert!(
            has_replacement(&candidates, "src/"),
            "Second arg should use file path completion"
        );
    }

    // ── Pair description tests ─────────────────────────────────────

    #[test]
    fn test_slash_completion_pairs_include_descriptions() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "/" completion should return Pairs where display contains descriptions
        let (_, candidates) = helper.complete("/", 1, &ctx).unwrap();
        let help_pair = candidates.iter().find(|c| c.replacement == "/help");
        assert!(help_pair.is_some(), "Should include /help");
        let help_display = &help_pair.unwrap().display;
        assert!(
            help_display.contains("Show help"),
            "Display should include description: {help_display}"
        );

        let add_pair = candidates.iter().find(|c| c.replacement == "/add");
        assert!(add_pair.is_some(), "Should include /add");
        let add_display = &add_pair.unwrap().display;
        assert!(
            add_display.contains("Add file"),
            "Display should include description: {add_display}"
        );
    }

    #[test]
    fn test_slash_completion_display_is_padded() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        let (_, candidates) = helper.complete("/", 1, &ctx).unwrap();
        // All slash command pairs should have display wider than replacement
        // (because display includes padding + description)
        for c in &candidates {
            assert!(
                c.display.len() > c.replacement.len(),
                "Display '{}' should be wider than replacement '{}'",
                c.display,
                c.replacement
            );
        }
    }

    #[test]
    fn test_subcommand_pairs_have_matching_display_and_replacement() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Subcommand completions (like /think off) should have display == replacement
        let (_, candidates) = helper.complete("/think ", 7, &ctx).unwrap();
        for c in &candidates {
            assert_eq!(
                c.display, c.replacement,
                "Subcommand display and replacement should match"
            );
        }
    }

    #[test]
    fn test_hinter_shows_command_completion() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // Typing "/he" should suggest "lp — Show help for commands"
        let hint = helper.hint("/he", 3, &ctx);
        assert!(hint.is_some());
        assert!(hint.unwrap().starts_with("lp"));
    }

    #[test]
    fn test_hinter_shows_description_for_complete_command() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // Typing "/help" exactly should show description
        let hint = helper.hint("/help", 5, &ctx);
        assert!(hint.is_some());
        let hint_text = hint.unwrap();
        assert!(
            hint_text.contains("—"),
            "Hint should contain em-dash: {hint_text}"
        );
    }

    #[test]
    fn test_hinter_no_hint_when_typing_argument() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // When user is already typing an argument, no hint
        let hint = helper.hint("/add src/", 9, &ctx);
        assert!(hint.is_none());
    }

    #[test]
    fn test_hinter_shows_arg_hint_after_command_space() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // "/diff " should show argument hints
        let hint = helper.hint("/diff ", 6, &ctx);
        assert!(hint.is_some(), "Should show arg hint for /diff ");
        let hint_text = hint.unwrap();
        assert!(
            hint_text.contains("--stat"),
            "Diff arg hint should contain --stat: {hint_text}"
        );
    }

    #[test]
    fn test_hinter_shows_arg_hint_for_help() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        let hint = helper.hint("/help ", 6, &ctx);
        assert!(hint.is_some(), "Should show arg hint for /help ");
        assert!(hint.unwrap().contains("command"));
    }

    #[test]
    fn test_hinter_no_arg_hint_for_no_arg_command() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // /version takes no args, so trailing space should give no hint
        let hint = helper.hint("/version ", 9, &ctx);
        assert!(hint.is_none());
    }

    #[test]
    fn test_hinter_no_hint_for_non_slash() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        let hint = helper.hint("hello", 5, &ctx);
        assert!(hint.is_none());
    }

    #[test]
    fn test_hinter_no_hint_for_bare_slash() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        let hint = helper.hint("/", 1, &ctx);
        assert!(hint.is_none());
    }

    #[test]
    fn test_hinter_no_hint_when_cursor_not_at_end() {
        let helper = YoyoHelper;
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);
        // Cursor at position 2, but line is 5 chars
        let hint = helper.hint("/help", 2, &ctx);
        assert!(hint.is_none());
    }

    // ── looks_incomplete tests ──────────────────────────────────────────

    #[test]
    fn test_looks_incomplete_empty_and_short() {
        assert!(!looks_incomplete(""));
        assert!(!looks_incomplete("ok"));
        assert!(!looks_incomplete("short response"));
    }

    // ── Terminal-state marker (issue #631) ──

    const SUMMARY: &str = "Fixed the parser and added a regression test; all checks pass locally.";

    #[test]
    fn classify_turn_end_table() {
        // (final_text, queue, used_tools, had_error, used, max, expected)
        let cases: &[(&str, usize, bool, bool, u32, u32, TurnEnd)] = &[
            // Clean finish: work done, real summary, nothing queued.
            (SUMMARY, 0, true, false, 0, 5, TurnEnd::Done),
            // Tool activity that trailed off into silence — the #631 shape.
            ("", 0, true, false, 0, 5, TurnEnd::Paused),
            ("ok", 0, true, false, 0, 5, TurnEnd::Paused),
            // Errors always hand back.
            (SUMMARY, 0, true, true, 0, 5, TurnEnd::Paused),
            // Work still queued but budget not spent → paused, not budget.
            (SUMMARY, 2, true, false, 1, 5, TurnEnd::Paused),
            // Budget spent WITH pending work.
            (SUMMARY, 3, true, false, 5, 5, TurnEnd::BudgetSpent),
            ("", 1, true, false, 6, 5, TurnEnd::BudgetSpent),
            // NEAR MISS: budget fully used but queue empty → a long finished
            // turn, not a hand-back (Day 122: test the non-firing side).
            (SUMMARY, 0, true, false, 5, 5, TurnEnd::Done),
            // NEAR MISS: max_continues == 0 (auto-continue disabled) must not
            // read as "budget spent" just because 0 >= 0.
            (SUMMARY, 0, true, false, 0, 0, TurnEnd::Done),
            // NEAR MISS: short text with NO tool activity is ordinary chat.
            ("sure", 0, false, false, 0, 5, TurnEnd::Done),
            // Provider that never populates the queue: 0 pending is normal and
            // must not read as paused when a summary exists.
            (SUMMARY, 0, true, false, 0, 5, TurnEnd::Done),
        ];
        for (text, queue, tools, err, used, max, expected) in cases {
            let got = classify_turn_end(text, *queue, *tools, *err, *used, *max);
            assert_eq!(
                got, *expected,
                "classify_turn_end({text:?}, {queue}, {tools}, {err}, {used}, {max})"
            );
        }
    }

    #[test]
    fn classify_turn_end_summary_length_boundary() {
        // Exactly at the threshold counts as a summary; one char short doesn't.
        let at = "x".repeat(MIN_SUMMARY_CHARS);
        let below = "x".repeat(MIN_SUMMARY_CHARS - 1);
        assert_eq!(classify_turn_end(&at, 0, true, false, 0, 5), TurnEnd::Done);
        assert_eq!(
            classify_turn_end(&below, 0, true, false, 0, 5),
            TurnEnd::Paused
        );
        // Multi-byte text must be counted in chars, not bytes (and not panic).
        let emoji = "🐙".repeat(MIN_SUMMARY_CHARS);
        assert_eq!(
            classify_turn_end(&emoji, 0, true, false, 0, 5),
            TurnEnd::Done
        );
    }

    #[test]
    fn format_turn_end_debug_reports_every_classifier_input() {
        // Same rows as `classify_turn_end_table` — the debug view must speak
        // about exactly the inputs the classifier consumes, on every shape.
        let cases: &[(&str, usize, bool, bool, u32, u32)] = &[
            (SUMMARY, 0, true, false, 0, 5),
            ("", 0, true, false, 0, 5),
            ("ok", 0, true, false, 0, 5),
            (SUMMARY, 0, true, true, 0, 5),
            (SUMMARY, 2, true, false, 1, 5),
            (SUMMARY, 3, true, false, 5, 5),
            ("", 1, true, false, 6, 5),
            (SUMMARY, 0, true, false, 5, 5),
            (SUMMARY, 0, true, false, 0, 0),
            ("sure", 0, false, false, 0, 5),
            // Multi-byte closing text: the char count must not panic or
            // disagree with the classifier's own trimmed char count.
            ("完了しました ✓ 全部通った", 0, true, false, 0, 5),
        ];
        for (text, queue, tools, err, used, max) in cases {
            let chars = text.trim().chars().count();
            let end = classify_turn_end(text, *queue, *tools, *err, *used, *max);
            let line = format_turn_end_debug(end, *queue, *tools, *err, *used, *max, chars);

            // Every input value is present, by name.
            assert!(line.contains(&format!("queue={queue}")), "{line}");
            assert!(
                line.contains(&format!("tools={}", if *tools { "yes" } else { "no" })),
                "{line}"
            );
            assert!(
                line.contains(&format!("error={}", if *err { "yes" } else { "no" })),
                "{line}"
            );
            assert!(line.contains(&format!("continues={used}/{max}")), "{line}");
            assert!(line.contains(&format!("final_text={chars}ch")), "{line}");

            // And the verdict it produced.
            let verdict = match end {
                TurnEnd::Done => "done",
                TurnEnd::Paused => "paused",
                TurnEnd::BudgetSpent => "budget_spent",
            };
            assert!(line.ends_with(&format!("-> {verdict}")), "{line}");

            // Safe in screen-reader mode without a second code path: pure
            // ASCII, no ANSI escapes, no glyphs — one string for both modes.
            assert!(line.is_ascii(), "non-ASCII in debug line: {line}");
            assert!(
                !line.contains('\u{1b}'),
                "ANSI escape in debug line: {line}"
            );
        }
    }

    #[test]
    fn format_turn_end_debug_distinguishes_all_three_verdicts() {
        // Near-miss guard: the verdict word must actually vary with the state,
        // not be a constant that happens to match on the row I tested first.
        let done = format_turn_end_debug(TurnEnd::Done, 0, true, false, 0, 5, 70);
        let paused = format_turn_end_debug(TurnEnd::Paused, 1, true, true, 1, 5, 0);
        let budget = format_turn_end_debug(TurnEnd::BudgetSpent, 3, true, false, 5, 5, 70);
        assert!(done.ends_with("-> done"), "{done}");
        assert!(paused.ends_with("-> paused"), "{paused}");
        assert!(budget.ends_with("-> budget_spent"), "{budget}");
        assert_ne!(done, paused);
        assert_ne!(paused, budget);
    }

    #[test]
    fn format_turn_end_suppressed_for_trivial_chat_turns() {
        // No work signal → no marker at all, whatever the classification.
        for end in [TurnEnd::Done, TurnEnd::Paused, TurnEnd::BudgetSpent] {
            assert_eq!(format_turn_end(end, false, 0, 5, false), None);
        }
    }

    #[test]
    fn format_turn_end_renders_each_state() {
        let done = format_turn_end(TurnEnd::Done, true, 0, 5, false).unwrap();
        assert!(done.contains("done"), "{done}");
        assert!(done.contains("nothing queued"), "{done}");

        let paused = format_turn_end(TurnEnd::Paused, true, 0, 5, false).unwrap();
        assert!(paused.contains("continue"), "{paused}");

        let budget = format_turn_end(TurnEnd::BudgetSpent, true, 5, 5, false).unwrap();
        assert!(budget.contains("5/5"), "{budget}");
        assert!(budget.contains("continue"), "{budget}");

        // Always exactly one line — this prints after every working turn.
        for line in [done, paused, budget] {
            assert!(!line.contains('\n'), "marker must be one line: {line}");
        }
    }

    #[test]
    fn format_turn_end_plain_mode_drops_glyphs() {
        for end in [TurnEnd::Done, TurnEnd::Paused, TurnEnd::BudgetSpent] {
            let plain = format_turn_end(end, true, 5, 5, true).unwrap();
            assert!(
                !plain.contains('✓') && !plain.contains('⏸') && !plain.contains('\x1b'),
                "plain marker must be glyph-free and escape-free: {plain}"
            );
        }
    }

    #[test]
    fn should_auto_continue_true_when_queue_pending() {
        // A non-empty follow-up queue overrides the text heuristic: "ok" is a
        // string looks_incomplete() returns false for, but pending work wins.
        assert!(!looks_incomplete("ok"));
        assert!(should_auto_continue("ok", "ok", 1, false, false));
    }

    #[test]
    fn should_auto_continue_falls_back_to_heuristic_when_queue_empty_incomplete() {
        // Empty queue → fall back to the heuristic, which fires on this phrase.
        let incomplete = "I've fixed the first file. Next, I'll update the remaining tests.";
        assert!(looks_incomplete(incomplete));
        assert!(should_auto_continue(
            incomplete, incomplete, 0, false, false
        ));
    }

    #[test]
    fn should_auto_continue_false_when_queue_empty_and_complete() {
        // Paired negative (Day 122): empty queue + complete-looking text → false.
        assert!(!looks_incomplete("short response"));
        assert!(!should_auto_continue(
            "short response",
            "short response",
            0,
            false,
            false
        ));
    }

    /// The old two-argument behaviour, spelled out literally. This is what
    /// `should_auto_continue(.., enabled = false)` must equal for every input —
    /// the product-safety guarantee for `--continue-on-silence` (issue #631).
    fn legacy_should_auto_continue(text: &str, queue_pending: usize) -> bool {
        queue_pending > 0 || looks_incomplete(text)
    }

    #[test]
    fn should_auto_continue_default_off_is_byte_identical_to_legacy() {
        // Includes the case the flag exists for: empty final text + tools used
        // + empty queue. With the flag off it must still read as "finished".
        let texts = [
            "",
            "   ",
            "\n\t  \n",
            "ok",
            "short response",
            "Done — all tests pass and the build is green.",
            "I've fixed the first file. Next, I'll update the remaining tests.",
            "1234567890123456789",  // 19 chars: under MIN_SUMMARY_CHARS
            "12345678901234567890", // 20 chars: exactly at the boundary
        ];
        for text in texts {
            for queue_pending in [0usize, 1, 3] {
                for used_tools in [false, true] {
                    let expected = legacy_should_auto_continue(text, queue_pending);
                    assert_eq!(
                        should_auto_continue(text, text, queue_pending, used_tools, false),
                        expected,
                        "default-off must match legacy for \
                         (text={text:?}, queue={queue_pending}, tools={used_tools})"
                    );
                }
            }
        }
    }

    #[test]
    fn announced_intent_then_closing_silence_fires_the_gate_808() {
        // The live A1 shape that produced 0 firings across 11 opted-in
        // sessions: a long narration BEFORE the last tool ("Let me trace the
        // mechanism."), then a tool, then nothing. Whole-turn text is far
        // over MIN_SUMMARY_CHARS and trips no looks_incomplete phrase, but
        // the closing text is empty — the gate must fire.
        let whole_turn = "Confirmed a live defect. Let me trace the mechanism.";
        assert!(
            !looks_incomplete(whole_turn),
            "precondition: this tail is not caught by the phrase list — if it              ever is, this test is pinning the wrong branch"
        );
        assert!(should_auto_continue(whole_turn, "", 0, true, true));

        // Same turn with a real closing summary after the last tool: no fire.
        assert!(!should_auto_continue(
            whole_turn,
            "Done — assessment written to session_plan/assessment.md.",
            0,
            true,
            true
        ));

        // Off-mode unchanged: without the opt-in the closing text is ignored.
        assert!(!should_auto_continue(whole_turn, "", 0, true, false));
    }

    #[test]
    fn should_auto_continue_on_silence_fires_only_after_tool_use() {
        // The abstention case: tools ran, then silence, nothing queued.
        assert!(should_auto_continue("", "", 0, true, true));
        assert!(should_auto_continue("   \n ", "   \n ", 0, true, true));
        assert!(should_auto_continue("done", "done", 0, true, true));

        // A plain chat turn that returns nothing must NOT loop — no tools ran,
        // so there is no work left on the table, just a model declining to speak.
        assert!(!should_auto_continue("", "", 0, false, true));
        assert!(!should_auto_continue("   \n ", "   \n ", 0, false, true));
        assert!(!should_auto_continue("ok", "ok", 0, false, true));
    }

    #[test]
    fn should_auto_continue_on_silence_leaves_complete_turns_alone() {
        // Real closing prose → false even with the flag on and tools used.
        let complete = "Done — all tests pass and the build is green.";
        assert!(!looks_incomplete(complete));
        assert!(!should_auto_continue(complete, complete, 0, true, true));

        // Pending queue still wins regardless of the flag or tool usage.
        for used_tools in [false, true] {
            for enabled in [false, true] {
                assert!(should_auto_continue(
                    complete, complete, 1, used_tools, enabled
                ));
            }
        }
    }

    #[test]
    fn should_auto_continue_on_silence_boundary_and_whitespace() {
        // MIN_SUMMARY_CHARS is a trimmed *char* count, exclusive: exactly at the
        // boundary is a summary, one under is silence.
        let at_boundary = "a".repeat(MIN_SUMMARY_CHARS);
        let under_boundary = "a".repeat(MIN_SUMMARY_CHARS - 1);
        assert_eq!(at_boundary.chars().count(), MIN_SUMMARY_CHARS);
        assert!(!should_auto_continue(
            &at_boundary,
            &at_boundary,
            0,
            true,
            true
        ));
        assert!(should_auto_continue(
            &under_boundary,
            &under_boundary,
            0,
            true,
            true
        ));

        // Padding with whitespace doesn't buy length — it's trimmed first.
        let padded = format!("   {under_boundary}   ");
        assert!(should_auto_continue(&padded, &padded, 0, true, true));

        // Multi-byte text at the boundary counts chars, not bytes (19 chars,
        // 57 bytes) — the byte count would clear the threshold, chars don't.
        let multibyte = "✓".repeat(MIN_SUMMARY_CHARS - 1);
        assert!(multibyte.len() > MIN_SUMMARY_CHARS);
        assert!(should_auto_continue(&multibyte, &multibyte, 0, true, true));
    }

    #[test]
    fn test_looks_incomplete_continuation_phrases() {
        assert!(looks_incomplete(
            "I've fixed the first file. Next, I'll update the remaining tests."
        ));
        assert!(looks_incomplete(
            "The build passes now. Let me continue with the second module."
        ));
        assert!(looks_incomplete(
            "That's done. I'll now update the configuration."
        ));
        assert!(looks_incomplete(
            "Changes applied successfully. Moving on to the documentation."
        ));
        assert!(looks_incomplete(
            "First part is complete. Now I need to handle the edge cases."
        ));
        assert!(looks_incomplete(
            "I still need to update the tests for this module."
        ));
    }

    #[test]
    fn test_looks_incomplete_remaining_pattern() {
        assert!(looks_incomplete(
            "I've handled 3 of the 5 files. The remaining files need the same fix."
        ));
        assert!(looks_incomplete(
            "Two remaining tasks need to be completed."
        ));
    }

    #[test]
    fn test_looks_incomplete_trailing_ellipsis() {
        // Ellipsis with corroborating continuation signals — should trigger
        assert!(looks_incomplete("I'll update these files next..."));
        assert!(looks_incomplete("There are a few more things to handle…"));
        assert!(looks_incomplete("I need to fix the remaining modules..."));
        assert!(looks_incomplete("Let me continue with the next file..."));
    }

    #[test]
    fn test_looks_incomplete_trailing_ellipsis_prose_no_trigger() {
        // Ellipsis at end of completed prose — should NOT trigger
        assert!(!looks_incomplete(
            "The function handles errors, logging, caching..."
        ));
        assert!(!looks_incomplete(
            "It supports JSON, YAML, TOML, and other formats..."
        ));
        assert!(!looks_incomplete(
            "There are several approaches to consider..."
        ));
        assert!(!looks_incomplete("Everything works now: auth, DB, API..."));
        // Short trailing-off summary
        assert!(!looks_incomplete(
            "The changes cover validation, parsing, serialization..."
        ));
    }

    #[test]
    fn test_looks_incomplete_numbered_steps() {
        let text = "Here's the plan:\n\
                     1. Fix the parser\n\
                     2. Update tests\n\
                     3. Fix formatting\n\
                     4. Update docs\n\n\
                     I've completed step 1. Fixed the parser.\n\
                     2. Now updating the tests.";
        assert!(looks_incomplete(text));
    }

    #[test]
    fn test_looks_incomplete_completed_work() {
        // These should NOT trigger
        assert!(!looks_incomplete(
            "All done! The tests pass and everything is working correctly."
        ));
        assert!(!looks_incomplete(
            "I've completed all the changes. The build succeeds and tests pass."
        ));
        assert!(!looks_incomplete(
            "Everything looks good. All 5 files have been updated successfully."
        ));
        assert!(!looks_incomplete(
            "The refactoring is complete. Here's a summary of what changed."
        ));
    }

    #[test]
    fn test_looks_incomplete_step_number_extraction() {
        assert_eq!(extract_step_number("step 3: do something"), Some(3));
        assert_eq!(extract_step_number("**step 2**"), Some(2));
        assert_eq!(extract_step_number("1. first item"), Some(1));
        assert_eq!(extract_step_number("12) twelfth item"), Some(12));
        assert_eq!(extract_step_number("no step here"), None);
        assert_eq!(extract_step_number(""), None);
    }

    #[test]
    fn test_looks_incomplete_unclosed_code_block() {
        // Odd number of ``` fences = unclosed code block
        assert!(looks_incomplete(
            "Here's the fix:\n```rust\nfn main() {\n    println!(\"hello\");\n"
        ));
        // Even number = properly closed
        assert!(!looks_incomplete(
            "Here's the fix:\n```rust\nfn main() {}\n```\nAll done."
        ));
        // Multiple blocks, last one unclosed
        assert!(looks_incomplete(
            "First block:\n```\nfoo\n```\nSecond block:\n```\nbar\n"
        ));
    }

    #[test]
    fn test_looks_incomplete_action_phrases() {
        assert!(looks_incomplete(
            "I've reviewed the code. Let me update the configuration file now."
        ));
        assert!(looks_incomplete(
            "Found the bug. Let me fix the parser to handle this edge case."
        ));
        assert!(looks_incomplete(
            "Good, that worked. I'll implement the remaining handlers."
        ));
        assert!(looks_incomplete(
            "Tests pass. Let me create the new module for this feature."
        ));
        assert!(looks_incomplete(
            "That looks correct. Let me write the documentation for it."
        ));
        assert!(looks_incomplete(
            "Alright, I'll add the missing error handling next."
        ));
        assert!(looks_incomplete(
            "The structure looks right. Let me handle the edge case."
        ));
        assert!(looks_incomplete(
            "I see the issue. I'll modify the return type to fix it."
        ));
    }

    #[test]
    fn test_looks_incomplete_ordinal_progression() {
        // "first" without "finally" or "second" — more steps expected
        assert!(looks_incomplete(
            "First, I'll fix the parser. Then we need to update the tests and docs."
        ));
        // "first" and "second" without "third" or "finally" — still incomplete
        assert!(looks_incomplete(
            "First, I fixed the parser. Second, I updated the tests."
        ));
        // "first" with "finally" — complete
        assert!(!looks_incomplete(
            "First, I fixed the parser. Second, the tests. Finally, the docs are updated."
        ));
    }

    #[test]
    fn test_looks_incomplete_step_x_of_y() {
        assert!(looks_incomplete(
            "That's step 2 of 5 done. Moving to the next one."
        ));
        assert!(looks_incomplete(
            "Completed step 1 of 4. The parser is now fixed."
        ));
        assert!(looks_incomplete(
            "I've finished step 3/7 for this refactoring."
        ));
        // Step X of Y where X == Y — complete, should NOT trigger
        assert!(!looks_incomplete(
            "That's step 5 of 5 done. Everything is complete."
        ));
        // Step X of Y where X > Y — should NOT trigger
        assert!(!looks_incomplete(
            "We ended up doing step 6 of 5 as a bonus. All done!"
        ));
    }

    #[test]
    fn test_looks_incomplete_negative_cases_new_patterns() {
        // Completed work — should NOT trigger any pattern
        assert!(!looks_incomplete(
            "I've updated all the files and the tests pass. Everything is working."
        ));
        assert!(!looks_incomplete(
            "```rust\nfn main() {}\n```\nThe implementation is complete and all tests pass."
        ));
        assert!(!looks_incomplete(
            "First, I fixed the parser. Second, the tests. Finally, the documentation. All done!"
        ));
    }

    #[test]
    fn test_looks_incomplete_ordinal_completed_work() {
        // "First" used as a discourse marker in completed explanations —
        // should NOT trigger when work is clearly done
        assert!(!looks_incomplete(
            "First, here's what I changed: the parser now handles JSON correctly."
        ));
        assert!(!looks_incomplete(
            "First off, the build passes. All tests are green."
        ));
        assert!(!looks_incomplete(
            "The first thing to note is that this approach works well."
        ));
    }

    #[test]
    fn test_looks_incomplete_completion_guard() {
        // When the tail contains strong completion signals, action phrases in
        // summary/recap context should NOT trigger auto-continue.
        assert!(!looks_incomplete(
            "I've finished all the changes. Let me update you on what I did: the parser is fixed."
        ));
        assert!(!looks_incomplete(
            "All done! Let me write a brief summary of everything that changed."
        ));
        assert!(!looks_incomplete(
            "The implementation is complete. Let me add a note: test coverage is at 95%."
        ));
        assert!(!looks_incomplete(
            "Everything is working now. I'll create a recap of the three files I modified."
        ));
        assert!(!looks_incomplete(
            "That's all the changes. Let me update the summary for you."
        ));
        assert!(!looks_incomplete(
            "All changes have been applied successfully. Let me handle the final summary."
        ));

        // But action phrases WITHOUT completion signals should still trigger
        assert!(looks_incomplete(
            "I've reviewed the code. Let me update the configuration file now."
        ));
        assert!(looks_incomplete(
            "Found the bug. Let me fix the parser to handle this edge case."
        ));
        assert!(looks_incomplete(
            "Good, that worked. I'll implement the remaining handlers."
        ));
    }

    #[test]
    fn test_looks_incomplete_completion_guard_with_ellipsis() {
        // Completion + ellipsis: should NOT trigger even with work signals,
        // because the completion phrase overrides.
        assert!(!looks_incomplete(
            "Everything is done. The changes cover auth, DB, API..."
        ));
        assert!(!looks_incomplete(
            "All complete. I'll add that the tests handle edge cases..."
        ));
    }

    #[test]
    fn test_step_x_of_y_incomplete() {
        assert_eq!(step_x_of_y_incomplete("step 2 of 5"), Some(true));
        assert_eq!(step_x_of_y_incomplete("step 3/7"), Some(true));
        assert_eq!(step_x_of_y_incomplete("step 1 of 4 done"), Some(true));
        assert_eq!(step_x_of_y_incomplete("step 5 of 5"), Some(false));
        assert_eq!(step_x_of_y_incomplete("step 7 of 5"), Some(false));
        assert_eq!(step_x_of_y_incomplete("no steps here"), None);
    }

    #[test]
    fn test_step_x_of_y_incomplete_non_ascii() {
        // LLM output routinely contains em-dashes, smart quotes, emoji, etc.
        // These are multi-byte UTF-8 characters. Byte-level iteration would
        // panic when indexing lands inside a multi-byte char boundary.
        assert_eq!(
            step_x_of_y_incomplete("here's — step 2 of 5 remaining"),
            Some(true)
        );
        assert_eq!(
            step_x_of_y_incomplete("✓ done — step 5 of 5 complete"),
            Some(false)
        );
        assert_eq!(
            step_x_of_y_incomplete("→ step 3/7: update the \u{201C}config\u{201D}"),
            Some(true)
        );
        // Pure non-ASCII with no step pattern
        assert_eq!(step_x_of_y_incomplete("全部完成 — 没有更多步骤"), None);
        // Emoji right before "step"
        assert_eq!(step_x_of_y_incomplete("🔧step 1 of 3"), Some(true));
    }

    #[test]
    fn test_get_max_auto_continues_default() {
        let config = std::collections::HashMap::new();
        assert_eq!(get_max_auto_continues(&config, false), 5);
    }

    #[test]
    fn test_get_max_auto_continues_disabled() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_continue".to_string(), "false".to_string());
        assert_eq!(get_max_auto_continues(&config, false), 0);
    }

    #[test]
    fn test_get_max_auto_continues_custom_value() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "8".to_string());
        assert_eq!(get_max_auto_continues(&config, false), 8);
    }

    #[test]
    fn test_get_max_auto_continues_plan_apply_raises_minimum() {
        let config = std::collections::HashMap::new();
        // Default is 5, but plan-apply mode raises minimum to 10
        assert_eq!(get_max_auto_continues(&config, true), 10);
    }

    #[test]
    fn test_get_max_auto_continues_plan_apply_keeps_higher() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "15".to_string());
        // User set 15, plan-apply minimum is 10, so 15 wins
        assert_eq!(get_max_auto_continues(&config, true), 15);
    }

    #[test]
    fn test_get_max_auto_continues_disabled_overrides_plan_apply() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_continue".to_string(), "false".to_string());
        // Disabled means 0 even in plan-apply mode
        assert_eq!(get_max_auto_continues(&config, true), 0);
    }

    #[test]
    fn test_get_max_auto_continues_zero_explicit() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "0".to_string());
        assert_eq!(get_max_auto_continues(&config, false), 0);
    }

    #[test]
    fn test_format_followup_hint_empty_is_none() {
        // Zero pending → no output (silent, product-safe no-op).
        assert_eq!(format_followup_hint(0, "anything"), None);
        // Non-zero count but blank first item → still None.
        assert_eq!(format_followup_hint(3, ""), None);
        assert_eq!(format_followup_hint(2, "   "), None);
    }

    #[test]
    fn test_format_followup_hint_basic() {
        let hint = format_followup_hint(2, "run the tests").expect("should produce a hint");
        assert!(hint.contains("2 pending"));
        assert!(hint.contains("run the tests"));
        assert!(hint.contains('↻'));
    }

    #[test]
    fn test_format_followup_hint_multibyte_no_panic() {
        // A multi-byte first character (✓ is 3 bytes, 🚀 is 4 bytes) must not
        // panic during truncation.
        let first = "✓ done — now 🚀 deploy the release to production servers";
        let hint = format_followup_hint(1, first).expect("should produce a hint");
        assert!(hint.contains("1 pending"));
        assert!(hint.contains('✓'));
        // Repeat with a very long multi-byte string to force truncation.
        let long = "🚀".repeat(200);
        let hint2 = format_followup_hint(5, &long).expect("should produce a hint");
        assert!(hint2.contains("5 pending"));
    }

    #[test]
    fn test_format_followup_hint_long_truncated() {
        let long = "a".repeat(200);
        let hint = format_followup_hint(1, &long).expect("should produce a hint");
        // Long item is truncated with an ellipsis.
        assert!(hint.contains('…'));
        // The full 200-char item is not reproduced verbatim.
        assert!(!hint.contains(&long));
    }
}
