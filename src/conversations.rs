//! Side, quick, and extended conversation handlers.
//!
//! Extracted from `repl.rs` — these are self-contained conversation modes
//! that don't depend on the REPL loop infrastructure.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::commands;
use crate::format::*;
use crate::prompt::run_prompt_auto_retry;
use crate::session::SessionChanges;
use crate::sync_util::lock_or_recover;
use crate::watch::run_watch_after_prompt;
use crate::AgentConfig;
use yoagent::context::ExecutionLimits;
use yoagent::*;

/// Build content blocks from `/add` results, ensuring images always have
/// accompanying text context so the model can see them.
///
/// For each `AddResult::Image`, a `Content::Text` label is inserted *before*
/// the `Content::Image` block (e.g. `"[Image: photo.png (42 KB, image/png)]"`).
/// If the entire batch contains only images (no text files), a general
/// introductory text block is prepended at the start.
pub fn build_add_content_blocks(results: &[commands::AddResult]) -> Vec<yoagent::types::Content> {
    if results.is_empty() {
        return Vec::new();
    }

    let mut blocks: Vec<yoagent::types::Content> = Vec::new();

    let has_text_file = results
        .iter()
        .any(|r| matches!(r, commands::AddResult::Text { .. }));

    // If there are only images and no text files, prepend a contextual intro
    if !has_text_file {
        blocks.push(yoagent::types::Content::Text {
            text: "The user is sharing the following image(s) for you to analyze:".to_string(),
        });
    }

    for result in results {
        match result {
            commands::AddResult::Text { content, .. } => {
                blocks.push(yoagent::types::Content::Text {
                    text: content.clone(),
                });
            }
            commands::AddResult::Image {
                summary,
                data,
                mime_type,
            } => {
                // Extract a readable label from the summary (which contains the
                // filename, size, and mime type). The summary looks like:
                //   "\x1b[32m  ✓ added image photo.png (42 KB, image/png)\x1b[0m"
                // We extract what's between "added image " and the RESET code,
                // but if parsing fails, fall back to the mime_type alone.
                let label = extract_image_label(summary, mime_type);
                blocks.push(yoagent::types::Content::Text {
                    text: format!("[Image: {label}]"),
                });
                blocks.push(yoagent::types::Content::Image {
                    data: data.clone(),
                    mime_type: mime_type.clone(),
                });
            }
        }
    }

    blocks
}

/// Extract a human-readable label from an AddResult::Image summary string.
/// The summary has ANSI codes and looks like:
///   "\x1b[32m  ✓ added image photo.png (42 KB, image/png)\x1b[0m"
/// We want: "photo.png (42 KB, image/png)"
fn extract_image_label(summary: &str, fallback_mime: &str) -> String {
    // Strip ANSI escape codes first
    let stripped: String = {
        let mut out = String::new();
        let mut in_escape = false;
        for ch in summary.chars() {
            if ch == '\x1b' {
                in_escape = true;
            } else if in_escape {
                if ch.is_ascii_alphabetic() {
                    in_escape = false;
                }
            } else {
                out.push(ch);
            }
        }
        out
    };

    // Try to find "added image " and extract everything after it
    if let Some(idx) = stripped.find("added image ") {
        let after = &stripped[idx + "added image ".len()..];
        let trimmed = after.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // Fallback
    format!("image ({fallback_mime})")
}

// ── Side conversations ──

/// Parse a `/side` question from the input. Returns `None` if no question provided.
fn parse_side_question(input: &str) -> Option<String> {
    let question = input.strip_prefix("/side").unwrap_or("").trim().to_string();
    if question.is_empty() {
        None
    } else {
        Some(question)
    }
}

/// A single stored side exchange — the last `/side` question and its answer.
/// Mirrors the "last bang result" pattern in `repl.rs`: one slot, latest wins,
/// so `/side pull` can carry the most recent side answer back into the main
/// conversation without the user retyping it.
#[derive(Clone, Debug, PartialEq)]
struct LastSide {
    question: String,
    answer: String,
}

/// Module-level store for the last side exchange. One slot: each new answered
/// `/side` overwrites the previous. Poison-safe via `lock_or_recover` (Day 58).
static LAST_SIDE: Mutex<Option<LastSide>> = Mutex::new(None);

/// Save the last side exchange (overwriting any prior one — one slot, latest wins).
fn store_last_side(question: &str, answer: &str) {
    let mut guard = lock_or_recover(&LAST_SIDE);
    *guard = Some(LastSide {
        question: question.to_string(),
        answer: answer.to_string(),
    });
}

/// Return the last stored side exchange as `(question, answer)`, if any.
///
/// This is **non-consuming** (clones): unlike the one-rewind-per-clear pattern in
/// `commands_stash`, a pulled side answer may be referenced again later, so we
/// leave the slot intact and let a newer `/side` overwrite it.
fn last_side() -> Option<(String, String)> {
    let guard = lock_or_recover(&LAST_SIDE);
    guard
        .as_ref()
        .map(|s| (s.question.clone(), s.answer.clone()))
}

/// Returns `true` if the `/side` input is exactly the `pull` subcommand
/// (i.e. `/side pull` with optional surrounding whitespace).
fn is_side_pull(input: &str) -> bool {
    input.strip_prefix("/side").unwrap_or("").trim() == "pull"
}

/// Handle a `/side <question>` command — quick Q&A without touching main context.
///
/// `/side pull` is special: it does not run a new query. Instead it fetches the
/// last answered `/side` exchange and injects that answer into the MAIN
/// conversation (the return path for the otherwise fire-and-forget side chat).
pub(crate) async fn handle_side(
    input: &str,
    agent: &mut yoagent::agent::Agent,
    agent_config: &AgentConfig,
) {
    // `/side pull` — bring the last side answer back into the main conversation.
    if is_side_pull(input) {
        handle_side_pull(agent);
        return;
    }

    let question = match parse_side_question(input) {
        Some(q) => q,
        None => {
            eprintln!(
                "{YELLOW}  Usage: /side <question>{RESET}\n\
                 {DIM}  Ask a quick question without affecting the main conversation.\n\
                 {DIM}  No tools — just text Q&A. Fast and cheap.\n\
                 {DIM}  /side pull carries the last side answer into the main conversation.\n\n\
                 {DIM}  Examples:\n\
                 {DIM}    /side what's the syntax for a match guard in Rust?\n\
                 {DIM}    /side explain the difference between clone and copy\n\
                 {DIM}    /side pull{RESET}\n"
            );
            return;
        }
    };

    eprintln!("{DIM}  [side] thinking...{RESET}");

    let mut side_agent = agent_config.build_side_agent();
    let mut rx = side_agent.prompt(&question).await;

    let mut md_renderer = MarkdownRenderer::new();
    let mut collected_text = String::new();
    let mut started = false;

    loop {
        match rx.recv().await {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => {
                if !started {
                    // Print a side-conversation header on first text
                    print!("\n{DIM}[side]{RESET} ");
                    started = true;
                }
                collected_text.push_str(&delta);
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

    side_agent.finish().await;

    if !started {
        eprintln!("{DIM}  [side] (no response){RESET}");
    } else {
        println!(); // newline after streamed text
    }

    // Remember this exchange so `/side pull` can carry the answer back into the
    // main conversation (one slot, latest wins).
    let trimmed_answer = collected_text.trim();
    if !trimmed_answer.is_empty() {
        store_last_side(&question, trimmed_answer);
    }

    // Show side conversation cost
    let messages = side_agent.messages();
    let mut side_usage = Usage::default();
    for msg in messages {
        if let AgentMessage::Llm(yoagent::types::Message::Assistant { usage, .. }) = msg {
            side_usage.input += usage.input;
            side_usage.output += usage.output;
            side_usage.cache_read += usage.cache_read;
            side_usage.cache_write += usage.cache_write;
        }
    }
    let total_tokens = side_usage.input + side_usage.output;
    if total_tokens > 0 {
        let cost = estimate_cost(&side_usage, &agent_config.model);
        if let Some(c) = cost {
            eprintln!("{DIM}  [side] {} tokens, ${:.4}{RESET}\n", total_tokens, c);
        } else {
            eprintln!("{DIM}  [side] {} tokens{RESET}\n", total_tokens);
        }
    } else {
        eprintln!();
    }
}

/// Handle `/side pull` — inject the last side answer into the MAIN conversation.
///
/// This is the return path for the otherwise fire-and-forget `/side` chat
/// (Day-127 lesson: any feature that isolates work implies a way back). The
/// stored answer is appended as a user-visible context message so the main
/// agent can see and build on it, exactly like `/add` injects file content.
fn handle_side_pull(agent: &mut yoagent::agent::Agent) {
    let (question, answer) = match last_side() {
        Some(pair) => pair,
        None => {
            eprintln!("{DIM}  [side] nothing to pull — ask a /side question first{RESET}\n");
            return;
        }
    };

    let block = format!("Context from a side conversation.\n\nQ: {question}\n\nA: {answer}");
    let msg = yoagent::types::AgentMessage::Llm(yoagent::types::Message::User {
        content: vec![yoagent::types::Content::Text { text: block }],
        timestamp: yoagent::types::now_ms(),
    });
    agent.append_message(msg);
    eprintln!("{DIM}  [side] pulled last answer into the conversation{RESET}\n");
}

// ── Quick mode ──

fn parse_quick_question(input: &str) -> Option<String> {
    let question = input
        .strip_prefix("/quick")
        .unwrap_or("")
        .trim()
        .to_string();
    if question.is_empty() {
        None
    } else {
        Some(question)
    }
}

/// Handle a `/quick <question>` command — fast single-turn answer without tools or agent loop.
pub(crate) async fn handle_quick(input: &str, agent_config: &AgentConfig) {
    let question = match parse_quick_question(input) {
        Some(q) => q,
        None => {
            eprintln!(
                "{YELLOW}  Usage: /quick <question>{RESET}\n\
                 {DIM}  Fast single-turn answer without tools or agent loop.\n\
                 {DIM}  Great for quick lookups, syntax help, and explanations.\n\n\
                 {DIM}  Examples:\n\
                 {DIM}    /quick what does this error mean: borrow of moved value?\n\
                 {DIM}    /quick how do I use sed to replace X with Y?\n\
                 {DIM}    /quick explain the difference between async and threading{RESET}\n"
            );
            return;
        }
    };

    eprintln!("{DIM}  [quick] thinking...{RESET}");

    let mut side_agent = agent_config.build_side_agent();
    let mut rx = side_agent.prompt(&question).await;

    let mut md_renderer = MarkdownRenderer::new();
    let mut collected_text = String::new();
    let mut started = false;

    loop {
        match rx.recv().await {
            Some(AgentEvent::MessageUpdate {
                delta: StreamDelta::Text { delta },
                ..
            }) => {
                if !started {
                    print!("\n{DIM}[quick]{RESET} ");
                    started = true;
                }
                collected_text.push_str(&delta);
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

    side_agent.finish().await;

    if !started {
        eprintln!("{DIM}  [quick] (no response){RESET}");
    } else {
        println!(); // newline after streamed text
    }

    // Show quick query cost
    let messages = side_agent.messages();
    let mut quick_usage = Usage::default();
    for msg in messages {
        if let AgentMessage::Llm(yoagent::types::Message::Assistant { usage, .. }) = msg {
            quick_usage.input += usage.input;
            quick_usage.output += usage.output;
            quick_usage.cache_read += usage.cache_read;
            quick_usage.cache_write += usage.cache_write;
        }
    }
    let total_tokens = quick_usage.input + quick_usage.output;
    if total_tokens > 0 {
        let cost = estimate_cost(&quick_usage, &agent_config.model);
        if let Some(c) = cost {
            eprintln!("{DIM}  [quick] {} tokens, ${:.4}{RESET}\n", total_tokens, c);
        } else {
            eprintln!("{DIM}  [quick] {} tokens{RESET}\n", total_tokens);
        }
    } else {
        eprintln!();
    }
}

// ── Extended mode ──

/// Parse the `/extended` command input, extracting the prompt, optional `--turns N`,
/// and optional `--budget N` (time limit in minutes).
///
/// Returns `(prompt, explicit_turns, budget)`. `explicit_turns` is `Some(n)` **only**
/// when the user actually wrote `--turns n` — absence is its own value, not
/// `DEFAULT_EXTENDED_TURNS`, because "user asked for a cap" and "user said nothing"
/// resolve to different enforced limits (see [`extended_turn_limit`]).
/// If `--budget N` is present, it is stripped and returned as `Some(Duration)`.
fn parse_extended_args(input: &str) -> (String, Option<usize>, Option<Duration>) {
    let raw = input
        .strip_prefix("/extended")
        .unwrap_or(input)
        .trim()
        .to_string();

    // Look for --turns N and --budget N anywhere in the string
    let mut turns: Option<usize> = None;
    let mut budget: Option<Duration> = None;
    let mut prompt_parts: Vec<&str> = Vec::new();
    let words: Vec<&str> = raw.split_whitespace().collect();
    let mut skip_next = false;

    for (i, word) in words.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if *word == "--turns" {
            if let Some(next) = words.get(i + 1) {
                if let Ok(n) = next.parse::<usize>() {
                    turns = Some(n.max(1)); // At least 1 turn
                    skip_next = true;
                    continue;
                }
            }
        }
        if *word == "--budget" {
            if let Some(next) = words.get(i + 1) {
                if let Ok(mins) = next.parse::<u64>() {
                    if mins > 0 {
                        budget = Some(Duration::from_secs(mins * 60));
                    }
                    skip_next = true;
                    continue;
                }
            }
        }
        prompt_parts.push(word);
    }

    let prompt = prompt_parts.join(" ");
    (prompt, turns, budget)
}

/// What `/extended` will actually enforce for this run.
///
/// `/extended` used to print "N turns max" and inject N into the prompt text while
/// enforcing nothing — the flag was a suggestion to the model, not a limit. This type
/// makes the three cases explicit so the banner can report the number that is really
/// installed on the agent, including the case where nothing is enforced at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExtendedTurnLimit {
    /// A turn cap is enforced for this run.
    Enforced(usize),
    /// No turn cap is enforced: the agent has execution limits disabled and the user
    /// did not ask for one. This is an explicit third value, not `Enforced(0)`.
    Unlimited,
}

/// Decide the turn limit `/extended` should enforce.
///
/// - The user wrote `--turns n` → enforce `n` (even when the agent has no limits).
/// - Otherwise the agent's own configured `max_turns` stands — `/extended` never
///   silently *lowers* a limit the user configured elsewhere.
/// - No flag and no configured limits → nothing is enforced.
pub(crate) fn extended_turn_limit(
    current: Option<&ExecutionLimits>,
    explicit_turns: Option<usize>,
) -> ExtendedTurnLimit {
    match (explicit_turns, current) {
        (Some(n), _) => ExtendedTurnLimit::Enforced(n.max(1)),
        (None, Some(limits)) => ExtendedTurnLimit::Enforced(limits.max_turns),
        (None, None) => ExtendedTurnLimit::Unlimited,
    }
}

/// Build the `ExecutionLimits` to install on the agent for one `/extended` run,
/// preserving every other limit (tokens, duration) from the agent's current config.
pub(crate) fn extended_limits_override(
    current: Option<&ExecutionLimits>,
    max_turns: usize,
) -> ExecutionLimits {
    let mut limits = current.cloned().unwrap_or_default();
    limits.max_turns = max_turns.max(1);
    limits
}

/// Human-readable turn description for the extended-mode banner.
fn extended_turns_label(limit: ExtendedTurnLimit) -> String {
    match limit {
        ExtendedTurnLimit::Enforced(n) => format!("{n} turns max"),
        ExtendedTurnLimit::Unlimited => "no turn limit".to_string(),
    }
}

/// Build the system-level instruction for extended autonomous mode.
fn build_extended_system_prompt(task: &str, limit: ExtendedTurnLimit) -> String {
    let turn_rule = match limit {
        ExtendedTurnLimit::Enforced(n) => {
            format!("- You have up to {n} turns to complete this task.")
        }
        ExtendedTurnLimit::Unlimited => {
            "- No turn limit is enforced — stop as soon as the task is done.".to_string()
        }
    };
    format!(
        "You are in EXTENDED AUTONOMOUS MODE. Work on this task step by step:\n\n\
         {task}\n\n\
         Rules for extended mode:\n\
         - Work autonomously — do NOT ask the user questions. Make your best judgment.\n\
         - Break the task into steps and execute them one at a time.\n\
         - Run tests after making changes to verify correctness.\n\
         - If you get stuck, explain what you tried and move on.\n\
         {turn_rule}\n\
         - When the task is complete, summarize what you did and what files were modified."
    )
}

/// Handle the `/extended` command — run the agent in autonomous mode with a turn budget.
pub(crate) async fn handle_extended(
    input: &str,
    agent: &mut yoagent::agent::Agent,
    session_total: &mut Usage,
    model: &str,
    session_changes: &SessionChanges,
) -> Option<String> {
    let (prompt, explicit_turns, budget) = parse_extended_args(input);

    if prompt.is_empty() {
        eprintln!(
            "{YELLOW}  Usage: /extended <task description> [--turns N] [--budget N]{RESET}\n\
             {DIM}  Run the agent autonomously on a task.\n\
             {DIM}  --turns N caps turns for this run (default: the agent's configured limit).\n\
             {DIM}  --budget N sets a wall-clock time limit in minutes.\n\
             \n\
             {DIM}  Examples:\n\
             {DIM}    /extended add error handling to the parser module\n\
             {DIM}    /extended refactor the auth system --turns 30\n\
             {DIM}    /extended rebuild the test suite --budget 15{RESET}\n"
        );
        return None;
    }

    let budget_label = if let Some(dur) = budget {
        format!(" | budget: {} min", dur.as_secs() / 60)
    } else {
        String::new()
    };

    let limit = extended_turn_limit(agent.execution_limits.as_ref(), explicit_turns);
    let turns_label = extended_turns_label(limit);

    eprintln!(
        "\n{BOLD_CYAN}  🐙 Extended mode{RESET} — working autonomously ({turns_label}{budget_label})\n\
         {DIM}  Task: {prompt}{RESET}\n"
    );

    let extended_prompt = build_extended_system_prompt(&prompt, limit);

    // Actually enforce the turn cap for this run. Before Day 151 the parsed
    // `--turns N` only reached the prompt *text* — the agent's real
    // `ExecutionLimits` were untouched, so the banner and the prompt both
    // claimed a cap that nothing enforced (silent wrong-op). Save the previous
    // limits and restore them after the run so `/extended` stays scoped.
    let saved_limits = agent.execution_limits.clone();
    if let ExtendedTurnLimit::Enforced(n) = limit {
        agent.execution_limits = Some(extended_limits_override(saved_limits.as_ref(), n));
    }

    // Run the task using the existing prompt infrastructure with auto-retry.
    // If a budget is set, wrap in tokio::time::timeout.
    let prompt_start = Instant::now();
    let timed_out;

    if let Some(dur) = budget {
        match tokio::time::timeout(
            dur,
            run_prompt_auto_retry(
                agent,
                &extended_prompt,
                session_total,
                model,
                session_changes,
            ),
        )
        .await
        {
            Ok(_outcome) => {
                timed_out = false;
            }
            Err(_elapsed) => {
                timed_out = true;
            }
        }
    } else {
        let _outcome = run_prompt_auto_retry(
            agent,
            &extended_prompt,
            session_total,
            model,
            session_changes,
        )
        .await;
        timed_out = false;
    }

    let elapsed = prompt_start.elapsed();

    // Restore the agent's previous limits — the cap applies to this run only.
    agent.execution_limits = saved_limits;

    if timed_out {
        let budget_mins = budget.map(|d| d.as_secs() / 60).unwrap_or(0);
        eprintln!(
            "\n{YELLOW}  🐙 Extended mode — time budget exhausted ({budget_mins} min){RESET}"
        );
    }

    // Run watch command after prompt if active (auto lint/test loop)
    if !timed_out {
        run_watch_after_prompt(agent, session_total, model, session_changes).await;
    }

    // Summary
    let files_changed = session_changes.snapshot().len();
    eprintln!(
        "\n{BOLD_CYAN}  🐙 Extended mode complete{RESET}\n\
         {DIM}  Time: {elapsed:.1?} | Files modified: {files_changed}{RESET}\n"
    );

    // Return the prompt so it can be set as last_input for /retry
    Some(extended_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── build_add_content_blocks tests ──

    // ── build_add_content_blocks ─────────────────────────────────────

    #[test]
    fn add_content_blocks_image_only_has_intro_and_label() {
        let results = vec![commands::AddResult::Image {
            summary: "\x1b[32m  ✓ added image photo.png (42 KB, image/png)\x1b[0m".to_string(),
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        }];
        let blocks = build_add_content_blocks(&results);

        // Should be: intro text, label text, image = 3 blocks
        assert_eq!(blocks.len(), 3, "expected intro + label + image");

        // First block: introductory text
        match &blocks[0] {
            yoagent::types::Content::Text { text } => {
                assert!(
                    text.contains("image(s)"),
                    "intro should mention images: {text}"
                );
            }
            other => panic!("expected Text intro, got {other:?}"),
        }

        // Second block: image label text
        match &blocks[1] {
            yoagent::types::Content::Text { text } => {
                assert!(
                    text.starts_with("[Image:"),
                    "label should start with [Image:: {text}"
                );
                assert!(
                    text.contains("photo.png"),
                    "label should contain filename: {text}"
                );
            }
            other => panic!("expected Text label, got {other:?}"),
        }

        // Third block: actual image
        match &blocks[2] {
            yoagent::types::Content::Image { data, mime_type } => {
                assert_eq!(data, "base64data");
                assert_eq!(mime_type, "image/png");
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn add_content_blocks_text_only_no_intro() {
        let results = vec![commands::AddResult::Text {
            summary: "added foo.rs".to_string(),
            content: "fn main() {}".to_string(),
        }];
        let blocks = build_add_content_blocks(&results);

        // Text-only: no intro, just the text block
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            yoagent::types::Content::Text { text } => {
                assert_eq!(text, "fn main() {}");
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn add_content_blocks_mixed_text_and_image() {
        let results = vec![
            commands::AddResult::Text {
                summary: "added main.rs".to_string(),
                content: "fn main() {}".to_string(),
            },
            commands::AddResult::Image {
                summary: "\x1b[32m  ✓ added image logo.png (10 KB, image/png)\x1b[0m".to_string(),
                data: "imgdata".to_string(),
                mime_type: "image/png".to_string(),
            },
        ];
        let blocks = build_add_content_blocks(&results);

        // Mixed: no intro (text file present), text + label + image = 3 blocks
        assert_eq!(blocks.len(), 3, "expected text + label + image");

        // First: text file content
        match &blocks[0] {
            yoagent::types::Content::Text { text } => {
                assert_eq!(text, "fn main() {}");
            }
            other => panic!("expected Text, got {other:?}"),
        }

        // Second: image label
        match &blocks[1] {
            yoagent::types::Content::Text { text } => {
                assert!(text.starts_with("[Image:"), "label: {text}");
                assert!(
                    text.contains("logo.png"),
                    "label should have filename: {text}"
                );
            }
            other => panic!("expected Text label, got {other:?}"),
        }

        // Third: image data
        match &blocks[2] {
            yoagent::types::Content::Image { data, mime_type } => {
                assert_eq!(data, "imgdata");
                assert_eq!(mime_type, "image/png");
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn add_content_blocks_multiple_images_each_has_label() {
        let results = vec![
            commands::AddResult::Image {
                summary: "\x1b[32m  ✓ added image a.jpg (5 KB, image/jpeg)\x1b[0m".to_string(),
                data: "d1".to_string(),
                mime_type: "image/jpeg".to_string(),
            },
            commands::AddResult::Image {
                summary: "\x1b[32m  ✓ added image b.webp (8 KB, image/webp)\x1b[0m".to_string(),
                data: "d2".to_string(),
                mime_type: "image/webp".to_string(),
            },
        ];
        let blocks = build_add_content_blocks(&results);

        // intro + (label + image) × 2 = 5 blocks
        assert_eq!(blocks.len(), 5, "expected intro + 2×(label+image)");

        // Verify intro
        assert!(
            matches!(&blocks[0], yoagent::types::Content::Text { text } if text.contains("image(s)"))
        );

        // Verify label-then-image ordering for first image
        assert!(
            matches!(&blocks[1], yoagent::types::Content::Text { text } if text.contains("a.jpg"))
        );
        assert!(matches!(&blocks[2], yoagent::types::Content::Image { data, .. } if data == "d1"));

        // Verify label-then-image ordering for second image
        assert!(
            matches!(&blocks[3], yoagent::types::Content::Text { text } if text.contains("b.webp"))
        );
        assert!(matches!(&blocks[4], yoagent::types::Content::Image { data, .. } if data == "d2"));
    }

    #[test]
    fn add_content_blocks_empty_input() {
        let blocks = build_add_content_blocks(&[]);
        assert!(blocks.is_empty(), "empty input should produce empty output");
    }

    #[test]
    fn extract_image_label_parses_ansi_summary() {
        let label = extract_image_label(
            "\x1b[32m  ✓ added image photo.png (42 KB, image/png)\x1b[0m",
            "image/png",
        );
        assert_eq!(label, "photo.png (42 KB, image/png)");
    }

    #[test]
    fn extract_image_label_fallback() {
        let label = extract_image_label("something unexpected", "image/jpeg");
        assert_eq!(label, "image (image/jpeg)");
    }

    // ── parse_extended_args tests ──

    #[test]
    fn test_parse_extended_args_basic_prompt() {
        let (prompt, turns, budget) = parse_extended_args("/extended build a REST API");
        assert_eq!(prompt, "build a REST API");
        assert_eq!(turns, None, "no --turns flag means no explicit override");
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_with_turns() {
        let (prompt, turns, budget) = parse_extended_args("/extended refactor auth --turns 10");
        assert_eq!(prompt, "refactor auth");
        assert_eq!(turns, Some(10));
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_turns_at_start() {
        let (prompt, turns, budget) = parse_extended_args("/extended --turns 5 fix all bugs");
        assert_eq!(prompt, "fix all bugs");
        assert_eq!(turns, Some(5));
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_turns_in_middle() {
        let (prompt, turns, budget) =
            parse_extended_args("/extended add tests --turns 15 for parser");
        assert_eq!(prompt, "add tests for parser");
        assert_eq!(turns, Some(15));
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_no_prompt() {
        let (prompt, turns, budget) = parse_extended_args("/extended");
        assert!(prompt.is_empty());
        assert_eq!(turns, None, "no --turns flag means no explicit override");
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_turns_minimum_one() {
        let (prompt, turns, budget) = parse_extended_args("/extended do stuff --turns 0");
        assert_eq!(prompt, "do stuff");
        assert_eq!(turns, Some(1)); // Clamped to 1
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_invalid_turns_kept_as_prompt() {
        let (prompt, turns, budget) = parse_extended_args("/extended do stuff --turns abc");
        assert_eq!(prompt, "do stuff --turns abc");
        assert_eq!(turns, None, "no --turns flag means no explicit override");
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_args_turns_without_value() {
        let (prompt, turns, budget) = parse_extended_args("/extended do stuff --turns");
        assert_eq!(prompt, "do stuff --turns");
        assert_eq!(turns, None, "no --turns flag means no explicit override");
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_budget() {
        let (prompt, turns, budget) = parse_extended_args("/extended do stuff --budget 10");
        assert_eq!(prompt, "do stuff");
        assert_eq!(turns, None, "no --turns flag means no explicit override");
        assert_eq!(budget, Some(Duration::from_secs(600)));
    }

    #[test]
    fn test_parse_extended_turns_and_budget() {
        let (prompt, turns, budget) =
            parse_extended_args("/extended rebuild tests --turns 30 --budget 15");
        assert_eq!(prompt, "rebuild tests");
        assert_eq!(turns, Some(30));
        assert_eq!(budget, Some(Duration::from_secs(900)));
    }

    #[test]
    fn test_parse_extended_no_budget() {
        let (prompt, turns, budget) = parse_extended_args("/extended simple task");
        assert_eq!(prompt, "simple task");
        assert_eq!(turns, None, "no --turns flag means no explicit override");
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_budget_zero_ignored() {
        let (prompt, _turns, budget) = parse_extended_args("/extended task --budget 0");
        assert_eq!(prompt, "task");
        // --budget 0 is consumed (skip_next fires) but budget stays None
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_budget_invalid_kept_as_prompt() {
        let (prompt, _turns, budget) = parse_extended_args("/extended task --budget abc");
        assert_eq!(prompt, "task --budget abc");
        assert!(budget.is_none());
    }

    #[test]
    fn test_parse_extended_budget_without_value() {
        let (prompt, _turns, budget) = parse_extended_args("/extended task --budget");
        assert_eq!(prompt, "task --budget");
        assert!(budget.is_none());
    }

    #[test]
    fn test_build_extended_system_prompt_contains_task() {
        let prompt = build_extended_system_prompt("build a REST API", ExtendedTurnLimit::Enforced(20));
        assert!(prompt.contains("build a REST API"));
        assert!(prompt.contains("20"));
        assert!(prompt.contains("EXTENDED AUTONOMOUS MODE"));
        assert!(prompt.contains("do NOT ask the user questions"));
    }

    // ── /side parsing tests ──

    #[test]
    fn test_parse_side_question_basic() {
        let q = parse_side_question("/side what is a monad?");
        assert_eq!(q.unwrap(), "what is a monad?");
    }

    #[test]
    fn test_parse_side_question_empty() {
        assert!(parse_side_question("/side").is_none());
        assert!(parse_side_question("/side   ").is_none());
    }

    #[test]
    fn test_parse_side_question_preserves_whitespace_in_question() {
        let q = parse_side_question("/side   what   is   this  ");
        assert_eq!(q.unwrap(), "what   is   this");
    }

    #[test]
    fn test_parse_side_question_multiword() {
        let q = parse_side_question("/side how do I convert Vec<u8> to String in Rust?");
        assert_eq!(q.unwrap(), "how do I convert Vec<u8> to String in Rust?");
    }

    // ── last-side store tests ──
    // These serialize on a shared static, so we run them within one test to
    // avoid cross-test interference on the single `LAST_SIDE` slot.
    #[test]
    fn test_last_side_store_set_get_overwrite_empty() {
        // NOTE: mutates the process-global LAST_SIDE. Keep the whole store
        // lifecycle in one test so parallel test threads don't collide.
        let mut guard = lock_or_recover(&LAST_SIDE);
        *guard = None;
        drop(guard);

        // Empty store returns None.
        assert!(last_side().is_none());

        // Set → get returns it.
        store_last_side(
            "what is a monad?",
            "a monoid in the category of endofunctors",
        );
        assert_eq!(
            last_side(),
            Some((
                "what is a monad?".to_string(),
                "a monoid in the category of endofunctors".to_string(),
            ))
        );

        // Overwrite replaces the previous (one slot, latest wins).
        store_last_side("second q", "second a");
        assert_eq!(
            last_side(),
            Some(("second q".to_string(), "second a".to_string()))
        );

        // Non-consuming: a second read still returns the same value.
        assert_eq!(
            last_side(),
            Some(("second q".to_string(), "second a".to_string()))
        );

        // Reset so we don't leak state to other tests.
        let mut guard = lock_or_recover(&LAST_SIDE);
        *guard = None;
    }

    #[test]
    fn test_is_side_pull() {
        assert!(is_side_pull("/side pull"));
        assert!(is_side_pull("/side   pull  "));
        assert!(!is_side_pull("/side pull the answer")); // extra args → real question
        assert!(!is_side_pull("/side what is pull?"));
        assert!(!is_side_pull("/side"));
        assert!(!is_side_pull("/side "));
    }

    #[test]
    fn test_parse_quick_question_basic() {
        let q = parse_quick_question("/quick what does borrow of moved value mean?");
        assert_eq!(q.unwrap(), "what does borrow of moved value mean?");
    }

    #[test]
    fn test_parse_quick_question_empty() {
        assert!(parse_quick_question("/quick").is_none());
        assert!(parse_quick_question("/quick   ").is_none());
    }

    #[test]
    fn test_parse_quick_question_preserves_content() {
        let q = parse_quick_question("/quick   how do I use sed?  ");
        assert_eq!(q.unwrap(), "how do I use sed?");
    }

    #[test]
    fn test_parse_quick_question_multiword() {
        let q = parse_quick_question("/quick explain async vs threading in Rust");
        assert_eq!(q.unwrap(), "explain async vs threading in Rust");
    }
}
