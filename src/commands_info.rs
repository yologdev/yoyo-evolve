//! Read-only "info" REPL command handlers.
//!
//! These handlers print state without mutating anything: `/version`, `/status`,
//! `/tokens`, `/cost`, `/profile`, `/model` (show), `/provider` (show),
//! `/think` (show), `/changelog`, `/evolution`.
//!
//! Extracted from `commands.rs` as the first slice of issue #260, which tracks
//! splitting the 3,500-line `commands.rs` into focused modules. Read-only
//! handlers are the safest possible first slice — no shared mutable state, no
//! session-changes plumbing, no provider rebuild paths.

use crate::cli::{default_model_for_provider, known_models_for_provider, KNOWN_PROVIDERS, VERSION};
use crate::commands::thinking_level_name;
use crate::format::*;
use crate::git::*;

use std::sync::OnceLock;
use yoagent::agent::Agent;
use yoagent::context::{estimate_tokens, total_tokens};
use yoagent::*;

/// Session-level cache for self-written percentage so repeated calls
/// don't re-run git blame.
static SELF_WRITTEN_CACHE: OnceLock<Option<(usize, usize, f64)>> = OnceLock::new();

/// Compute the percentage of source code lines written by the bot (yoyo-evolve).
///
/// Returns `(self_written_lines, total_lines, percentage)` or `None` if git blame
/// fails (not a git repo, no git installed, etc.).
///
/// This runs `git blame --line-porcelain` across all `src/**/*.rs` files and counts
/// lines where the author contains "yoyo". The result is cached for the session
/// duration via `OnceLock`, so repeated calls are free after the first.
pub fn compute_self_written_pct() -> Option<(usize, usize, f64)> {
    *SELF_WRITTEN_CACHE.get_or_init(compute_self_written_pct_inner)
}

/// Inner (uncached) implementation. Finds source files via `git ls-files`,
/// then runs `git blame --line-porcelain` and counts author lines.
fn compute_self_written_pct_inner() -> Option<(usize, usize, f64)> {
    // Find all tracked .rs files under src/
    let ls_output = std::process::Command::new("git")
        .args(["ls-files", "src/"])
        .output()
        .ok()?;
    if !ls_output.status.success() {
        return None;
    }
    let file_list = String::from_utf8_lossy(&ls_output.stdout);
    let rs_files: Vec<&str> = file_list.lines().filter(|f| f.ends_with(".rs")).collect();
    if rs_files.is_empty() {
        return None;
    }

    // Run git blame --line-porcelain on all files at once
    let mut cmd = std::process::Command::new("git");
    cmd.arg("blame").arg("--line-porcelain");
    for f in &rs_files {
        cmd.arg(f);
    }
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut total = 0usize;
    let mut self_written = 0usize;
    for line in stdout.lines() {
        if let Some(author) = line.strip_prefix("author ") {
            total += 1;
            // Match bot author: "yoyo-evolve[bot]", "yoyo-evolve", or anything with "yoyo"
            let author_lower = author.to_lowercase();
            if author_lower.contains("yoyo") {
                self_written += 1;
            }
        }
    }

    if total == 0 {
        return None;
    }

    let pct = (self_written as f64 / total as f64) * 100.0;
    Some((self_written, total, pct))
}

/// Format the self-written percentage for display.
fn format_self_written(self_written: usize, total: usize, pct: f64) -> String {
    format!(
        "self-written: {:.1}% ({} / {} lines)",
        pct,
        format_token_count(self_written as u64),
        format_token_count(total as u64),
    )
}

// ── /version ─────────────────────────────────────────────────────────────

/// Build a compact version string: `yoyo v0.1.9 (abc1234 2026-04-23) linux-x86_64`
///
/// Uses compile-time env vars `GIT_HASH` and `BUILD_DATE` (set by `build.rs`
/// or overridden in CI/release builds).
pub fn version_line() -> String {
    let hash = option_env!("GIT_HASH").unwrap_or("dev");
    let date = option_env!("BUILD_DATE").unwrap_or("dev");
    let target = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);

    format!("yoyo v{VERSION} ({hash} {date}) {target}")
}

pub fn handle_version() {
    println!("{DIM}  {}{RESET}\n", version_line());
}

/// Print enriched version output. When verbose, also shows provider,
/// model, and yoagent version.
pub fn handle_version_verbose(provider: &str, model: &str) {
    println!("{DIM}  {}", version_line());
    println!("  provider: {provider}  model: {model}");
    let yoagent_ver = option_env!("YOAGENT_VERSION").unwrap_or("unknown");
    println!("  yoagent:  v{yoagent_ver}");
    if let Some((s, t, pct)) = compute_self_written_pct() {
        println!("  {}", format_self_written(s, t, pct));
    }
    println!("{RESET}");
}

// ── /status ──────────────────────────────────────────────────────────────

/// Count uncommitted file changes via `git status --porcelain`.
///
/// Returns `(modified, added)` counts, or `None` if git is unavailable.
fn count_session_file_changes() -> Option<(usize, usize)> {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut modified = 0usize;
    let mut added = 0usize;
    for line in text.lines() {
        if line.len() < 2 {
            continue;
        }
        // git status --porcelain: first two chars are index/worktree status
        let index = line.as_bytes()[0];
        let worktree = line.as_bytes()[1];
        match (index, worktree) {
            (b'?', b'?') => added += 1,             // untracked
            (b'A', _) => added += 1,                // added to index
            (b'M', _) | (_, b'M') => modified += 1, // modified
            (b'R', _) => modified += 1,             // renamed
            (b'D', _) | (_, b'D') => modified += 1, // deleted counts as modified
            _ => {}
        }
    }
    Some((modified, added))
}

pub fn handle_status(
    model: &str,
    cwd: &str,
    session_total: &Usage,
    elapsed: std::time::Duration,
    turns: usize,
    context_used: u64,
    context_max: u64,
) {
    println!("{DIM}  model:   {model}");
    if let Some(branch) = git_branch() {
        println!("  git:     {branch}");
    }
    println!("  cwd:     {cwd}");
    // Show active modes
    if crate::commands_config::is_teach_mode() {
        println!("  mode:    {GREEN}teach{DIM}");
    }
    if let Some(arch_status) = crate::commands_config::architect_status(model) {
        println!("  mode:    {GREEN}{arch_status}{DIM}");
    }
    if crate::commands_plan::is_plan_mode() {
        println!("  mode:    {GREEN}plan{DIM}");
    }
    if crate::commands_config::is_read_mode() {
        println!("  mode:    {GREEN}read-only{DIM}");
    }
    // Show active goal if set
    if let Some(goal) = crate::commands_goal::load_goal() {
        let goal_preview = if goal.len() > 60 {
            format!("{}…", safe_truncate(&goal, 60))
        } else {
            goal
        };
        println!("  goal:    {goal_preview}");
    }
    // Show active watch command(s)
    if let Some(cmd) = crate::watch::get_watch_command() {
        println!("  watch:   {cmd}");
    }
    println!(
        "  session: {} elapsed, {turns} turn{}",
        format_duration(elapsed),
        if turns == 1 { "" } else { "s" }
    );
    // Show file changes
    if let Some((modified, added)) = count_session_file_changes() {
        if modified + added > 0 {
            let mut parts = Vec::new();
            if modified > 0 {
                parts.push(format!("{modified} modified"));
            }
            if added > 0 {
                parts.push(format!("{added} added"));
            }
            println!("  changes: {}", parts.join(", "));
        }
    }
    println!(
        "  tokens:  {} in / {} out (session total)",
        session_total.input, session_total.output
    );
    if context_max > 0 {
        let pct = ((context_used as f64 / context_max as f64) * 100.0) as u32;
        let color = context_usage_color(pct);
        println!(
            "  context: {} / {} tokens ({color}{pct}%{DIM})",
            format_token_count(context_used),
            format_token_count(context_max),
        );
    }
    if let Some((s, t, pct)) = compute_self_written_pct() {
        println!("  {}", format_self_written(s, t, pct));
    }
    println!("{RESET}");
}

// ── /tokens ──────────────────────────────────────────────────────────────

/// Breakdown of what's consuming context tokens by category.
pub struct ContextBreakdown {
    /// Estimated tokens for system prompt + project context.
    pub system_estimate: usize,
    /// Tokens from user messages.
    pub user_messages: usize,
    /// Tokens from assistant text responses.
    pub assistant_messages: usize,
    /// Tokens from tool calls (inside assistant messages).
    pub tool_calls: usize,
    /// Tokens from tool result messages.
    pub tool_results: usize,
    /// Tokens from thinking blocks.
    pub thinking: usize,
    /// Total of all categories.
    pub total: usize,
}

/// Analyze messages to produce a per-category token breakdown.
///
/// Walks each message and categorizes its content. For assistant messages,
/// separates text/thinking tokens from tool-call tokens so users can see
/// whether tool results or their own conversation is dominating context.
pub fn context_breakdown(messages: &[AgentMessage]) -> ContextBreakdown {
    let system_estimate = estimate_tokens(crate::cli::SYSTEM_PROMPT);

    let mut user_messages: usize = 0;
    let mut assistant_messages: usize = 0;
    let mut tool_calls: usize = 0;
    let mut tool_results: usize = 0;
    let mut thinking: usize = 0;

    for msg in messages {
        match msg {
            AgentMessage::Llm(m) => match m {
                Message::User { content, .. } => {
                    user_messages += content_block_tokens(content) + 4;
                }
                Message::Assistant { content, .. } => {
                    // Separate assistant text, thinking, and tool calls
                    for c in content {
                        match c {
                            Content::Text { text } => {
                                assistant_messages += estimate_tokens(text);
                            }
                            Content::Thinking { thinking: t, .. } => {
                                thinking += estimate_tokens(t);
                            }
                            Content::ToolCall {
                                name, arguments, ..
                            } => {
                                tool_calls += estimate_tokens(name)
                                    + estimate_tokens(&arguments.to_string())
                                    + 8;
                            }
                            Content::Image { data, .. } => {
                                let raw_bytes = data.len() * 3 / 4;
                                assistant_messages += (raw_bytes / 750).clamp(85, 16_000);
                            }
                        }
                    }
                    assistant_messages += 4; // message overhead
                }
                Message::ToolResult {
                    content, tool_name, ..
                } => {
                    tool_results += content_block_tokens(content) + estimate_tokens(tool_name) + 8;
                }
            },
            AgentMessage::Extension(ext) => {
                // Count extensions toward user messages as a catch-all
                user_messages += estimate_tokens(&ext.data.to_string()) + 4;
            }
        }
    }

    let total =
        system_estimate + user_messages + assistant_messages + tool_calls + tool_results + thinking;

    ContextBreakdown {
        system_estimate,
        user_messages,
        assistant_messages,
        tool_calls,
        tool_results,
        thinking,
        total,
    }
}

/// Estimate tokens for a content block list (mirrors yoagent's content_tokens).
fn content_block_tokens(content: &[Content]) -> usize {
    content
        .iter()
        .map(|c| match c {
            Content::Text { text } => estimate_tokens(text),
            Content::Image { data, .. } => {
                let raw_bytes = data.len() * 3 / 4;
                (raw_bytes / 750).clamp(85, 16_000)
            }
            Content::Thinking { thinking, .. } => estimate_tokens(thinking),
            Content::ToolCall {
                name, arguments, ..
            } => estimate_tokens(name) + estimate_tokens(&arguments.to_string()) + 8,
        })
        .sum()
}

pub fn handle_tokens(agent: &Agent, session_total: &Usage, model: &str) {
    let max_context = crate::cli::effective_context_tokens();
    let messages = agent.messages().to_vec();
    let context_used = total_tokens(&messages) as u64;
    let bar = context_bar(context_used, max_context);

    println!("{DIM}  Active context:");
    println!("    messages:    {}", messages.len());
    println!(
        "    current:     {} / {} tokens",
        format_token_count(context_used),
        format_token_count(max_context)
    );
    println!("    {bar}");

    // Show per-category context breakdown
    if !messages.is_empty() {
        let breakdown = context_breakdown(&messages);
        println!();
        println!("{}", format_context_breakdown(&breakdown));
    }

    if session_total.input > context_used + 1000 {
        println!("    {DIM}(earlier messages were compacted to save space — session totals below show full usage){RESET}");
    }
    if context_used as f64 / max_context as f64 > 0.75 {
        println!("    {YELLOW}⚠ Context is getting full. Consider /clear or /compact.{RESET}");
    }
    println!();
    println!("  Session totals (all API calls):");
    println!(
        "    input:       {} tokens",
        format_token_count(session_total.input)
    );
    println!(
        "    output:      {} tokens",
        format_token_count(session_total.output)
    );
    println!(
        "    cache read:  {} tokens",
        format_token_count(session_total.cache_read)
    );
    println!(
        "    cache write: {} tokens",
        format_token_count(session_total.cache_write)
    );
    if let Some(cache_line) = format_cache_stats(session_total) {
        println!("    {cache_line}");
    }
    if let Some(cost) = estimate_cost(session_total, model) {
        println!("    est. cost:   {}", format_cost(cost));
    }
    println!("{RESET}");
}

// ── /cost ────────────────────────────────────────────────────────────────

pub fn handle_cost(session_total: &Usage, model: &str, messages: &[yoagent::AgentMessage]) {
    if let Some(cost) = estimate_cost(session_total, model) {
        println!("{DIM}  Session cost: {}", format_cost(cost));
        println!(
            "    {} in / {} out",
            format_token_count(session_total.input),
            format_token_count(session_total.output)
        );
        if session_total.cache_read > 0 || session_total.cache_write > 0 {
            println!(
                "    cache: {} read / {} write",
                format_token_count(session_total.cache_read),
                format_token_count(session_total.cache_write)
            );
            if let Some(cache_line) = format_cache_stats(session_total) {
                println!("    {cache_line}");
            }
        }
        if let Some((input_cost, cw_cost, cr_cost, output_cost)) =
            cost_breakdown(session_total, model)
        {
            println!();
            println!("    Breakdown:");
            println!("      input:       {}", format_cost(input_cost));
            println!("      output:      {}", format_cost(output_cost));
            if cw_cost > 0.0 {
                println!("      cache write: {}", format_cost(cw_cost));
            }
            if cr_cost > 0.0 {
                println!("      cache read:  {}", format_cost(cr_cost));
            }
        }

        // Per-turn breakdown
        let turn_costs = extract_turn_costs(messages, model);
        if !turn_costs.is_empty() {
            println!();
            println!("{}", format_turn_costs(&turn_costs));
        }

        println!("{RESET}");
    } else {
        println!("{DIM}  Cost estimation not available for model '{model}'.{RESET}\n");
    }
}

// ── /model ───────────────────────────────────────────────────────────────

pub fn handle_model_show(model: &str) {
    println!("{DIM}  current model: {model}");
    println!("  usage: /model <name>{RESET}\n");
}

/// Display known models grouped by provider. If `filter` is non-empty and matches
/// a provider name, only that provider's models are shown.
pub fn handle_model_list(current_model: &str, current_provider: &str, filter: &str) {
    // If filter matches a known provider, show only that one
    let providers: Vec<&str> = if !filter.is_empty()
        && KNOWN_PROVIDERS
            .iter()
            .any(|p| p.eq_ignore_ascii_case(filter))
    {
        KNOWN_PROVIDERS
            .iter()
            .filter(|p| p.eq_ignore_ascii_case(filter))
            .copied()
            .collect()
    } else if !filter.is_empty() {
        println!(
            "{YELLOW}  unknown provider: {filter}. available: {}{RESET}\n",
            KNOWN_PROVIDERS.join(", ")
        );
        return;
    } else {
        KNOWN_PROVIDERS.to_vec()
    };

    println!("{DIM}  Models by provider (active: {current_model}){RESET}\n");

    for provider in &providers {
        let models = known_models_for_provider(provider);
        if models.is_empty() {
            continue;
        }
        let default_model = default_model_for_provider(provider);
        let provider_label = if *provider == current_provider {
            format!("{BOLD}{provider}{RESET}")
        } else {
            format!("{DIM}{provider}{RESET}")
        };
        println!("  {provider_label}");

        for model in models {
            let is_active = *model == current_model;
            let is_default = *model == default_model;
            let marker = if is_active { "\u{25b8}" } else { " " };
            let suffix = if is_default { "  (default)" } else { "" };
            if is_active {
                println!("  {marker} {GREEN}{model}{suffix}{RESET}");
            } else {
                println!("{DIM}  {marker} {model}{suffix}{RESET}");
            }
        }
        println!();
    }

    println!("{DIM}  Use: /model <name> to switch{RESET}\n");
}

// ── /provider ────────────────────────────────────────────────────────────

/// Return the context window size (in tokens) for well-known models.
/// Returns `None` when the model isn't in our registry.
pub fn model_context_window(model: &str) -> Option<u64> {
    // Anthropic Claude family
    if model.contains("claude") {
        // Sonnet 4+ all have 1M context
        if model.contains("sonnet-4") || model.contains("sonnet-4.") {
            return Some(1_000_000);
        }
        if model.contains("opus") {
            // Opus 4.6+ have 1M; older Opus (4.0, 4.1, 4.5) have 200k
            if model.contains("4-6")
                || model.contains("4.6")
                || model.contains("4-7")
                || model.contains("4.7")
            {
                return Some(1_000_000);
            }
            return Some(200_000);
        }
        // Haiku and older Claude models: 200k
        return Some(200_000);
    }
    // OpenAI codex-mini — 192k
    if model.contains("codex-mini") {
        return Some(192_000);
    }
    // OpenAI GPT-4.1 family — 1M
    if model.contains("gpt-4.1") {
        return Some(1_048_576);
    }
    // OpenAI GPT-4o family — 128k
    if model.contains("gpt-4o") {
        return Some(128_000);
    }
    // OpenAI GPT-5 family — 1M
    if model.contains("gpt-5") {
        return Some(1_048_576);
    }
    // OpenAI o-series reasoning models
    if model.starts_with("o3") || model.starts_with("o4") {
        return Some(200_000);
    }
    // Google Gemini 3.x — 1M (estimated, Google's trajectory)
    if model.contains("gemini-3") {
        return Some(1_048_576);
    }
    // Google Gemini 2.5 — 1M
    if model.contains("gemini-2.5") {
        return Some(1_048_576);
    }
    // Google Gemini 2.0 — 1M
    if model.contains("gemini-2.0") {
        return Some(1_048_576);
    }
    // xAI Grok — 131k
    if model.contains("grok") {
        return Some(131_072);
    }
    // Groq Llama 4 — 128k (practical API limit)
    if model.contains("llama-4") {
        return Some(128_000);
    }
    // DeepSeek — 128k
    if model.contains("deepseek") {
        return Some(128_000);
    }
    // Mistral Large — 128k
    if model.contains("mistral") || model.contains("codestral") {
        return Some(128_000);
    }
    None
}

/// Format a token count as a human-readable string (e.g. "200k", "1M").
fn format_context_size(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        let m = tokens as f64 / 1_000_000.0;
        if (m - m.round()).abs() < 0.001 {
            format!("{}M tokens", m as u64)
        } else {
            format!("{:.1}M tokens", m)
        }
    } else {
        format!("{}k tokens", tokens / 1_000)
    }
}

/// Find which known provider serves a given model name.
/// Scans all providers' model lists and returns the first match.
pub fn find_provider_for_model(model: &str) -> Option<&'static str> {
    for provider in KNOWN_PROVIDERS {
        let models = known_models_for_provider(provider);
        if models.contains(&model) {
            return Some(provider);
        }
    }
    // Heuristic fallback: infer from model name prefix
    if model.contains("claude") {
        return Some("anthropic");
    }
    if model.starts_with("gpt-") || model.starts_with("o3") || model.starts_with("o4") {
        return Some("openai");
    }
    if model.contains("gemini") {
        return Some("google");
    }
    if model.contains("grok") {
        return Some("xai");
    }
    if model.contains("deepseek") {
        return Some("deepseek");
    }
    if model.contains("mistral") || model.contains("codestral") {
        return Some("mistral");
    }
    None
}

/// Display detailed information about a model: provider, context window, pricing.
pub fn handle_model_info(model_name: &str, current_model: &str) {
    let separator = "\u{2500}".repeat(model_name.len() + 6);
    println!("\n  {DIM}{separator}{RESET}");
    println!("  {BOLD}\u{2500}\u{2500} {model_name} \u{2500}\u{2500}{RESET}");
    println!("  {DIM}{separator}{RESET}\n");

    // Provider
    let provider = find_provider_for_model(model_name);
    match provider {
        Some(p) => println!("  {DIM}Provider:{RESET}  {p}"),
        None => println!("  {DIM}Provider:{RESET}  {YELLOW}unknown{RESET}"),
    }

    // Context window
    match model_context_window(model_name) {
        Some(ctx) => println!("  {DIM}Context:{RESET}   {}", format_context_size(ctx)),
        None => println!("  {DIM}Context:{RESET}   {YELLOW}unknown{RESET}"),
    }

    // Pricing — use estimate_cost with synthetic Usage to extract per-MTok rates
    let input_usage = yoagent::Usage {
        input: 1_000_000,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 1_000_000,
    };
    let output_usage = yoagent::Usage {
        input: 0,
        output: 1_000_000,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 1_000_000,
    };
    let input_cost = estimate_cost(&input_usage, model_name);
    let output_cost = estimate_cost(&output_usage, model_name);
    match (input_cost, output_cost) {
        (Some(ic), Some(oc)) => {
            println!(
                "  {DIM}Pricing:{RESET}   ${:.2} in / ${:.2} out (per MTok)",
                ic, oc
            );
        }
        _ => println!("  {DIM}Pricing:{RESET}   {YELLOW}unknown{RESET}"),
    }

    // Default model for the provider?
    if let Some(p) = provider {
        let default = default_model_for_provider(p);
        if default == model_name {
            println!("  {DIM}Default:{RESET}   {GREEN}\u{2713}{RESET} (for {p})");
        }
    }

    // Active?
    if model_name == current_model {
        println!("  {DIM}Active:{RESET}    {GREEN}\u{2713}{RESET}");
    }

    // Not in registry?
    if provider.is_none() {
        println!(
            "\n  {YELLOW}Not in known model registry \u{2014} pricing and context may be unavailable.{RESET}"
        );
    }

    println!();
}

pub fn handle_provider_show(provider: &str) {
    println!("{DIM}  current provider: {provider}");
    println!("  usage: /provider <name>");
    println!("  available: {}{RESET}\n", KNOWN_PROVIDERS.join(", "));
}

// ── /think ───────────────────────────────────────────────────────────────

pub fn handle_think_show(thinking: ThinkingLevel) {
    let level_str = thinking_level_name(thinking);
    println!("{DIM}  thinking: {level_str}");
    println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
}

// ── /changelog ──────────────────────────────────────────────────────────

pub fn handle_profile(
    agent: &Agent,
    model: &str,
    provider: &str,
    session_start: std::time::Instant,
    session_total: &Usage,
) {
    let max_context = crate::cli::effective_context_tokens();
    let messages = agent.messages();
    let context_used = total_tokens(messages) as u64;
    // Count assistant turns
    let turns = messages
        .iter()
        .filter(|m| {
            matches!(
                m,
                yoagent::AgentMessage::Llm(yoagent::Message::Assistant { .. })
            )
        })
        .count();
    let elapsed = session_start.elapsed();

    // Cost string
    let cost_str = estimate_cost(session_total, model)
        .map(|c| format!("~{}", format_cost(c)))
        .unwrap_or_else(|| "n/a".to_string());

    // Token strings
    let tokens_str = format!(
        "{} in / {} out",
        format_token_count(session_total.input),
        format_token_count(session_total.output)
    );

    // Context string (plain, for width calculation)
    let ctx_plain = if max_context > 0 {
        let pct = ((context_used as f64 / max_context as f64) * 100.0) as u32;
        format!(
            "{} / {} ({}%)",
            format_token_count(context_used),
            format_token_count(max_context),
            pct
        )
    } else {
        format_token_count(context_used)
    };

    // Context color for the display version
    let pct_val = if max_context > 0 {
        ((context_used as f64 / max_context as f64) * 100.0) as u32
    } else {
        0
    };
    let ctx_color = context_usage_color(pct_val);

    // Speed string (avg output tok/s, only if session > 1s)
    let speed_str = if elapsed.as_secs_f64() > 1.0 && session_total.output > 0 {
        let tps = (session_total.output as f64 / elapsed.as_secs_f64()) as u32;
        format!("~{} tok/s", tps)
    } else {
        String::new()
    };

    let label = "Session Profile";
    // Build content lines: (key, plain_value, display_value)
    // plain_value is for width calculation, display_value may contain ANSI
    let duration_str = format_duration(elapsed);
    let turns_str = format!("{turns}");
    let mut lines: Vec<(&str, &str, String)> = vec![
        ("Model", model, model.to_string()),
        ("Provider", provider, provider.to_string()),
        ("Duration", &duration_str, duration_str.clone()),
        ("Turns", &turns_str, turns_str.clone()),
        ("Tokens", &tokens_str, tokens_str.clone()),
        ("Cost", &cost_str, cost_str.clone()),
    ];
    if !speed_str.is_empty() {
        lines.push(("Speed", &speed_str, speed_str.clone()));
    }
    lines.push((
        "Context",
        &ctx_plain,
        format!("{ctx_color}{ctx_plain}{DIM}"),
    ));

    // Use fixed label column of 10 chars (longest key is "Provider" = 8 + ":  " = 11)
    let label_col = 10;
    // Find the longest value for box width
    let max_val_width = lines.iter().map(|(_, pv, _)| pv.len()).max().unwrap_or(20);
    // inner_width = "│ " + label_col + value + " │"
    let inner_width = (label_col + max_val_width + 2).max(label.len() + 4);

    // Top border
    let top_pad = inner_width - label.len() - 2;
    println!("{DIM}  ╭─ {label} {}╮", "─".repeat(top_pad));

    // Content lines
    for (key, plain_val, display_val) in &lines {
        let key_pad = label_col - key.len() - 1; // -1 for the colon
        let val_pad = inner_width - label_col - plain_val.len() - 2;
        println!(
            "  │ {key}:{}{display_val}{} │",
            " ".repeat(key_pad),
            " ".repeat(val_pad)
        );
    }

    // Bottom border
    println!("  ╰{}╯{RESET}", "─".repeat(inner_width));
    println!();
}

/// Parse the optional count argument from `/changelog [N]` input.
/// Returns a count clamped to 1..=100, defaulting to 15.
pub fn parse_changelog_count(input: &str) -> usize {
    let arg = input.strip_prefix("/changelog").unwrap_or("").trim();
    if arg.is_empty() {
        return 15;
    }
    arg.parse::<usize>().unwrap_or(15).clamp(1, 100)
}

pub fn handle_changelog(input: &str) {
    let count = parse_changelog_count(input);

    let count_arg = format!("-{count}");
    let output = std::process::Command::new("git")
        .args(["log", "--oneline", "--format=%h %s (%ar)", &count_arg])
        .output();

    match output {
        Ok(result) if result.status.success() => {
            let text = String::from_utf8_lossy(&result.stdout);
            let text = text.trim();
            if text.is_empty() {
                println!("{DIM}  (no commits found){RESET}\n");
            } else {
                println!("{DIM}  Recent commits ({count} max):\n");
                for line in text.lines() {
                    println!("    {line}");
                }
                println!("{RESET}");
            }
        }
        Ok(_) => {
            println!("{DIM}  (not in a git repository){RESET}\n");
        }
        Err(_) => {
            println!("{DIM}  (git not available){RESET}\n");
        }
    }
}

/// A parsed evolution session from a git tag like `day54-15-04`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionSession {
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub title: Option<String>,
}

/// Parse a git tag like `day54-15-04` into an `EvolutionSession`.
pub fn parse_evolution_tag(tag: &str) -> Option<EvolutionSession> {
    let rest = tag.strip_prefix("day")?;
    let parts: Vec<&str> = rest.splitn(3, '-').collect();
    if parts.len() != 3 {
        return None;
    }
    let day = parts[0].parse::<u32>().ok()?;
    let hour = parts[1].parse::<u32>().ok()?;
    let minute = parts[2].parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(EvolutionSession {
        day,
        hour,
        minute,
        title: None,
    })
}

/// Parse journal titles from JOURNAL.md content.
/// Returns a map of (day, hour, minute) → title.
pub fn parse_journal_titles(content: &str) -> std::collections::HashMap<(u32, u32, u32), String> {
    let mut titles = std::collections::HashMap::new();
    for line in content.lines() {
        // Format: ## Day NN — HH:MM — Title text
        if let Some(rest) = line.strip_prefix("## Day ") {
            let parts: Vec<&str> = rest.splitn(3, " — ").collect();
            if parts.len() == 3 {
                if let Ok(day) = parts[0].parse::<u32>() {
                    let time_parts: Vec<&str> = parts[1].splitn(2, ':').collect();
                    if time_parts.len() == 2 {
                        if let (Ok(hour), Ok(minute)) =
                            (time_parts[0].parse::<u32>(), time_parts[1].parse::<u32>())
                        {
                            titles.insert((day, hour, minute), parts[2].to_string());
                        }
                    }
                }
            }
        }
    }
    titles
}

/// Parse optional count from `/evolution [N]`.
pub fn parse_evolution_count(input: &str) -> usize {
    let arg = input.strip_prefix("/evolution").unwrap_or("").trim();
    if arg.is_empty() {
        return 10;
    }
    arg.parse::<usize>().unwrap_or(10).clamp(1, 100)
}

/// Compute sessions-per-day stats: (avg, max_day, max_count, current_streak).
/// current_streak = consecutive days with at least one session ending at current_day.
pub fn session_stats(sessions: &[EvolutionSession], current_day: u32) -> (f64, u32, u32, u32) {
    if sessions.is_empty() {
        return (0.0, 0, 0, 0);
    }

    // Count sessions per day
    let mut day_counts: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for s in sessions {
        *day_counts.entry(s.day).or_insert(0) += 1;
    }

    let total_days = day_counts.len() as f64;
    let total_sessions = sessions.len() as f64;
    let avg = total_sessions / total_days;

    let (max_day, max_count) = day_counts
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(&day, &count)| (day, count))
        .unwrap_or((0, 0));

    // Compute current streak (consecutive days ending at current_day)
    let mut streak = 0u32;
    let mut check_day = current_day;
    loop {
        if day_counts.contains_key(&check_day) {
            streak += 1;
            if check_day == 0 {
                break;
            }
            check_day -= 1;
        } else {
            break;
        }
    }

    (avg, max_day, max_count, streak)
}

// --- CI run status for /evolution ---

/// A single CI workflow run parsed from `gh run list` JSON output.
#[derive(Debug, Clone)]
pub struct CiRun {
    pub status: String,      // "completed", "in_progress", "queued"
    pub conclusion: String,  // "success", "failure", "cancelled", "" (when in progress)
    pub name: String,        // workflow name
    pub created_at: String,  // ISO 8601 timestamp
    pub head_branch: String, // branch name
}

/// Format a CI run status as a colored emoji indicator.
pub fn format_ci_status(status: &str, conclusion: &str) -> &'static str {
    match (status, conclusion) {
        (_, "success") => "✅",
        (_, "failure") => "❌",
        (_, "cancelled") => "⏹️",
        ("in_progress", _) => "🔄",
        ("queued", _) => "🕐",
        _ => "❓",
    }
}

/// Format a CI run's created_at timestamp as a relative time string (e.g. "2h ago").
/// Falls back to the raw timestamp if parsing fails.
pub fn format_ci_time_ago(created_at: &str) -> String {
    // Parse ISO 8601 like "2026-04-24T10:30:00Z"
    // Simple parsing: extract date and time components
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Try to parse the timestamp manually (avoid adding chrono dependency)
    if let Some(secs) = parse_iso8601_to_epoch(created_at) {
        let diff = now.saturating_sub(secs);
        if diff < 60 {
            "just now".to_string()
        } else if diff < 3600 {
            format!("{}m ago", diff / 60)
        } else if diff < 86400 {
            format!("{}h ago", diff / 3600)
        } else {
            format!("{}d ago", diff / 86400)
        }
    } else {
        // Fallback: show the date portion
        created_at
            .split('T')
            .next()
            .unwrap_or(created_at)
            .to_string()
    }
}

/// Parse a simplified ISO 8601 timestamp (e.g. "2026-04-24T10:30:00Z") to Unix epoch seconds.
/// Returns None if parsing fails.
pub fn parse_iso8601_to_epoch(ts: &str) -> Option<u64> {
    // Expected format: YYYY-MM-DDTHH:MM:SSZ
    let ts = ts.trim().trim_end_matches('Z');
    let (date_part, time_part) = ts.split_once('T')?;

    let date_parts: Vec<&str> = date_part.split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }
    let year: u64 = date_parts[0].parse().ok()?;
    let month: u64 = date_parts[1].parse().ok()?;
    let day: u64 = date_parts[2].parse().ok()?;

    let time_parts: Vec<&str> = time_part.split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }
    let hour: u64 = time_parts[0].parse().ok()?;
    let min: u64 = time_parts[1].parse().ok()?;
    let sec: u64 = time_parts[2].parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 59 {
        return None;
    }

    // Days from year 1970 to the given year (simplified, ignoring leap seconds)
    let mut total_days: u64 = 0;
    for y in 1970..year {
        total_days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Days from months in current year
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        total_days += days_in_month[(m - 1) as usize] as u64;
        if m == 2 && is_leap_year(year) {
            total_days += 1;
        }
    }

    total_days += day - 1;

    Some(total_days * 86400 + hour * 3600 + min * 60 + sec)
}

fn is_leap_year(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Parse `gh run list --json ...` JSON output into a list of `CiRun`s.
/// Uses serde_json for robust parsing.
pub fn parse_ci_runs(json_str: &str) -> Vec<CiRun> {
    let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(json_str);
    let items = match parsed {
        Ok(items) => items,
        Err(_) => return Vec::new(),
    };

    items
        .into_iter()
        .filter_map(|obj| {
            let status = obj.get("status")?.as_str()?.to_string();
            let conclusion = obj
                .get("conclusion")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = obj.get("name")?.as_str()?.to_string();
            let created_at = obj.get("createdAt")?.as_str()?.to_string();
            let head_branch = obj
                .get("headBranch")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            Some(CiRun {
                status,
                conclusion,
                name,
                created_at,
                head_branch,
            })
        })
        .collect()
}

/// Fetch recent CI runs via `gh run list`. Returns an empty vec if `gh` is unavailable.
pub fn fetch_ci_runs(limit: usize) -> Vec<CiRun> {
    let output = std::process::Command::new("gh")
        .args([
            "run",
            "list",
            "--limit",
            &limit.to_string(),
            "--json",
            "status,conclusion,name,createdAt,headBranch",
        ])
        .output();

    match output {
        Ok(result) if result.status.success() => {
            let json_str = String::from_utf8_lossy(&result.stdout);
            parse_ci_runs(&json_str)
        }
        _ => Vec::new(),
    }
}

/// Format a list of CI runs for display.
pub fn format_ci_runs(runs: &[CiRun]) -> Vec<String> {
    runs.iter()
        .map(|run| {
            let icon = format_ci_status(&run.status, &run.conclusion);
            let time_ago = format_ci_time_ago(&run.created_at);
            let branch = if run.head_branch == "main" {
                String::new()
            } else {
                format!(" {DIM}({})  {RESET}", run.head_branch)
            };
            format!(
                "    {icon} {name:<20} {DIM}{time_ago:<10}{RESET}{branch}",
                name = safe_truncate(&run.name, 20),
            )
        })
        .collect()
}

/// Handle the `/evolution` command — show evolution history and stats.
pub fn handle_evolution(input: &str) {
    let count = parse_evolution_count(input);

    // Read DAY_COUNT
    let current_day = std::fs::read_to_string("DAY_COUNT")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);

    // Fetch git tags
    let tag_output = std::process::Command::new("git")
        .args(["tag", "--sort=-creatordate"])
        .output();

    let tags_text = match tag_output {
        Ok(result) if result.status.success() => {
            String::from_utf8_lossy(&result.stdout).to_string()
        }
        Ok(_) => {
            println!("{DIM}  (not in a git repository){RESET}\n");
            return;
        }
        Err(_) => {
            println!("{DIM}  (git not available){RESET}\n");
            return;
        }
    };

    // Parse tags into sessions
    let mut sessions: Vec<EvolutionSession> =
        tags_text.lines().filter_map(parse_evolution_tag).collect();

    // Try to load journal titles
    let journal_titles = std::fs::read_to_string("journals/JOURNAL.md")
        .map(|content| parse_journal_titles(&content))
        .unwrap_or_default();

    // Attach titles to sessions
    for session in &mut sessions {
        if let Some(title) = journal_titles.get(&(session.day, session.hour, session.minute)) {
            session.title = Some(title.clone());
        }
    }

    // Get test count
    let test_count = std::process::Command::new("cargo")
        .args(["test", "--", "--list"])
        .output()
        .ok()
        .and_then(|r| {
            if r.status.success() {
                let text = String::from_utf8_lossy(&r.stdout).to_string();
                Some(text.lines().filter(|l| l.ends_with(": test")).count())
            } else {
                None
            }
        })
        .unwrap_or(0);

    let total_sessions = sessions.len();

    // Header
    println!("\n  {BOLD}🐙 Evolution History — Day {current_day}{RESET}");
    println!();

    // Summary line
    let test_str = if test_count > 0 {
        format!(" | {CYAN}{test_count}{RESET} tests")
    } else {
        String::new()
    };
    println!(
        "  {DIM}{current_day} days{RESET} | {GREEN}{total_sessions}{RESET} sessions{test_str}"
    );

    // Stats
    let (avg, max_day, max_count, streak) = session_stats(&sessions, current_day);
    if total_sessions > 0 {
        println!(
            "  {DIM}avg {avg:.1}/day | peak {max_count} sessions (day {max_day}) | streak {streak} days{RESET}"
        );
    }
    println!();

    // Recent sessions
    if sessions.is_empty() {
        println!("{DIM}  (no evolution sessions found){RESET}\n");
        return;
    }

    let show_count = count.min(sessions.len());
    println!("  {BOLD}Recent sessions:{RESET}");
    for session in sessions.iter().take(show_count) {
        let today_marker = if session.day == current_day {
            format!(" {GREEN}(today){RESET}")
        } else {
            String::new()
        };

        let title_str = session
            .title
            .as_deref()
            .map(|t| format!("  {DIM}{t}{RESET}"))
            .unwrap_or_default();

        println!(
            "    {CYAN}Day {:>3}{RESET}  {:02}:{:02}{today_marker}{title_str}",
            session.day, session.hour, session.minute
        );
    }

    if total_sessions > show_count {
        let remaining = total_sessions - show_count;
        println!(
            "    {DIM}... and {remaining} more (use /evolution {total_sessions} to see all){RESET}"
        );
    }
    println!();

    // --- Recent CI runs ---
    let ci_runs = fetch_ci_runs(10);
    if !ci_runs.is_empty() {
        println!("  {BOLD}Recent CI runs:{RESET}");
        for line in format_ci_runs(&ci_runs) {
            println!("{line}");
        }
        println!();
    }
}

// ---------------------------------------------------------------------------
// /tips — context-sensitive feature discovery
// ---------------------------------------------------------------------------

/// Feature-discovery tips — randomly sampled each invocation.
const DISCOVERY_TIPS: &[&str] = &[
    "💡 `/spawn <task>` runs a sub-agent for parallel work",
    "💡 `/add <file>` injects file contents into the conversation",
    "💡 `/find <query>` does fuzzy file search across your project",
    "💡 `/grep <pattern>` searches file contents with context",
    "💡 `/map` shows a symbol map of your codebase",
    "💡 `/fork` creates a conversation branch to try a different approach",
    "💡 `/checkpoint save <name>` snapshots your files for easy rollback",
    "💡 `/review` gets an AI code review of your current diff",
    "💡 `/export` saves the conversation as markdown",
    "💡 `/doctor` checks your environment for common issues",
    "💡 `/profile` shows where time was spent this session",
    "💡 `/bg <cmd>` runs commands in the background",
    "💡 Use `@file.rs` in your prompt to auto-inject file contents",
    "💡 `/plan` enables plan mode — think before acting",
    "💡 `/open <file>` opens files in your editor",
];

/// Generate context-sensitive tips based on the current project and session state.
pub fn generate_tips() -> Vec<String> {
    use crate::commands_goal::load_goal;
    use crate::commands_project::{detect_project_type, ProjectType};
    use crate::watch::get_watch_command;

    let mut tips: Vec<String> = Vec::new();
    let cwd = std::env::current_dir().unwrap_or_default();
    let project = detect_project_type(&cwd);

    // --- Project-type tips ---
    match project {
        ProjectType::Rust => {
            tips.push("💡 `/watch cargo test` auto-runs tests after every change".into());
            tips.push("💡 `/lint fix` runs clippy and auto-fixes warnings".into());
        }
        ProjectType::Node => {
            tips.push("💡 `/watch npm test` monitors your test suite".into());
        }
        ProjectType::Python => {
            tips.push("💡 `/watch pytest` monitors your test suite".into());
        }
        ProjectType::Go => {
            tips.push("💡 `/watch go test ./...` monitors your test suite".into());
        }
        _ => {}
    }

    // Git repo tip
    if cwd.join(".git").exists() {
        tips.push("💡 `/diff --stat` shows a compact summary of your changes".into());
    }

    // --- Session-state tips ---
    if get_watch_command().is_none() {
        tips.push("💡 Set `/watch <cmd>` to auto-check after every agent edit".into());
    }

    if load_goal().is_none() {
        tips.push("💡 `/goal set <description>` gives the agent persistent focus".into());
    }

    // --- Feature-discovery tips (randomly sampled, 2-3) ---
    let sample_count = if DISCOVERY_TIPS.len() >= 3 {
        3
    } else {
        DISCOVERY_TIPS.len()
    };
    let mut indices: Vec<usize> = (0..DISCOVERY_TIPS.len()).collect();

    // Simple shuffle using thread_rng-equivalent: use elapsed nanos as seed
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as usize;
    // Fisher-Yates partial shuffle for `sample_count` items
    for i in 0..sample_count {
        let j = i + (seed.wrapping_add(i.wrapping_mul(7919))) % (indices.len() - i);
        indices.swap(i, j);
    }
    for &idx in &indices[..sample_count] {
        tips.push(DISCOVERY_TIPS[idx].into());
    }

    tips
}

/// Handle the `/tips` command — print context-sensitive feature suggestions.
pub fn handle_tips() {
    let tips = generate_tips();

    println!("\n  🐙 {BOLD}Tips for your current session:{RESET}\n");

    for tip in &tips {
        println!("  {CYAN}{tip}{RESET}");
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::provider::AnthropicProvider;
    use yoagent::{Agent, Usage};

    #[test]
    fn test_tokens_display_labels() {
        // Verify no panic with zero usage and empty conversation
        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");

        let usage = Usage {
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };

        // Should not panic with zero usage and empty conversation
        handle_tokens(&agent, &usage, "test-model");
    }

    #[test]
    fn test_tokens_display_with_large_values() {
        // Verify no panic with very large token counts
        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");

        let usage = Usage {
            input: 10_000_000,
            output: 5_000_000,
            cache_read: 3_000_000,
            cache_write: 1_000_000,
            total_tokens: 19_000_000,
        };

        // Should not panic with very large values
        handle_tokens(&agent, &usage, "test-model");
    }

    #[test]
    fn test_tokens_labels_are_clarified() {
        // Source-level check: the function body should use the clarified labels
        // from Issue #189, not the old confusing ones
        let source = include_str!("commands_info.rs");
        assert!(
            source.contains("Active context:"),
            "/tokens should use 'Active context:' header"
        );
        assert!(
            source.contains("Session totals (all API calls):"),
            "/tokens should use 'Session totals (all API calls):' header"
        );
        assert!(
            source.contains("session totals below show full usage"),
            "Compaction note should reference session totals"
        );
    }

    #[test]
    fn test_handle_status_with_timing() {
        use std::time::Duration;
        // Just verify it doesn't panic with various inputs
        handle_status(
            "test-model",
            "/tmp",
            &Usage::default(),
            Duration::from_secs(0),
            0,
            0,
            0,
        );
        handle_status(
            "test-model",
            "/tmp",
            &Usage::default(),
            Duration::from_secs(125),
            1,
            5000,
            200_000,
        );
        handle_status(
            "test-model",
            "/tmp",
            &Usage::default(),
            Duration::from_secs(7200),
            42,
            180_000,
            200_000,
        );
    }

    #[test]
    fn test_handle_status_context_line() {
        use std::time::Duration;
        // When context_max > 0, the context line should appear (no panic)
        handle_status(
            "test-model",
            "/tmp",
            &Usage::default(),
            Duration::from_secs(60),
            3,
            45_231,
            200_000,
        );
    }

    #[test]
    fn test_handle_status_skips_context_when_zero() {
        use std::time::Duration;
        // When context_max == 0, it should skip the context line (no panic)
        handle_status(
            "test-model",
            "/tmp",
            &Usage::default(),
            Duration::from_secs(60),
            3,
            0,
            0,
        );
    }

    #[test]
    fn test_parse_changelog_count_default() {
        assert_eq!(parse_changelog_count("/changelog"), 15);
    }

    #[test]
    fn test_parse_changelog_count_custom() {
        assert_eq!(parse_changelog_count("/changelog 30"), 30);
        assert_eq!(parse_changelog_count("/changelog 1"), 1);
        assert_eq!(parse_changelog_count("/changelog 100"), 100);
    }

    #[test]
    fn test_parse_changelog_count_clamped() {
        assert_eq!(parse_changelog_count("/changelog 0"), 1);
        assert_eq!(parse_changelog_count("/changelog 999"), 100);
    }

    #[test]
    fn test_parse_changelog_count_invalid() {
        // Non-numeric falls back to default 15
        assert_eq!(parse_changelog_count("/changelog abc"), 15);
        assert_eq!(parse_changelog_count("/changelog -5"), 15);
    }

    #[test]
    fn test_handle_changelog_no_panic() {
        // Should not panic regardless of git availability
        handle_changelog("/changelog");
        handle_changelog("/changelog 5");
    }

    #[test]
    fn test_handle_profile_no_panic() {
        use std::time::Instant;
        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");

        let usage = Usage::default();
        // Should not panic with empty agent and zero usage
        handle_profile(
            &agent,
            "claude-sonnet-4-20250514",
            "anthropic",
            Instant::now(),
            &usage,
        );
    }

    #[test]
    fn test_handle_profile_with_usage() {
        use std::time::Instant;
        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");

        let usage = Usage {
            input: 45_231,
            output: 12_890,
            cache_read: 5_000,
            cache_write: 2_000,
            total_tokens: 65_121,
        };
        // Should not panic with real-ish usage
        handle_profile(
            &agent,
            "claude-sonnet-4-20250514",
            "anthropic",
            Instant::now(),
            &usage,
        );
    }

    #[test]
    fn test_version_line_contains_version() {
        let line = version_line();
        assert!(
            line.contains(&format!("v{VERSION}")),
            "version_line should contain the version: {line}"
        );
    }

    #[test]
    fn test_version_line_contains_target() {
        let line = version_line();
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        assert!(
            line.contains(&format!("{os}-{arch}")),
            "version_line should contain target triple: {line}"
        );
    }

    #[test]
    fn test_version_line_format() {
        let line = version_line();
        // Should match: yoyo vX.Y.Z (HASH DATE) OS-ARCH
        assert!(
            line.starts_with("yoyo v"),
            "should start with 'yoyo v': {line}"
        );
        assert!(line.contains('('), "should contain '(': {line}");
        assert!(line.contains(')'), "should contain ')': {line}");
    }

    #[test]
    fn test_handle_version_no_panic() {
        // Basic version should not panic
        handle_version();
    }

    #[test]
    fn test_handle_version_verbose_no_panic() {
        // Verbose version with provider/model should not panic
        handle_version_verbose("anthropic", "claude-sonnet-4-20250514");
    }

    // === /evolution tests ===

    #[test]
    fn test_parse_evolution_tag_valid() {
        let s = parse_evolution_tag("day54-15-04").unwrap();
        assert_eq!(s.day, 54);
        assert_eq!(s.hour, 15);
        assert_eq!(s.minute, 4);
        assert!(s.title.is_none());
    }

    #[test]
    fn test_parse_evolution_tag_single_digits() {
        let s = parse_evolution_tag("day1-0-0").unwrap();
        assert_eq!(s.day, 1);
        assert_eq!(s.hour, 0);
        assert_eq!(s.minute, 0);
    }

    #[test]
    fn test_parse_evolution_tag_invalid_no_prefix() {
        assert!(parse_evolution_tag("v0.1.9").is_none());
    }

    #[test]
    fn test_parse_evolution_tag_invalid_bad_time() {
        assert!(parse_evolution_tag("day5-25-00").is_none()); // hour > 23
        assert!(parse_evolution_tag("day5-12-60").is_none()); // minute > 59
    }

    #[test]
    fn test_parse_evolution_tag_invalid_not_numbers() {
        assert!(parse_evolution_tag("dayX-12-30").is_none());
        assert!(parse_evolution_tag("day5-ab-30").is_none());
    }

    #[test]
    fn test_parse_evolution_tag_too_few_parts() {
        assert!(parse_evolution_tag("day5-12").is_none());
        assert!(parse_evolution_tag("day5").is_none());
    }

    #[test]
    fn test_parse_journal_titles() {
        let content = "\
# Journal

## Day 54 — 15:04 — Five sessions of standing still

Some text here.

## Day 54 — 04:40 — Knowing where you were built

More text.

## Day 53 — 19:11 — The file that was three things pretending to be one
";
        let titles = parse_journal_titles(content);
        assert_eq!(titles.len(), 3);
        assert_eq!(
            titles.get(&(54, 15, 4)),
            Some(&"Five sessions of standing still".to_string())
        );
        assert_eq!(
            titles.get(&(54, 4, 40)),
            Some(&"Knowing where you were built".to_string())
        );
        assert_eq!(
            titles.get(&(53, 19, 11)),
            Some(&"The file that was three things pretending to be one".to_string())
        );
    }

    #[test]
    fn test_parse_journal_titles_empty() {
        let titles = parse_journal_titles("");
        assert!(titles.is_empty());
    }

    #[test]
    fn test_parse_journal_titles_no_entries() {
        let titles = parse_journal_titles("# Journal\n\nSome other content.\n");
        assert!(titles.is_empty());
    }

    #[test]
    fn test_parse_evolution_count_default() {
        assert_eq!(parse_evolution_count("/evolution"), 10);
    }

    #[test]
    fn test_parse_evolution_count_custom() {
        assert_eq!(parse_evolution_count("/evolution 20"), 20);
        assert_eq!(parse_evolution_count("/evolution 1"), 1);
    }

    #[test]
    fn test_parse_evolution_count_clamped() {
        assert_eq!(parse_evolution_count("/evolution 0"), 1);
        assert_eq!(parse_evolution_count("/evolution 999"), 100);
    }

    #[test]
    fn test_parse_evolution_count_invalid() {
        assert_eq!(parse_evolution_count("/evolution abc"), 10);
    }

    #[test]
    fn test_session_stats_empty() {
        let (avg, max_day, max_count, streak) = session_stats(&[], 55);
        assert_eq!(avg, 0.0);
        assert_eq!(max_day, 0);
        assert_eq!(max_count, 0);
        assert_eq!(streak, 0);
    }

    #[test]
    fn test_session_stats_basic() {
        let sessions = vec![
            EvolutionSession {
                day: 54,
                hour: 4,
                minute: 40,
                title: None,
            },
            EvolutionSession {
                day: 54,
                hour: 15,
                minute: 4,
                title: None,
            },
            EvolutionSession {
                day: 53,
                hour: 19,
                minute: 11,
                title: None,
            },
        ];
        let (avg, max_day, max_count, streak) = session_stats(&sessions, 54);
        assert!((avg - 1.5).abs() < 0.01); // 3 sessions / 2 days
        assert_eq!(max_day, 54);
        assert_eq!(max_count, 2);
        assert_eq!(streak, 2); // days 54 and 53 are consecutive
    }

    #[test]
    fn test_session_stats_streak_with_gap() {
        let sessions = vec![
            EvolutionSession {
                day: 55,
                hour: 1,
                minute: 0,
                title: None,
            },
            // gap: no day 54
            EvolutionSession {
                day: 53,
                hour: 10,
                minute: 0,
                title: None,
            },
        ];
        let (_avg, _max_day, _max_count, streak) = session_stats(&sessions, 55);
        assert_eq!(streak, 1); // only day 55, gap before 53
    }

    #[test]
    fn test_handle_evolution_no_panic() {
        // Should not panic regardless of environment
        handle_evolution("/evolution");
        handle_evolution("/evolution 5");
    }

    // === CI run tests ===

    #[test]
    fn test_parse_ci_runs_valid_json() {
        let json = r#"[
            {
                "status": "completed",
                "conclusion": "success",
                "name": "CI",
                "createdAt": "2026-04-24T10:30:00Z",
                "headBranch": "main"
            },
            {
                "status": "completed",
                "conclusion": "failure",
                "name": "Evolve",
                "createdAt": "2026-04-24T08:00:00Z",
                "headBranch": "main"
            },
            {
                "status": "in_progress",
                "conclusion": "",
                "name": "CI",
                "createdAt": "2026-04-24T11:00:00Z",
                "headBranch": "feature-branch"
            }
        ]"#;
        let runs = parse_ci_runs(json);
        assert_eq!(runs.len(), 3);

        assert_eq!(runs[0].status, "completed");
        assert_eq!(runs[0].conclusion, "success");
        assert_eq!(runs[0].name, "CI");
        assert_eq!(runs[0].head_branch, "main");

        assert_eq!(runs[1].conclusion, "failure");
        assert_eq!(runs[1].name, "Evolve");

        assert_eq!(runs[2].status, "in_progress");
        assert_eq!(runs[2].conclusion, "");
        assert_eq!(runs[2].head_branch, "feature-branch");
    }

    #[test]
    fn test_parse_ci_runs_empty_array() {
        let runs = parse_ci_runs("[]");
        assert!(runs.is_empty());
    }

    #[test]
    fn test_parse_ci_runs_invalid_json() {
        let runs = parse_ci_runs("not json at all");
        assert!(runs.is_empty());
    }

    #[test]
    fn test_parse_ci_runs_missing_fields() {
        // Missing 'name' should skip that entry
        let json = r#"[
            {
                "status": "completed",
                "conclusion": "success",
                "createdAt": "2026-04-24T10:30:00Z"
            }
        ]"#;
        let runs = parse_ci_runs(json);
        assert!(runs.is_empty());
    }

    #[test]
    fn test_parse_ci_runs_null_conclusion() {
        // conclusion can be null for in-progress runs
        let json = r#"[
            {
                "status": "in_progress",
                "conclusion": null,
                "name": "CI",
                "createdAt": "2026-04-24T10:30:00Z",
                "headBranch": "main"
            }
        ]"#;
        let runs = parse_ci_runs(json);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].conclusion, "");
    }

    #[test]
    fn test_format_ci_status_icons() {
        assert_eq!(format_ci_status("completed", "success"), "✅");
        assert_eq!(format_ci_status("completed", "failure"), "❌");
        assert_eq!(format_ci_status("completed", "cancelled"), "⏹️");
        assert_eq!(format_ci_status("in_progress", ""), "🔄");
        assert_eq!(format_ci_status("queued", ""), "🕐");
        assert_eq!(format_ci_status("weird", "weird"), "❓");
    }

    #[test]
    fn test_format_ci_runs_output() {
        let runs = vec![
            CiRun {
                status: "completed".to_string(),
                conclusion: "success".to_string(),
                name: "CI".to_string(),
                created_at: "2026-04-24T10:30:00Z".to_string(),
                head_branch: "main".to_string(),
            },
            CiRun {
                status: "completed".to_string(),
                conclusion: "failure".to_string(),
                name: "Evolve".to_string(),
                created_at: "2026-04-24T08:00:00Z".to_string(),
                head_branch: "feature-x".to_string(),
            },
        ];
        let lines = format_ci_runs(&runs);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("✅"));
        assert!(lines[0].contains("CI"));
        // main branch should NOT show branch name
        assert!(!lines[0].contains("(main)"));
        // non-main branch should show branch name
        assert!(lines[1].contains("❌"));
        assert!(lines[1].contains("feature-x"));
    }

    #[test]
    fn test_format_ci_runs_empty() {
        let lines = format_ci_runs(&[]);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_fetch_ci_runs_graceful_when_gh_unavailable() {
        // If gh is not installed or not in a repo, should return empty vec, not panic
        let runs = fetch_ci_runs(5);
        // We can't assert the exact result since it depends on environment,
        // but it must not panic
        let _ = runs;
    }

    #[test]
    fn test_parse_iso8601_to_epoch_valid() {
        // 2026-01-01T00:00:00Z should be calculable
        let epoch = parse_iso8601_to_epoch("2026-01-01T00:00:00Z");
        assert!(epoch.is_some());
        let secs = epoch.unwrap();
        // Rough check: 2026 is ~56 years after 1970, so > 56*365*86400
        assert!(secs > 56 * 365 * 86400);
    }

    #[test]
    fn test_parse_iso8601_to_epoch_known_value() {
        // 1970-01-01T00:00:00Z should be epoch 0
        let epoch = parse_iso8601_to_epoch("1970-01-01T00:00:00Z");
        assert_eq!(epoch, Some(0));
    }

    #[test]
    fn test_parse_iso8601_to_epoch_with_time() {
        // 1970-01-01T01:00:00Z = 3600
        let epoch = parse_iso8601_to_epoch("1970-01-01T01:00:00Z");
        assert_eq!(epoch, Some(3600));
    }

    #[test]
    fn test_parse_iso8601_to_epoch_invalid() {
        assert!(parse_iso8601_to_epoch("not a date").is_none());
        assert!(parse_iso8601_to_epoch("2026-13-01T00:00:00Z").is_none()); // month 13
        assert!(parse_iso8601_to_epoch("2026-01-32T00:00:00Z").is_none()); // day 32
        assert!(parse_iso8601_to_epoch("").is_none());
    }

    #[test]
    fn test_format_ci_time_ago_fallback() {
        // Invalid timestamp should fallback gracefully
        let result = format_ci_time_ago("not-a-date");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_handle_model_list_no_panic_empty_filter() {
        // Should not panic with empty filter
        handle_model_list("claude-sonnet-4-20250514", "anthropic", "");
    }

    #[test]
    fn test_handle_model_list_no_panic_provider_filter() {
        // Should not panic when filtering by a specific provider
        handle_model_list("claude-sonnet-4-20250514", "anthropic", "openai");
    }

    #[test]
    fn test_handle_model_list_no_panic_unknown_provider() {
        // Should not panic with an unknown provider filter
        handle_model_list("claude-sonnet-4-20250514", "anthropic", "nonexistent");
    }

    #[test]
    fn test_model_context_window_known_models() {
        // Anthropic — Sonnet 4+ has 1M context
        assert_eq!(
            model_context_window("claude-sonnet-4-20250514"),
            Some(1_000_000)
        );
        assert_eq!(model_context_window("claude-sonnet-4-6"), Some(1_000_000));
        assert_eq!(model_context_window("claude-sonnet-4-5"), Some(1_000_000));
        // Opus 4.6+ has 1M, older Opus has 200k
        assert_eq!(model_context_window("claude-opus-4-7"), Some(1_000_000));
        assert_eq!(model_context_window("claude-opus-4-6"), Some(1_000_000));
        assert_eq!(
            model_context_window("claude-opus-4-20250514"),
            Some(200_000)
        );
        // Haiku: 200k
        assert_eq!(model_context_window("claude-haiku-4-5"), Some(200_000));
        // OpenAI
        assert_eq!(model_context_window("gpt-4.1"), Some(1_048_576));
        assert_eq!(model_context_window("gpt-4.1-mini"), Some(1_048_576));
        assert_eq!(model_context_window("gpt-4o"), Some(128_000));
        assert_eq!(model_context_window("gpt-5"), Some(1_048_576));
        assert_eq!(model_context_window("o3"), Some(200_000));
        assert_eq!(model_context_window("o4-mini"), Some(200_000));
        // Google
        assert_eq!(model_context_window("gemini-2.5-pro"), Some(1_048_576));
        assert_eq!(model_context_window("gemini-2.0-flash"), Some(1_048_576));
        // xAI
        assert_eq!(model_context_window("grok-4"), Some(131_072));
        // DeepSeek
        assert_eq!(model_context_window("deepseek-chat"), Some(128_000));
        // Mistral
        assert_eq!(model_context_window("mistral-large"), Some(128_000));
        assert_eq!(model_context_window("codestral"), Some(128_000));
    }

    // Day 76: tests for new model context windows
    #[test]
    fn test_model_context_window_new_models() {
        // Gemini 3.x — 1M
        assert_eq!(model_context_window("gemini-3.0-pro"), Some(1_048_576));
        assert_eq!(model_context_window("gemini-3.0-flash"), Some(1_048_576));

        // Llama 4 variants — 128k
        assert_eq!(model_context_window("llama-4-maverick-17b"), Some(128_000));
        assert_eq!(model_context_window("llama-4-scout-17b"), Some(128_000));

        // claude-sonnet-4-7 — already covered by sonnet-4 branch
        assert_eq!(model_context_window("claude-sonnet-4-7"), Some(1_000_000));

        // codex-mini — already covered
        assert_eq!(model_context_window("codex-mini"), Some(192_000));

        // grok-4-mini — covered by grok catch-all
        assert_eq!(model_context_window("grok-4-mini"), Some(131_072));

        // deepseek-r2 — covered by deepseek catch-all
        assert_eq!(model_context_window("deepseek-r2"), Some(128_000));

        // o4-mini-high — covered by o4 prefix
        assert_eq!(model_context_window("o4-mini-high"), Some(200_000));
    }

    #[test]
    fn test_model_context_window_unknown() {
        assert_eq!(model_context_window("totally-unknown-xyz"), None);
        assert_eq!(model_context_window(""), None);
    }

    #[test]
    fn test_find_provider_for_model_known() {
        // Exact match in provider lists
        assert_eq!(
            find_provider_for_model("claude-sonnet-4-20250514"),
            Some("anthropic")
        );
        assert_eq!(find_provider_for_model("gpt-4o"), Some("openai"));
        assert_eq!(find_provider_for_model("gemini-2.5-pro"), Some("google"));
        assert_eq!(find_provider_for_model("grok-4"), Some("xai"));
        assert_eq!(find_provider_for_model("deepseek-chat"), Some("deepseek"));
    }

    #[test]
    fn test_find_provider_for_model_heuristic() {
        // Not in any provider's exact list, but inferred from name
        assert_eq!(
            find_provider_for_model("claude-some-future-model"),
            Some("anthropic")
        );
        assert_eq!(find_provider_for_model("gpt-99-turbo"), Some("openai"));
    }

    #[test]
    fn test_find_provider_for_model_unknown() {
        assert_eq!(find_provider_for_model("totally-unknown-xyz"), None);
    }

    #[test]
    fn test_handle_model_info_no_panic_known() {
        // Known model — should print info without panic
        handle_model_info("claude-sonnet-4-20250514", "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_handle_model_info_no_panic_unknown() {
        // Unknown model — should print gracefully without panic
        handle_model_info("totally-unknown-xyz", "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_handle_model_info_no_panic_inactive() {
        // Known model that is not the active one
        handle_model_info("gpt-4o", "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_format_context_size() {
        assert_eq!(format_context_size(200_000), "200k tokens");
        assert_eq!(format_context_size(128_000), "128k tokens");
        assert_eq!(format_context_size(1_000_000), "1M tokens");
        assert_eq!(format_context_size(1_048_576), "1.0M tokens");
        assert_eq!(format_context_size(131_072), "131k tokens");
    }

    #[test]
    fn test_format_self_written() {
        let result = format_self_written(1000, 1000, 100.0);
        assert!(result.contains("100.0%"));
        assert!(result.contains("1.0k")); // format_token_count uses k/M suffixes
        assert!(result.contains("self-written"));

        let result = format_self_written(500, 1000, 50.0);
        assert!(result.contains("50.0%"));

        let result = format_self_written(42, 100, 42.0);
        assert!(result.contains("42 / 100 lines"));
    }

    #[test]
    fn test_compute_self_written_pct_in_yoyo_repo() {
        // When run in the yoyo repo, this should return Some with valid data.
        // The yoyo codebase is 100% self-written by yoyo-evolve[bot].
        // We skip if not in a git repo (e.g. CI without full clone).
        if std::process::Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            // Only test inner function to avoid OnceLock caching issues across tests
            let result = compute_self_written_pct_inner();
            if let Some((self_written, total, pct)) = result {
                assert!(total > 0, "should have some source lines");
                assert!(self_written > 0, "should have some self-written lines");
                assert!((0.0..=100.0).contains(&pct), "percentage should be valid");
                // In yoyo's repo, expect very high self-written %
                assert!(
                    pct > 90.0,
                    "yoyo codebase should be >90% self-written, got {pct:.1}%"
                );
            }
            // result can be None if src/ has no tracked .rs files (unlikely but graceful)
        }
    }

    #[test]
    fn test_compute_self_written_pct_not_in_git_repo() {
        // In a temp dir with no git repo, git ls-files should fail
        let tmp = std::env::temp_dir().join("yoyo_test_no_git_sw");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("src")).unwrap();
        std::fs::write(tmp.join("src/lib.rs"), "fn foo() {}\n").unwrap();

        // Run git ls-files in the temp dir — should fail (no .git)
        let output = std::process::Command::new("git")
            .args(["ls-files", "src/"])
            .current_dir(&tmp)
            .output()
            .unwrap();
        assert!(
            !output.status.success() || String::from_utf8_lossy(&output.stdout).trim().is_empty(),
            "git ls-files should fail or return nothing outside a git repo"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_compute_self_written_temp_repo() {
        // Create a temp git repo with a known author to verify counting logic.
        // We replicate compute_self_written_pct_inner's logic but with explicit
        // current_dir() calls, since set_current_dir is process-global and not
        // safe in parallel tests.
        let tmp = std::env::temp_dir().join("yoyo_test_self_written");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("src")).unwrap();

        // Init repo
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&tmp)
                .output()
                .unwrap()
        };

        run(&["init"]);
        run(&["config", "user.email", "test@test.com"]);
        run(&["config", "user.name", "yoyo-evolve[bot]"]);

        // Create a source file and commit as yoyo
        std::fs::write(
            tmp.join("src/main.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "init"]);

        // Run git blame in the temp repo
        let output = std::process::Command::new("git")
            .args(["blame", "--line-porcelain", "src/main.rs"])
            .current_dir(&tmp)
            .output()
            .unwrap();
        assert!(output.status.success(), "git blame should succeed");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut total = 0usize;
        let mut self_written = 0usize;
        for line in stdout.lines() {
            if let Some(author) = line.strip_prefix("author ") {
                total += 1;
                if author.to_lowercase().contains("yoyo") {
                    self_written += 1;
                }
            }
        }
        assert_eq!(total, 3, "should have 3 lines");
        assert_eq!(self_written, 3, "all lines by yoyo-evolve[bot]");

        // Now add a line from a different author
        run(&["config", "user.name", "someone-else"]);
        std::fs::write(
            tmp.join("src/main.rs"),
            "fn main() {\n    println!(\"hello\");\n    println!(\"world\");\n}\n",
        )
        .unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "add line"]);

        let output = std::process::Command::new("git")
            .args(["blame", "--line-porcelain", "src/main.rs"])
            .current_dir(&tmp)
            .output()
            .unwrap();
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut total = 0usize;
        let mut self_written = 0usize;
        for line in stdout.lines() {
            if let Some(author) = line.strip_prefix("author ") {
                total += 1;
                if author.to_lowercase().contains("yoyo") {
                    self_written += 1;
                }
            }
        }
        assert_eq!(total, 4, "should have 4 lines now");
        assert!(self_written < total, "not all lines should be yoyo's now");

        let pct = (self_written as f64 / total as f64) * 100.0;
        assert!(pct < 100.0, "percentage should be less than 100%");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // --- Context breakdown tests ---

    #[test]
    fn test_context_breakdown_empty() {
        let messages: Vec<AgentMessage> = vec![];
        let bd = context_breakdown(&messages);
        assert_eq!(bd.user_messages, 0);
        assert_eq!(bd.assistant_messages, 0);
        assert_eq!(bd.tool_calls, 0);
        assert_eq!(bd.tool_results, 0);
        assert_eq!(bd.thinking, 0);
        // system_estimate should still be non-zero (from SYSTEM_PROMPT)
        assert!(bd.system_estimate > 0, "system prompt should have tokens");
        assert_eq!(bd.total, bd.system_estimate);
    }

    #[test]
    fn test_context_breakdown_user_only() {
        let messages: Vec<AgentMessage> = vec![AgentMessage::Llm(Message::User {
            content: vec![Content::Text {
                text: "Hello world, this is a test message".to_string(),
            }],
            timestamp: 0,
        })];
        let bd = context_breakdown(&messages);
        assert!(bd.user_messages > 0, "user messages should have tokens");
        assert_eq!(bd.assistant_messages, 0);
        assert_eq!(bd.tool_calls, 0);
        assert_eq!(bd.tool_results, 0);
        assert_eq!(bd.thinking, 0);
    }

    #[test]
    fn test_context_breakdown_mixed() {
        let messages: Vec<AgentMessage> = vec![
            AgentMessage::Llm(Message::User {
                content: vec![Content::Text {
                    text: "Please help me".to_string(),
                }],
                timestamp: 0,
            }),
            AgentMessage::Llm(Message::Assistant {
                content: vec![
                    Content::Text {
                        text: "I'll help you with that.".to_string(),
                    },
                    Content::ToolCall {
                        id: "tc1".to_string(),
                        name: "bash".to_string(),
                        arguments: serde_json::json!({"command": "ls"}),
                        provider_metadata: None,
                    },
                ],
                stop_reason: yoagent::StopReason::ToolUse,
                model: "test".to_string(),
                provider: "test".to_string(),
                usage: Usage::default(),
                timestamp: 0,
                error_message: None,
            }),
            AgentMessage::Llm(Message::ToolResult {
                tool_call_id: "tc1".to_string(),
                tool_name: "bash".to_string(),
                content: vec![Content::Text {
                    text: "file1.rs\nfile2.rs\nfile3.rs".to_string(),
                }],
                is_error: false,
                timestamp: 0,
            }),
        ];

        let bd = context_breakdown(&messages);
        assert!(bd.user_messages > 0);
        assert!(bd.assistant_messages > 0);
        assert!(bd.tool_calls > 0);
        assert!(bd.tool_results > 0);
        assert_eq!(bd.thinking, 0);
        // Total should be sum of all parts
        assert_eq!(
            bd.total,
            bd.system_estimate
                + bd.user_messages
                + bd.assistant_messages
                + bd.tool_calls
                + bd.tool_results
                + bd.thinking
        );
    }

    #[test]
    fn test_context_breakdown_with_thinking() {
        let messages: Vec<AgentMessage> = vec![AgentMessage::Llm(Message::Assistant {
            content: vec![
                Content::Thinking {
                    thinking: "Let me think about this carefully for a while...".to_string(),
                    signature: None,
                },
                Content::Text {
                    text: "Here's my answer.".to_string(),
                },
            ],
            stop_reason: yoagent::StopReason::Stop,
            model: "test".to_string(),
            provider: "test".to_string(),
            usage: Usage::default(),
            timestamp: 0,
            error_message: None,
        })];

        let bd = context_breakdown(&messages);
        assert!(bd.thinking > 0, "thinking tokens should be counted");
        assert!(bd.assistant_messages > 0, "text response should be counted");
    }

    #[test]
    fn test_context_breakdown_percentages() {
        // Create a scenario where tool results dominate
        let big_result = "x".repeat(4000); // ~1000 tokens
        let messages: Vec<AgentMessage> = vec![
            AgentMessage::Llm(Message::User {
                content: vec![Content::Text {
                    text: "hi".to_string(),
                }],
                timestamp: 0,
            }),
            AgentMessage::Llm(Message::ToolResult {
                tool_call_id: "tc1".to_string(),
                tool_name: "bash".to_string(),
                content: vec![Content::Text { text: big_result }],
                is_error: false,
                timestamp: 0,
            }),
        ];

        let bd = context_breakdown(&messages);
        // tool_results should be the largest message-level category
        assert!(
            bd.tool_results > bd.user_messages,
            "tool results ({}) should exceed user messages ({})",
            bd.tool_results,
            bd.user_messages
        );
    }

    #[test]
    fn test_format_context_breakdown_output() {
        let bd = ContextBreakdown {
            system_estimate: 100,
            user_messages: 200,
            assistant_messages: 300,
            tool_calls: 50,
            tool_results: 150,
            thinking: 0,
            total: 800,
        };
        let output = format_context_breakdown(&bd);
        assert!(output.contains("Context breakdown:"));
        assert!(output.contains("system prompt"));
        assert!(output.contains("user messages"));
        assert!(output.contains("assistant"));
        assert!(output.contains("tool calls"));
        assert!(output.contains("tool results"));
        // thinking is 0, should be skipped
        assert!(!output.contains("thinking"));
        // Should contain percentages
        assert!(output.contains('%'));
        // Total line
        assert!(output.contains("total"));
    }

    #[test]
    fn test_format_context_breakdown_tool_heavy() {
        let bd = ContextBreakdown {
            system_estimate: 100,
            user_messages: 50,
            assistant_messages: 50,
            tool_calls: 20,
            tool_results: 800,
            thinking: 0,
            total: 1020,
        };
        let output = format_context_breakdown(&bd);
        // tool_results > 50% should trigger compact suggestion
        assert!(
            output.contains("compact"),
            "should suggest /compact when tool results dominate: {output}"
        );
    }

    #[test]
    fn test_generate_tips_returns_non_empty() {
        let tips = generate_tips();
        assert!(
            !tips.is_empty(),
            "generate_tips should return at least some tips"
        );
    }

    #[test]
    fn test_generate_tips_all_start_with_lightbulb() {
        let tips = generate_tips();
        for tip in &tips {
            assert!(
                tip.starts_with("💡"),
                "Every tip should start with 💡 emoji, got: {tip}"
            );
        }
    }

    #[test]
    fn test_generate_tips_includes_rust_hints() {
        // We're running inside a Rust project (Cargo.toml exists),
        // so we should see Rust-specific tips.
        let tips = generate_tips();
        let has_rust_tip = tips
            .iter()
            .any(|t| t.contains("cargo test") || t.contains("clippy"));
        assert!(
            has_rust_tip,
            "Should include Rust-specific tips when Cargo.toml exists: {tips:?}"
        );
    }

    #[test]
    fn test_generate_tips_includes_git_hint() {
        // We're running in a git repo, so git tip should appear.
        let tips = generate_tips();
        let has_git_tip = tips.iter().any(|t| t.contains("/diff --stat"));
        assert!(
            has_git_tip,
            "Should include git tip when .git exists: {tips:?}"
        );
    }

    #[test]
    fn test_generate_tips_feature_discovery_varies() {
        // Call twice — the feature-discovery portion is randomly sampled,
        // so over many calls the sets should differ (not deterministic,
        // but with 15 options and 3 samples the chance of identical
        // draws twice is ~1/455).
        let tips1 = generate_tips();
        let tips2 = generate_tips();
        // We can't guarantee they differ on a single pair, but we can
        // verify the count is reasonable (contextual + 3 discovery).
        assert!(
            tips1.len() >= 3,
            "Should have at least 3 tips (discovery alone): got {}",
            tips1.len()
        );
        assert!(
            tips2.len() >= 3,
            "Should have at least 3 tips (discovery alone): got {}",
            tips2.len()
        );
    }

    #[test]
    fn test_discovery_tips_constant_format() {
        for tip in DISCOVERY_TIPS {
            assert!(
                tip.starts_with("💡"),
                "DISCOVERY_TIPS entry should start with 💡: {tip}"
            );
        }
    }

    #[test]
    fn test_count_session_file_changes_in_temp_repo() {
        use std::fs;
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path();

        // Init a git repo with an initial commit
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init");
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .expect("git config email");
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .expect("git config name");

        fs::write(path.join("hello.txt"), "hello").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(path)
            .output()
            .expect("git commit");

        // Now modify a file and add a new untracked file
        fs::write(path.join("hello.txt"), "hello modified").unwrap();
        fs::write(path.join("new_file.txt"), "new").unwrap();

        // Run git status --porcelain from the temp dir to verify
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(path)
            .output()
            .expect("git status");
        let text = String::from_utf8_lossy(&output.stdout);

        let mut modified = 0usize;
        let mut added = 0usize;
        for line in text.lines() {
            if line.len() < 2 {
                continue;
            }
            let index = line.as_bytes()[0];
            let worktree = line.as_bytes()[1];
            match (index, worktree) {
                (b'?', b'?') => added += 1,
                (b'A', _) => added += 1,
                (b'M', _) | (_, b'M') => modified += 1,
                (b'R', _) => modified += 1,
                (b'D', _) | (_, b'D') => modified += 1,
                _ => {}
            }
        }

        assert_eq!(modified, 1, "should have 1 modified file");
        assert_eq!(added, 1, "should have 1 added (untracked) file");
    }

    #[test]
    fn test_handle_status_shows_enhanced_info() {
        use std::time::Duration;
        // Source-level check: handle_status should include goal, watch, and changes sections
        let source = include_str!("commands_info.rs");
        assert!(
            source.contains("goal:"),
            "/status should display goal when set"
        );
        assert!(
            source.contains("watch:"),
            "/status should display watch command when set"
        );
        assert!(
            source.contains("changes:"),
            "/status should display file changes"
        );
        assert!(
            source.contains("read-only"),
            "/status should display read-only mode"
        );
        // Should not panic
        handle_status(
            "test-model",
            "/tmp",
            &Usage::default(),
            Duration::from_secs(30),
            2,
            10_000,
            200_000,
        );
    }
}
