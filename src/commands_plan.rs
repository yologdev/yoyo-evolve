//! Plan command handler: `/plan` — toggle plan mode or create a structured plan.

use crate::commands::auto_compact_if_needed;
use crate::format::*;
use crate::prompt::run_prompt;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use yoagent::agent::Agent;
use yoagent::*;

// ---------------------------------------------------------------------------
// Plan mode — a session toggle that restricts the agent to read-only operations.
// When active, a constraint instruction is prepended to each user message so
// the agent reads and thinks but does not modify files or run destructive commands.
// ---------------------------------------------------------------------------

static PLAN_MODE: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Plan-apply mode — tracks whether `/plan apply` is currently executing.
// When active, the auto-continue limit is raised so the agent can work
// through the full plan without hitting the normal follow-up cap.
// ---------------------------------------------------------------------------

static PLAN_APPLY_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Set whether a `/plan apply` execution is currently in progress.
pub fn set_plan_apply_active(active: bool) {
    PLAN_APPLY_ACTIVE.store(active, Ordering::Relaxed);
}

/// Check whether a `/plan apply` execution is currently in progress.
pub fn is_plan_apply_active() -> bool {
    PLAN_APPLY_ACTIVE.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Structured plan types — a plan is parsed into numbered steps with tracking.
// ---------------------------------------------------------------------------

/// A single step within a structured plan.
#[derive(Debug, Clone)]
pub struct PlanStep {
    pub number: usize,
    pub title: String,
    pub description: String,
    pub completed: bool,
}

/// A structured plan: the original raw text plus parsed steps with completion tracking.
#[derive(Debug, Clone)]
pub struct StructuredPlan {
    pub raw_text: String,
    pub steps: Vec<PlanStep>,
}

/// Parse plan text into structured steps.
///
/// Recognizes several common formats:
/// - `1. **Title** — description` or `1. **Title**: description`
/// - `1. Title`
/// - `- [ ] Title` / `- [x] Title` (markdown checklist)
/// - `Step 1: Title`
pub fn parse_plan_steps(plan_text: &str) -> Vec<PlanStep> {
    let mut steps: Vec<PlanStep> = Vec::new();
    let mut current_desc_lines: Vec<String> = Vec::new();
    let lines: Vec<&str> = plan_text.lines().collect();

    for line in &lines {
        let trimmed = line.trim();

        // Try to match numbered list: `1. **Title** — desc` or `1. Title`
        if let Some(step) = try_parse_numbered(trimmed) {
            // Flush previous step's description
            flush_description(&mut steps, &mut current_desc_lines);
            steps.push(step);
            continue;
        }

        // Try to match markdown checklist: `- [ ] Title` or `- [x] Title`
        if let Some(step) = try_parse_checklist(trimmed, steps.len() + 1) {
            flush_description(&mut steps, &mut current_desc_lines);
            steps.push(step);
            continue;
        }

        // Try to match `Step N: Title`
        if let Some(step) = try_parse_step_prefix(trimmed) {
            flush_description(&mut steps, &mut current_desc_lines);
            steps.push(step);
            continue;
        }

        // Otherwise it's a continuation/description line for the current step
        if !steps.is_empty() && !trimmed.is_empty() {
            // Only add indented or clearly subordinate lines as description
            if line.starts_with("   ") || line.starts_with('\t') || trimmed.starts_with('-') {
                current_desc_lines.push(trimmed.to_string());
            }
        }
    }

    // Flush final step's description
    flush_description(&mut steps, &mut current_desc_lines);

    steps
}

fn flush_description(steps: &mut [PlanStep], desc_lines: &mut Vec<String>) {
    if !desc_lines.is_empty() {
        if let Some(last) = steps.last_mut() {
            if last.description.is_empty() {
                last.description = desc_lines.join("\n");
            } else {
                last.description.push('\n');
                last.description.push_str(&desc_lines.join("\n"));
            }
        }
        desc_lines.clear();
    }
}

/// Try parsing `1. **Title** — description` or `1. Title`
fn try_parse_numbered(line: &str) -> Option<PlanStep> {
    // Match: digits followed by `. ` or `) `
    let (num_str, rest) = if let Some(pos) = line.find(". ") {
        let num_part = &line[..pos];
        if num_part.chars().all(|c| c.is_ascii_digit()) && !num_part.is_empty() {
            (num_part, &line[pos + 2..])
        } else {
            return None;
        }
    } else {
        let pos = line.find(") ")?;
        let num_part = &line[..pos];
        if num_part.chars().all(|c| c.is_ascii_digit()) && !num_part.is_empty() {
            (num_part, &line[pos + 2..])
        } else {
            return None;
        }
    };

    let number: usize = num_str.parse().ok()?;

    // Extract title — strip bold markers
    let rest = rest.trim();
    let (title, description) = extract_title_and_desc(rest);

    Some(PlanStep {
        number,
        title,
        description,
        completed: false,
    })
}

/// Try parsing `- [ ] Title` or `- [x] Title`
fn try_parse_checklist(line: &str, default_number: usize) -> Option<PlanStep> {
    let rest = if let Some(r) = line.strip_prefix("- [ ] ") {
        r
    } else if let Some(r) = line.strip_prefix("- [x] ") {
        return Some(PlanStep {
            number: default_number,
            title: r.trim().to_string(),
            description: String::new(),
            completed: true,
        });
    } else {
        let r = line.strip_prefix("- [X] ")?;
        return Some(PlanStep {
            number: default_number,
            title: r.trim().to_string(),
            description: String::new(),
            completed: true,
        });
    };

    Some(PlanStep {
        number: default_number,
        title: rest.trim().to_string(),
        description: String::new(),
        completed: false,
    })
}

/// Try parsing `Step N: Title`
fn try_parse_step_prefix(line: &str) -> Option<PlanStep> {
    let lower = line.to_lowercase();
    let rest = lower.strip_prefix("step ")?;
    // Find the number
    let colon_pos = rest.find(':')?;
    let num_str = rest[..colon_pos].trim();
    let number: usize = num_str.parse().ok()?;

    // Get title from the original line (preserving case)
    let original_rest = &line["step ".len()..];
    let original_colon_pos = original_rest.find(':')?;
    let title = original_rest[original_colon_pos + 1..].trim().to_string();

    Some(PlanStep {
        number,
        title,
        description: String::new(),
        completed: false,
    })
}

/// Extract title and description from text that may have bold markers and separators.
fn extract_title_and_desc(text: &str) -> (String, String) {
    // Strip leading ** and find closing **
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("**") {
        if let Some(end_bold) = rest.find("**") {
            let title = rest[..end_bold].to_string();
            let after = rest[end_bold + 2..].trim();
            // Strip leading separator (—, :, -, etc.)
            let desc = after
                .strip_prefix('—')
                .or_else(|| after.strip_prefix(':'))
                .or_else(|| after.strip_prefix('-'))
                .unwrap_or(after)
                .trim()
                .to_string();
            return (title, desc);
        }
    }

    // No bold markers — just use the whole thing as title
    // But split on — or : if present for description
    if let Some(pos) = text.find(" — ") {
        let title = text[..pos].trim().to_string();
        let desc = text[pos + " — ".len()..].trim().to_string();
        return (title, desc);
    }

    (text.to_string(), String::new())
}

// ---------------------------------------------------------------------------
// Last plan storage — holds the structured plan so the user can review
// (/plan show), track progress (/plan status), or execute (/plan apply) it.
// ---------------------------------------------------------------------------

static LAST_PLAN: Mutex<Option<StructuredPlan>> = Mutex::new(None);

/// Store the text of the last generated plan (parses into structured steps).
pub fn set_last_plan(plan: String) {
    if let Ok(mut guard) = LAST_PLAN.lock() {
        let steps = parse_plan_steps(&plan);
        *guard = Some(StructuredPlan {
            raw_text: plan,
            steps,
        });
    }
}

/// Retrieve the last stored plan, if any.
pub fn get_last_plan() -> Option<StructuredPlan> {
    LAST_PLAN.lock().ok().and_then(|g| g.clone())
}

/// Clear the stored plan.
pub fn clear_last_plan() {
    if let Ok(mut guard) = LAST_PLAN.lock() {
        *guard = None;
    }
}

/// Mark a step as completed or not.
pub fn mark_step(step_number: usize, completed: bool) -> Result<(), String> {
    if let Ok(mut guard) = LAST_PLAN.lock() {
        if let Some(plan) = guard.as_mut() {
            if let Some(step) = plan.steps.iter_mut().find(|s| s.number == step_number) {
                step.completed = completed;
                Ok(())
            } else {
                Err(format!("No step {step_number} found in the current plan."))
            }
        } else {
            Err("No plan stored. Use /plan <task> to create one first.".to_string())
        }
    } else {
        Err("Failed to access plan state.".to_string())
    }
}

/// Format the plan status display.
pub fn format_plan_status(plan: &StructuredPlan) -> String {
    if plan.steps.is_empty() {
        return "  Plan has no parseable steps.\n\n  Raw plan text stored — use /plan show to view.".to_string();
    }

    let mut output = String::new();
    let total = plan.steps.len();
    let done = plan.steps.iter().filter(|s| s.completed).count();

    output.push_str(&format!(
        "  📋 Plan progress: {done}/{total} steps complete"
    ));
    if total > 0 {
        let pct = (done * 100).checked_div(total).unwrap_or(0);
        output.push_str(&format!(" ({pct}%)"));
    }
    output.push_str("\n\n");

    let mut next_incomplete_shown = false;
    for step in &plan.steps {
        let check = if step.completed { "x" } else { " " };
        let marker = if !step.completed && !next_incomplete_shown {
            next_incomplete_shown = true;
            "→"
        } else {
            " "
        };
        output.push_str(&format!(
            "  {marker} [{check}] Step {}: {}",
            step.number, step.title
        ));
        if !step.description.is_empty() {
            // Show a truncated description on the same line
            let short_desc = if step.description.len() > 60 {
                let mut b = 60;
                while b > 0 && !step.description.is_char_boundary(b) {
                    b -= 1;
                }
                format!("{}…", &step.description[..b])
            } else {
                step.description.clone()
            };
            output.push_str(&format!("\n       {short_desc}"));
        }
        output.push('\n');
    }

    output
}

/// Enable or disable plan mode.
pub fn set_plan_mode(enabled: bool) {
    PLAN_MODE.store(enabled, Ordering::Relaxed);
}

/// Check whether plan mode is currently active.
pub fn is_plan_mode() -> bool {
    PLAN_MODE.load(Ordering::Relaxed)
}

/// Instruction prepended to user messages when plan mode is on.
pub const PLAN_MODE_PROMPT: &str = "\
[PLAN MODE] You are in planning mode. You may read files, search, and analyze the codebase, \
but you MUST NOT modify any files or run destructive commands. Specifically:
- DO NOT use write_file or edit_file
- DO NOT use bash commands that create, modify, or delete files
- You MAY use read_file, list_files, search, and read-only bash commands (cat, grep, find, git log, git status, git diff)
Analyze the codebase, explain your plan, and describe what changes you WOULD make without making them.";

/// Subcommand names for `/plan <Tab>` completion.
pub const PLAN_SUBCOMMANDS: &[&str] = &[
    "on", "off", "open", "close", "show", "apply", "clear", "status", "step", "--deep",
];

/// Parse a `/plan` command and extract the task description.
/// Returns None if no task was provided or if the input is a mode toggle keyword.
pub fn parse_plan_task(input: &str) -> Option<String> {
    let task = input.strip_prefix("/plan").unwrap_or("").trim().to_string();
    if task.is_empty() {
        None
    } else {
        // Don't treat mode toggle keywords as plan tasks
        match task.as_str() {
            "on" | "off" | "open" | "close" | "show" | "apply" | "clear" | "status" | "step" => {
                None
            }
            _ => Some(task),
        }
    }
}

/// Strip a leading/trailing `--deep` flag from a plan task string, returning the
/// cleaned task and whether the flag was present. The `--deep` flag is opt-in and
/// requests per-step TDD (RED/GREEN/REFACTOR) structure in the generated plan.
pub fn extract_deep_flag(task: &str) -> (String, bool) {
    let mut deep = false;
    let cleaned: Vec<&str> = task
        .split_whitespace()
        .filter(|word| {
            if *word == "--deep" {
                deep = true;
                false
            } else {
                true
            }
        })
        .collect();
    (cleaned.join(" "), deep)
}

/// Build a planning-mode prompt that asks the agent to create a structured plan
/// WITHOUT executing any tools. This is the "architect mode" equivalent.
///
/// When `deep` is true (the opt-in `/plan --deep` flag), the prompt additionally
/// requests per-step TDD structure — a RED / GREEN / REFACTOR breakdown for each
/// implementation step. The default (fast, broad) pass is unchanged.
pub fn build_plan_prompt(task: &str, deep: bool) -> String {
    let base = format!(
        r#"Create a detailed step-by-step plan for the following task. Do NOT execute any tools — this is planning only.

## Task
{task}

## Instructions
Analyze the task and produce a structured plan covering:

1. **Files to examine** — which existing files need to be read to understand the current state
2. **Files to modify** — which files will be created or changed. For EACH file you list, include an `Approach:` line that states *what* changes in that file and *how* — the specific function, method, class, section, or config block to touch and the nature of the edit (add / replace / delete / rename). Do not merely name the file; describe the concrete change. This applies to any language (Go, Python, JS, Rust, etc.) — name the language-appropriate unit (function, struct, class, module, package).
3. **Step-by-step approach** — ordered list of concrete implementation steps
4. **Tests to write** — what tests should be added or updated
5. **Potential risks** — what could go wrong, edge cases, backwards compatibility concerns
6. **Verification** — how to confirm the changes work correctly

Be specific: mention file paths, function names, and concrete code changes where possible.
Keep the plan actionable — someone (or you, in the next step) should be able to execute it directly."#
    );

    if !deep {
        return base;
    }

    format!(
        r#"{base}

## Deep TDD structure (--deep)
For EACH numbered implementation step in the step-by-step approach, add a test-driven
breakdown with these three lines:

- **TDD RED** — the failing test to write first: name the test and the exact behavior/assertion
  it checks (the test should fail before the code exists).
- **TDD GREEN** — the minimal code change that makes that test pass — nothing more.
- **TDD REFACTOR** — cleanup notes: what to simplify, deduplicate, or rename once the test is green
  (or "none" if nothing to refactor).

This is language-agnostic: RED/GREEN/REFACTOR applies whether the tests are Go, Python, JS, or Rust."#
    )
}

/// Build a prompt that instructs the agent to execute a previously generated plan.
pub fn build_apply_prompt(plan_text: &str) -> String {
    format!(
        "Execute the following plan. Implement each step, writing code and running tests as you go.\n\n\
         ## Plan\n{plan_text}\n\n\
         Work through each step. After completing all steps, verify with `cargo build && cargo test` \
         (or the project's equivalent)."
    )
}

/// Result of handling a `/plan` command.
pub enum PlanResult {
    /// Command handled internally (toggle, show, clear, or no-op). Continue the REPL.
    Handled,
    /// A plan was generated. Contains the plan prompt used (stored as last_input).
    PlanGenerated(String),
    /// The user requested `/plan apply` — the returned string should be sent to the agent.
    Apply(String),
}

/// Handle `/plan step N done` or `/plan step N undo`.
fn handle_plan_step(step_arg: &str) -> PlanResult {
    let parts: Vec<&str> = step_arg.split_whitespace().collect();
    if parts.is_empty() {
        println!("{DIM}  Usage: /plan step <N> done|undo{RESET}\n");
        return PlanResult::Handled;
    }

    let number: usize = match parts[0].parse() {
        Ok(n) => n,
        Err(_) => {
            println!("{RED}  Invalid step number: '{}'{RESET}\n", parts[0]);
            return PlanResult::Handled;
        }
    };

    let action = parts.get(1).copied().unwrap_or("done");
    let completed = match action {
        "done" | "complete" | "check" => true,
        "undo" | "uncomplete" | "uncheck" => false,
        other => {
            println!("{RED}  Unknown step action: '{other}'. Use 'done' or 'undo'.{RESET}\n");
            return PlanResult::Handled;
        }
    };

    match mark_step(number, completed) {
        Ok(()) => {
            let verb = if completed {
                "completed"
            } else {
                "uncompleted"
            };
            println!("{GREEN}  ✓ Step {number} marked as {verb}.{RESET}");
            // Show updated status
            if let Some(plan) = get_last_plan() {
                println!("{}", format_plan_status(&plan));
            }
        }
        Err(e) => {
            println!("{RED}  {e}{RESET}\n");
        }
    }

    PlanResult::Handled
}

/// Handle the `/plan` command: toggle plan mode, create a structured plan,
/// or manage stored plans.
///
/// - `/plan on` or `/plan open` — enable plan mode (read-only)
/// - `/plan off` or `/plan close` — disable plan mode
/// - `/plan` (no args) — show current mode + usage
/// - `/plan <task>` — single-shot plan (generates + stores for later apply)
/// - `/plan show` — display the last generated plan
/// - `/plan apply` — execute the last plan via the agent
/// - `/plan clear` — discard the stored plan
pub async fn handle_plan(
    input: &str,
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> PlanResult {
    let arg = input.strip_prefix("/plan").unwrap_or("").trim();

    // Handle `/plan step N done|undo` before the main match
    if let Some(step_arg) = arg.strip_prefix("step ") {
        return handle_plan_step(step_arg);
    }

    // Handle mode toggle subcommands
    match arg {
        "on" | "open" => {
            set_plan_mode(true);
            println!(
                "{GREEN}  📋 Plan mode ON — agent will read and think but not modify files or run commands.{RESET}"
            );
            println!("{DIM}  Use /plan off to return to normal mode.{RESET}\n");
            return PlanResult::Handled;
        }
        "off" | "close" => {
            set_plan_mode(false);
            println!("{DIM}  Plan mode OFF — normal operation resumed.{RESET}\n");
            return PlanResult::Handled;
        }
        "show" => {
            match get_last_plan() {
                Some(plan) => {
                    println!("{BOLD}  📋 Last generated plan:{RESET}\n");
                    println!("{}\n", plan.raw_text);
                }
                None => {
                    println!("{DIM}  No plan stored. Use /plan <task> to create one.{RESET}\n");
                }
            }
            return PlanResult::Handled;
        }
        "status" => {
            match get_last_plan() {
                Some(plan) => {
                    println!("{}", format_plan_status(&plan));
                }
                None => {
                    println!("{DIM}  No plan stored. Use /plan <task> to create one.{RESET}\n");
                }
            }
            return PlanResult::Handled;
        }
        "apply" => match get_last_plan() {
            Some(plan) => {
                let prompt = build_apply_prompt(&plan.raw_text);
                println!("{GREEN}  🚀 Applying stored plan…{RESET}\n");
                clear_last_plan();
                return PlanResult::Apply(prompt);
            }
            None => {
                println!("{DIM}  No plan stored. Use /plan <task> to create one first.{RESET}\n");
                return PlanResult::Handled;
            }
        },
        "clear" => {
            clear_last_plan();
            println!("{DIM}  Stored plan cleared.{RESET}\n");
            return PlanResult::Handled;
        }
        "" => {
            // No args: show status + usage
            if is_plan_mode() {
                println!("{GREEN}  📋 Plan mode is ON{RESET}");
                println!("{DIM}  The agent can read and search but will not modify files.{RESET}");
                println!("{DIM}  Use /plan off to return to normal mode.{RESET}\n");
            } else {
                let has_plan = get_last_plan().is_some();
                println!("{DIM}  📋 Plan mode is OFF (normal operation){RESET}");
                println!("{DIM}  usage: /plan on          Enter plan mode (read-only){RESET}");
                println!("{DIM}         /plan off         Return to normal mode{RESET}");
                println!(
                    "{DIM}         /plan <task>      One-shot plan without executing tools{RESET}"
                );
                println!("{DIM}         /plan show        Display the last generated plan{RESET}");
                println!("{DIM}         /plan apply       Execute the last generated plan{RESET}");
                println!("{DIM}         /plan clear       Discard the stored plan{RESET}");
                if has_plan {
                    println!(
                        "{GREEN}  ✓ A plan is currently stored. Use /plan show to view it.{RESET}"
                    );
                }
                println!();
            }
            return PlanResult::Handled;
        }
        _ => {}
    }

    // Fall through to single-shot planning
    let task = match parse_plan_task(input) {
        Some(t) => t,
        None => {
            // Shouldn't reach here given the match above, but be safe
            return PlanResult::Handled;
        }
    };

    println!("{DIM}  📋 Planning: {task}{RESET}\n");

    let (clean_task, deep) = extract_deep_flag(&task);
    if deep {
        println!("{DIM}  🔬 Deep mode — requesting per-step TDD (RED/GREEN/REFACTOR) structure.{RESET}\n");
    }

    let plan_prompt = build_plan_prompt(&clean_task, deep);
    run_prompt(agent, &plan_prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Capture the plan text from the last assistant message for later retrieval
    if let Some(plan_text) = crate::commands_web::extract_last_assistant_text(agent.messages()) {
        set_last_plan(plan_text);
    }

    println!(
        "\n{DIM}  💡 Review the plan above. Use /plan apply to execute it, or refine it.{RESET}\n"
    );

    PlanResult::PlanGenerated(plan_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plan_task_with_description() {
        let result = parse_plan_task("/plan add error handling to the parser");
        assert_eq!(result, Some("add error handling to the parser".to_string()));
    }

    #[test]
    fn parse_plan_task_empty() {
        let result = parse_plan_task("/plan");
        assert!(result.is_none(), "Empty /plan should return None");
    }

    #[test]
    fn parse_plan_task_whitespace_only() {
        let result = parse_plan_task("/plan   ");
        assert!(result.is_none(), "Whitespace-only /plan should return None");
    }

    #[test]
    fn parse_plan_task_preserves_full_description() {
        let result = parse_plan_task("/plan refactor main.rs into smaller modules with tests");
        assert_eq!(
            result,
            Some("refactor main.rs into smaller modules with tests".to_string())
        );
    }

    #[test]
    fn build_plan_prompt_contains_task() {
        let prompt = build_plan_prompt("add a /plan command", false);
        assert!(
            prompt.contains("add a /plan command"),
            "Plan prompt should contain the task"
        );
    }

    #[test]
    fn build_plan_prompt_contains_no_tools_instruction() {
        let prompt = build_plan_prompt("something", false);
        assert!(
            prompt.contains("Do NOT execute any tools"),
            "Plan prompt should instruct not to use tools"
        );
    }

    #[test]
    fn build_plan_prompt_contains_structure_sections() {
        let prompt = build_plan_prompt("add feature X", false);
        assert!(
            prompt.contains("Files to examine"),
            "Should mention files to examine"
        );
        assert!(
            prompt.contains("Files to modify"),
            "Should mention files to modify"
        );
        assert!(
            prompt.contains("Step-by-step"),
            "Should mention step-by-step approach"
        );
        assert!(prompt.contains("Tests to write"), "Should mention tests");
        assert!(prompt.contains("Potential risks"), "Should mention risks");
        assert!(
            prompt.contains("Verification"),
            "Should mention verification"
        );
    }

    #[test]
    fn build_plan_prompt_demands_per_file_approach() {
        let prompt = build_plan_prompt("add a config loader", false);
        // The first-pass plan must instruct a per-file "Approach:" line (#583),
        // so file-level implementation depth appears without a manual second pass.
        assert!(
            prompt.contains("`Approach:` line"),
            "Should demand a per-file Approach line: {prompt}"
        );
        assert!(
            prompt.contains("*what* changes in that file and *how*"),
            "Should require both what and how for each file: {prompt}"
        );
        // Language-agnostic — must not assume Rust (product-safe, Day-448 lesson).
        assert!(
            prompt.contains("Go, Python, JS, Rust"),
            "Should be explicitly language-agnostic: {prompt}"
        );
    }

    #[test]
    fn build_plan_prompt_deep_adds_tdd_structure() {
        // --deep (opt-in) must request per-step RED/GREEN/REFACTOR TDD structure (#583).
        let prompt = build_plan_prompt("add a config loader", true);
        assert!(
            prompt.contains("TDD RED"),
            "Deep plan should request a TDD RED line per step: {prompt}"
        );
        assert!(
            prompt.contains("TDD GREEN"),
            "Deep plan should request a TDD GREEN line per step: {prompt}"
        );
        assert!(
            prompt.contains("TDD REFACTOR"),
            "Deep plan should request a TDD REFACTOR line per step: {prompt}"
        );
        // The per-file Approach line (Day-132) must remain present in deep mode too.
        assert!(
            prompt.contains("`Approach:` line"),
            "Deep plan must keep the per-file Approach line: {prompt}"
        );
    }

    #[test]
    fn build_plan_prompt_default_has_no_tdd_structure() {
        // Paired negative (Day-122 near-miss discipline): the default fast/broad pass
        // must NOT include the deep TDD structure — --deep is additive/opt-in only.
        let prompt = build_plan_prompt("add a config loader", false);
        assert!(
            !prompt.contains("TDD RED"),
            "Default plan must not include TDD RED: {prompt}"
        );
        assert!(
            !prompt.contains("TDD GREEN"),
            "Default plan must not include TDD GREEN: {prompt}"
        );
        assert!(
            !prompt.contains("TDD REFACTOR"),
            "Default plan must not include TDD REFACTOR: {prompt}"
        );
        // But the per-file Approach line must be present in both modes.
        assert!(
            prompt.contains("`Approach:` line"),
            "Default plan must keep the per-file Approach line: {prompt}"
        );
    }

    #[test]
    fn extract_deep_flag_detects_and_strips() {
        let (task, deep) = extract_deep_flag("add auth --deep");
        assert!(deep, "Should detect --deep flag");
        assert_eq!(task, "add auth", "Should strip --deep from the task");

        let (task, deep) = extract_deep_flag("--deep refactor parser");
        assert!(deep, "Should detect --deep flag at the start");
        assert_eq!(task, "refactor parser");
    }

    #[test]
    fn extract_deep_flag_absent_leaves_task_intact() {
        // Paired negative: no flag → deep=false, task unchanged.
        let (task, deep) = extract_deep_flag("add auth to the login flow");
        assert!(!deep, "Should not report deep when flag absent");
        assert_eq!(task, "add auth to the login flow");
    }

    #[test]
    fn test_parse_plan_task_extracts_task() {
        let result = parse_plan_task("/plan add error handling");
        assert_eq!(result, Some("add error handling".to_string()));
    }

    #[test]
    fn test_parse_plan_task_empty_returns_none() {
        assert!(parse_plan_task("/plan").is_none());
        assert!(parse_plan_task("/plan  ").is_none());
    }

    #[test]
    fn test_build_plan_prompt_structure() {
        let prompt = build_plan_prompt("migrate database schema", false);
        assert!(prompt.contains("migrate database schema"));
        assert!(prompt.contains("Do NOT execute any tools"));
        assert!(prompt.contains("Files to examine"));
        assert!(prompt.contains("Step-by-step"));
    }

    #[test]
    fn test_plan_mode_toggle() {
        // Ensure clean state
        set_plan_mode(false);
        assert!(!is_plan_mode());

        set_plan_mode(true);
        assert!(is_plan_mode());

        set_plan_mode(false);
        assert!(!is_plan_mode());
    }

    #[test]
    fn test_parse_plan_task_skips_mode_keywords() {
        // Mode toggle keywords should NOT be treated as plan tasks
        assert!(parse_plan_task("/plan on").is_none());
        assert!(parse_plan_task("/plan off").is_none());
        assert!(parse_plan_task("/plan open").is_none());
        assert!(parse_plan_task("/plan close").is_none());

        // But actual task descriptions should still work
        assert_eq!(
            parse_plan_task("/plan add error handling"),
            Some("add error handling".to_string())
        );
        assert_eq!(
            parse_plan_task("/plan on-boarding flow"),
            Some("on-boarding flow".to_string())
        );
    }

    #[test]
    fn test_plan_mode_prompt_content() {
        // The plan mode prompt should instruct the agent not to modify files
        assert!(PLAN_MODE_PROMPT.contains("PLAN MODE"));
        assert!(PLAN_MODE_PROMPT.contains("MUST NOT"));
        assert!(PLAN_MODE_PROMPT.contains("write_file"));
        assert!(PLAN_MODE_PROMPT.contains("edit_file"));
        assert!(PLAN_MODE_PROMPT.contains("read_file"));
    }

    #[test]
    fn test_plan_subcommands() {
        assert!(PLAN_SUBCOMMANDS.contains(&"on"));
        assert!(PLAN_SUBCOMMANDS.contains(&"off"));
        assert!(PLAN_SUBCOMMANDS.contains(&"open"));
        assert!(PLAN_SUBCOMMANDS.contains(&"close"));
        assert!(PLAN_SUBCOMMANDS.contains(&"show"));
        assert!(PLAN_SUBCOMMANDS.contains(&"apply"));
        assert!(PLAN_SUBCOMMANDS.contains(&"clear"));
        assert!(PLAN_SUBCOMMANDS.contains(&"status"));
        assert!(PLAN_SUBCOMMANDS.contains(&"step"));
    }

    #[test]
    fn test_parse_plan_steps_numbered_list() {
        let plan = "\
1. **Set up the database** — configure connection pooling
2. **Create migrations** — add users and posts tables
3. **Implement models** — define structs for User and Post";
        let steps = parse_plan_steps(plan);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].number, 1);
        assert_eq!(steps[0].title, "Set up the database");
        assert_eq!(steps[0].description, "configure connection pooling");
        assert!(!steps[0].completed);
        assert_eq!(steps[1].number, 2);
        assert_eq!(steps[1].title, "Create migrations");
        assert_eq!(steps[2].number, 3);
        assert_eq!(steps[2].title, "Implement models");
    }

    #[test]
    fn test_parse_plan_steps_markdown_checklist() {
        let plan = "\
- [ ] Add error handling
- [x] Write unit tests
- [ ] Update documentation";
        let steps = parse_plan_steps(plan);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].number, 1);
        assert_eq!(steps[0].title, "Add error handling");
        assert!(!steps[0].completed);
        assert_eq!(steps[1].number, 2);
        assert_eq!(steps[1].title, "Write unit tests");
        assert!(steps[1].completed);
        assert_eq!(steps[2].number, 3);
        assert_eq!(steps[2].title, "Update documentation");
        assert!(!steps[2].completed);
    }

    #[test]
    fn test_parse_plan_steps_mixed_formats() {
        let plan = "\
1. **First step** — do the first thing
Step 2: Second step
- [ ] Third step";
        let steps = parse_plan_steps(plan);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].number, 1);
        assert_eq!(steps[0].title, "First step");
        assert_eq!(steps[1].number, 2);
        assert_eq!(steps[1].title, "Second step");
        assert_eq!(steps[2].number, 3);
        assert_eq!(steps[2].title, "Third step");
    }

    #[test]
    fn test_step_marking_done_and_undo() {
        // Clear any previous plan state
        clear_last_plan();
        set_last_plan("1. Step one\n2. Step two\n3. Step three".to_string());

        // Mark step 2 as done
        assert!(mark_step(2, true).is_ok());
        let plan = get_last_plan().unwrap();
        assert!(!plan.steps[0].completed);
        assert!(plan.steps[1].completed);
        assert!(!plan.steps[2].completed);

        // Undo step 2
        assert!(mark_step(2, false).is_ok());
        let plan = get_last_plan().unwrap();
        assert!(!plan.steps[1].completed);

        // Non-existent step
        assert!(mark_step(99, true).is_err());

        // Clean up
        clear_last_plan();
    }

    #[test]
    fn test_format_plan_status_display() {
        let plan = StructuredPlan {
            raw_text: String::new(),
            steps: vec![
                PlanStep {
                    number: 1,
                    title: "First".to_string(),
                    description: String::new(),
                    completed: true,
                },
                PlanStep {
                    number: 2,
                    title: "Second".to_string(),
                    description: String::new(),
                    completed: false,
                },
                PlanStep {
                    number: 3,
                    title: "Third".to_string(),
                    description: String::new(),
                    completed: false,
                },
            ],
        };
        let status = format_plan_status(&plan);
        assert!(status.contains("1/3 steps complete"));
        assert!(status.contains("33%"));
        assert!(status.contains("[x] Step 1: First"));
        assert!(status.contains("[ ] Step 2: Second"));
        // The next incomplete step should have the → marker
        assert!(status.contains("→"));
    }

    #[test]
    fn test_parse_plan_steps_empty_plan() {
        let steps = parse_plan_steps("");
        assert!(steps.is_empty());

        let steps = parse_plan_steps("   \n\n  \n");
        assert!(steps.is_empty());
    }

    #[test]
    fn test_parse_plan_steps_no_parseable_steps() {
        let plan = "This is just a paragraph of text without any numbered steps\n\
                     or checklists. It describes what to do but not in a structured way.";
        let steps = parse_plan_steps(plan);
        assert!(steps.is_empty());

        // When stored as a StructuredPlan, raw_text is preserved
        clear_last_plan();
        set_last_plan(plan.to_string());
        let stored = get_last_plan().unwrap();
        assert!(stored.steps.is_empty());
        assert_eq!(stored.raw_text, plan);

        // Status display should handle empty steps gracefully
        let status = format_plan_status(&stored);
        assert!(status.contains("no parseable steps"));

        clear_last_plan();
    }

    #[test]
    fn test_plan_in_known_commands() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/plan"),
            "/plan should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_plan_in_help_text() {
        use crate::help::help_text;
        let help = help_text();
        assert!(help.contains("/plan"), "/plan should appear in help text");
        assert!(
            help.contains("architect"),
            "Help text should mention architect mode"
        );
    }
}
