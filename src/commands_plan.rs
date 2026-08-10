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
    /// Whether `/plan apply` has already dispatched this plan at least once.
    /// The plan is NOT consumed at dispatch (so a run that stops partway can be
    /// resumed) — this flag only makes the repeat visible.
    pub applied: bool,
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

/// One small flag so "cleared this session" and "never existed" don't collapse
/// into the same message. Set by `clear_last_plan` (only when a plan actually
/// existed), reset by `set_last_plan`.
static PLAN_WAS_CLEARED: AtomicBool = AtomicBool::new(false);

/// Store the text of the last generated plan (parses into structured steps).
pub fn set_last_plan(plan: String) {
    if let Ok(mut guard) = LAST_PLAN.lock() {
        let steps = parse_plan_steps(&plan);
        *guard = Some(StructuredPlan {
            raw_text: plan,
            steps,
            applied: false,
        });
    }
    // A fresh plan replaces any "was cleared" history.
    PLAN_WAS_CLEARED.store(false, Ordering::Relaxed);
}

/// Mark the stored plan as applied (dispatched at least once).
///
/// Returns `true` if the plan had *already* been applied before this call, so
/// the caller can print an honest re-apply line instead of pretending it's the
/// first run.
pub fn mark_plan_applied() -> bool {
    if let Ok(mut guard) = LAST_PLAN.lock() {
        if let Some(plan) = guard.as_mut() {
            let was_applied = plan.applied;
            plan.applied = true;
            return was_applied;
        }
    }
    false
}

/// Retrieve the last stored plan, if any.
pub fn get_last_plan() -> Option<StructuredPlan> {
    LAST_PLAN.lock().ok().and_then(|g| g.clone())
}

/// Clear the stored plan.
pub fn clear_last_plan() {
    if let Ok(mut guard) = LAST_PLAN.lock() {
        // Only remember an *explicit discard of a real plan*, so a plain
        // `/plan clear` on an empty slot doesn't rewrite history.
        if guard.is_some() {
            PLAN_WAS_CLEARED.store(true, Ordering::Relaxed);
        }
        *guard = None;
    }
}

/// What `/plan apply` should do at dispatch time, derived purely from the
/// stored-plan state. Kept separate from the handler so every branch —
/// including the near-miss (a first apply must NOT say "re-applying") — is
/// unit-testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyDispatch {
    /// A plan is stored and hasn't been dispatched yet.
    First,
    /// A plan is stored and was already dispatched at least once — run it
    /// again, but say so.
    ReApply,
    /// A plan existed this session and was explicitly discarded.
    Cleared,
    /// No plan was ever stored this session.
    Missing,
}

impl ApplyDispatch {
    /// Whether the handler should actually dispatch the plan.
    pub fn proceeds(self) -> bool {
        matches!(self, ApplyDispatch::First | ApplyDispatch::ReApply)
    }

    /// The line printed to the user (uncolored — the caller picks the color).
    pub fn message(self) -> &'static str {
        match self {
            ApplyDispatch::First => "🚀 Applying stored plan…",
            ApplyDispatch::ReApply => {
                "🔁 Re-applying stored plan (already applied once) — /plan clear to discard"
            }
            ApplyDispatch::Cleared => {
                "Plan was cleared this session. Use /plan <task> to create a new one."
            }
            ApplyDispatch::Missing => "No plan stored. Use /plan <task> to create one first.",
        }
    }
}

/// Pure decision for the `/plan apply` dispatch message.
pub fn apply_dispatch_state(
    has_plan: bool,
    already_applied: bool,
    was_cleared: bool,
) -> ApplyDispatch {
    match (has_plan, already_applied, was_cleared) {
        (true, false, _) => ApplyDispatch::First,
        (true, true, _) => ApplyDispatch::ReApply,
        (false, _, true) => ApplyDispatch::Cleared,
        (false, _, false) => ApplyDispatch::Missing,
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
    "on",
    "off",
    "open",
    "close",
    "show",
    "apply",
    "clear",
    "status",
    "step",
    "--deep",
    "--shallow",
];

/// Near-miss typo guard for `/plan`'s free-text argument.
///
/// A single word that isn't a known subcommand but sits within edit distance 2
/// of one (e.g. "aply" → "apply") is almost certainly a typo — launching a
/// full LLM planning run on that literal word burns tokens and can clobber a
/// stored plan flow. Returns the suggested subcommand when the guard should
/// fire. Multi-word arguments (real tasks) and words far from any subcommand
/// are never touched. Candidates derive from `PLAN_SUBCOMMANDS`, never a
/// hand-typed list.
pub fn plan_near_miss(arg: &str) -> Option<&'static str> {
    if arg.is_empty() || arg.contains(char::is_whitespace) {
        return None;
    }
    // Exact subcommands are handled by earlier match arms.
    if PLAN_SUBCOMMANDS.contains(&arg) {
        return None;
    }
    crate::commands::closest_match(arg, PLAN_SUBCOMMANDS, 2)
}

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

/// Strip a leading/trailing `--shallow` flag from a plan task string, returning the
/// cleaned task and whether the flag was present. `--shallow` forces the fast broad
/// pass and suppresses auto-escalation (#583 fourth route). It's the opt-out escape
/// hatch that preserves the old default behavior on a per-invocation basis.
pub fn extract_shallow_flag(task: &str) -> (String, bool) {
    let mut shallow = false;
    let cleaned: Vec<&str> = task
        .split_whitespace()
        .filter(|word| {
            if *word == "--shallow" {
                shallow = true;
                false
            } else {
                true
            }
        })
        .collect();
    (cleaned.join(" "), shallow)
}

/// Conservative, language-agnostic heuristic: does a `/plan` task look non-trivial
/// enough to warrant the deep (RED/GREEN/REFACTOR) planning pass by default? (#583)
///
/// Biased toward `false` — a false shallow is the old behavior (harmless), while a
/// false deep costs tokens and nags. A task is non-trivial if ANY of:
/// - it's "large" by word count (≥ 12 words), OR
/// - it names multiple deliverables (conjunction / enumeration markers), OR
/// - it contains implementation-verb cues (refactor / migrate / implement / integrate /
///   redesign).
pub fn task_looks_nontrivial(task: &str) -> bool {
    let trimmed = task.trim();
    if trimmed.is_empty() {
        return false;
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();

    // Signal 1: large by word count.
    if words.len() >= 12 {
        return true;
    }

    let lower = trimmed.to_lowercase();

    // Signal 2: multiple deliverables (conjunctions / enumeration markers).
    // " and " / " then " join clauses; a comma joining clauses; numbered "1." "2." lists.
    if lower.contains(" and ")
        || lower.contains(" then ")
        || lower.contains(',')
        || lower.contains("1.")
        || lower.contains("2.")
    {
        return true;
    }

    // Signal 3: implementation-verb cues — match whole words to avoid substring noise.
    const IMPL_VERBS: &[&str] = &["refactor", "migrate", "implement", "integrate", "redesign"];
    if words.iter().any(|w| {
        let cleaned = w
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        IMPL_VERBS.contains(&cleaned.as_str())
    }) {
        return true;
    }

    false
}

/// Pure decision helper combining the explicit flags with the auto-escalation
/// heuristic (#583 fourth route). Returns `(deep, auto_escalated)`:
/// - explicit `--shallow` always wins → shallow, never auto-escalated
/// - explicit `--deep` → deep (not counted as auto-escalation)
/// - neither flag → auto-escalate to deep iff `task_looks_nontrivial`
pub fn should_plan_deep(clean_task: &str, deep_flag: bool, shallow_flag: bool) -> (bool, bool) {
    if shallow_flag {
        return (false, false);
    }
    if deep_flag {
        return (true, false);
    }
    if task_looks_nontrivial(clean_task) {
        return (true, true);
    }
    (false, false)
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
///
/// `verify_cmd` is the project-aware verification command (e.g. `cargo test`,
/// `go test ./...`, or a generic fallback for unknown projects) — never hardcoded
/// to a single language, so `/plan apply` gives correct guidance to non-Rust users
/// (issue #448 failure class).
pub fn build_apply_prompt(plan_text: &str, verify_cmd: &str) -> String {
    format!(
        "Execute the following plan. Implement each step, writing code and running tests as you go.\n\n\
         ## Plan\n{plan_text}\n\n\
         Work through each step. After completing all steps, verify with `{verify_cmd}` \
         (or the project's equivalent)."
    )
}

/// Derive the project-aware verify command for `/plan apply`, reusing the same
/// project-detection + test-command helpers as `/test`. Falls back to a generic,
/// language-neutral string when the project type is unrecognized so no wrong tool
/// is ever named (issue #448 class).
fn apply_verify_command() -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let project_type = crate::commands_project::detect_project_type(&cwd);
    match crate::commands_lint::test_command_for_project(&project_type) {
        Some((label, _)) => label.to_string(),
        None => "your project's build and test commands".to_string(),
    }
}

/// Returns true when a generated plan names files to modify but gives no per-file
/// implementation detail (no `Approach:` line and no RED/GREEN/REFACTOR structure).
///
/// Used to surface a "want more depth? try /plan --deep" hint (#583) when the first
/// pass is broad. Pure — no I/O. Language-agnostic: a "file reference" is any line that
/// looks like a path (contains `/` or a `.<ext>` token) or a "Files to modify" section.
/// A plan with no file references returns `false` (there's nothing to deepen — don't nag).
pub fn plan_is_shallow(plan_text: &str) -> bool {
    let trimmed = plan_text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = plan_text.to_lowercase();

    // Depth signals: a per-file Approach line or any RED/GREEN/REFACTOR TDD structure.
    let has_depth = lower.contains("approach:")
        || lower.contains("tdd red")
        || lower.contains("tdd green")
        || lower.contains("tdd refactor");
    if has_depth {
        return false;
    }

    // File references: an explicit "Files to modify" section, or any line that looks
    // like a file path (a `/` separator, or a `word.ext` token with a short extension).
    let has_file_refs =
        lower.contains("files to modify") || plan_text.lines().any(line_has_file_ref);

    // Shallow = names files but has no per-file/per-step implementation detail.
    has_file_refs
}

/// Heuristic: does a single line look like it references a file path?
/// True if it contains a `/` path separator or a `word.ext` token (extension 1–5
/// alphanumerics). Language-agnostic; deliberately conservative to avoid false hits.
fn line_has_file_ref(line: &str) -> bool {
    if line.contains('/') {
        return true;
    }
    // Look for a `.<ext>` where ext is 1–5 alphanumeric chars preceded by a name char.
    let bytes: Vec<char> = line.chars().collect();
    for (i, &c) in bytes.iter().enumerate() {
        if c != '.' {
            continue;
        }
        // Preceding char must be alphanumeric (part of a filename).
        if i == 0 || !bytes[i - 1].is_ascii_alphanumeric() {
            continue;
        }
        // Following run: 1–5 alphanumeric chars, then a non-alphanumeric boundary.
        let mut ext_len = 0;
        let mut j = i + 1;
        while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
            ext_len += 1;
            j += 1;
        }
        if (1..=5).contains(&ext_len) {
            return true;
        }
    }
    false
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

/// The line printed after a planning turn.
///
/// Absence gets its own name (#692). `extract_last_assistant_text` returns `None`
/// when the newest turn produced no assistant text at all — it ended on tool calls,
/// came back empty, or was refused. The old code printed "Review the plan above.
/// Use /plan apply to execute it" unconditionally, while the *previously* captured
/// plan silently stayed in `set_last_plan` — so a user who followed the instruction
/// applied a plan for a different task. We keep the stored plan (the bug is the
/// silence, not the retention) and say out loud which plan `/plan apply` would run.
fn plan_footer(captured: bool) -> String {
    if captured {
        "💡 Review the plan above. Use /plan apply to execute it, or refine it.".to_string()
    } else {
        "⚠️  This turn produced no plan text — nothing new was captured. \
         /plan apply would run the previous plan, not this task. Re-run /plan to try again."
            .to_string()
    }
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
        "apply" => {
            // NOTE: the plan is deliberately NOT cleared here. If the apply run
            // stops partway (provider timeout, model hands back, user types a
            // plain continuation), the plan must still be there to resume from.
            // It lives until `/plan clear` or until a new `/plan <task>`
            // overwrites it.
            let stored = get_last_plan();
            let state = apply_dispatch_state(
                stored.is_some(),
                stored.as_ref().map(|p| p.applied).unwrap_or(false),
                PLAN_WAS_CLEARED.load(Ordering::Relaxed),
            );
            match stored {
                Some(plan) if state.proceeds() => {
                    let prompt = build_apply_prompt(&plan.raw_text, &apply_verify_command());
                    if state == ApplyDispatch::ReApply {
                        println!("{YELLOW}  {}{RESET}\n", state.message());
                    } else {
                        println!("{GREEN}  {}{RESET}\n", state.message());
                    }
                    mark_plan_applied();
                    return PlanResult::Apply(prompt);
                }
                _ => {
                    println!("{DIM}  {}{RESET}\n", state.message());
                    return PlanResult::Handled;
                }
            }
        }
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

    // Near-miss typo guard: a single word close to a known subcommand is
    // almost certainly a typo — don't launch a full planning run on it.
    if let Some(suggestion) = plan_near_miss(arg) {
        println!(
            "{YELLOW}  Unknown subcommand '{arg}' — did you mean '{suggestion}'? Use /plan \"{arg}\" if you meant it as a task.{RESET}\n"
        );
        return PlanResult::Handled;
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
    let (clean_task, shallow) = extract_shallow_flag(&clean_task);
    let (deep, auto_escalated) = should_plan_deep(&clean_task, deep, shallow);
    if auto_escalated {
        println!(
            "{DIM}  (task looks non-trivial — planning at depth; use /plan --shallow to skip){RESET}"
        );
    }
    if deep {
        println!("{DIM}  🔬 Deep mode — requesting per-step TDD (RED/GREEN/REFACTOR) structure.{RESET}\n");
    }

    let plan_prompt = build_plan_prompt(&clean_task, deep);
    run_prompt(agent, &plan_prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Capture the plan text from the last assistant message for later retrieval
    let plan_text = crate::commands_web::extract_last_assistant_text(agent.messages());
    let captured = plan_text.is_some();
    if let Some(plan_text) = plan_text {
        // If the first pass is broad (names files but has no per-file/per-step detail)
        // and the user didn't already ask for depth, point them at `/plan --deep` (#583).
        if !deep && plan_is_shallow(&plan_text) {
            println!(
                "{DIM}  (this plan is broad — run `/plan --deep {clean_task}` for per-step TDD implementation detail){RESET}"
            );
        }
        set_last_plan(plan_text);
    }

    println!("\n{DIM}  {}{RESET}\n", plan_footer(captured));

    PlanResult::PlanGenerated(plan_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn plan_footer_tells_the_truth_about_both_branches() {
        // (captured, must_contain, must_not_be_described_as_a_fresh_plan)
        let captured = plan_footer(true);
        assert!(
            captured.contains("/plan apply to execute it"),
            "captured branch must keep the original invitation byte-for-byte: {captured}"
        );
        assert!(
            captured.contains("Review the plan above"),
            "captured branch wording changed unexpectedly: {captured}"
        );

        let uncaptured = plan_footer(false);
        assert!(
            !uncaptured.contains("Review the plan above"),
            "an empty turn must not invite the user to review a plan that isn't there: {uncaptured}"
        );
        assert!(
            uncaptured.to_lowercase().contains("no plan"),
            "uncaptured branch must say no plan text was captured: {uncaptured}"
        );
        assert!(
            uncaptured.to_lowercase().contains("previous"),
            "uncaptured branch must warn that /plan apply would run the PREVIOUS plan: {uncaptured}"
        );
        assert!(
            uncaptured.contains("/plan apply"),
            "uncaptured branch must name the command whose meaning it is correcting: {uncaptured}"
        );
    }

    #[test]
    fn build_apply_prompt_contains_verify_cmd() {
        let prompt = build_apply_prompt("- step one", "go build ./... && go test ./...");
        assert!(
            prompt.contains("go build ./... && go test ./..."),
            "prompt should contain the project-aware verify command"
        );
        assert!(
            !prompt.contains("cargo"),
            "prompt must not name a Rust-only tool for a Go project"
        );
    }

    #[test]
    fn build_apply_prompt_contains_plan_text() {
        let prompt = build_apply_prompt("- implement the widget\n- add tests", "cargo test");
        assert!(
            prompt.contains("- implement the widget"),
            "plan text must be embedded in the apply prompt"
        );
        assert!(
            prompt.contains("- add tests"),
            "full plan text must be embedded in the apply prompt"
        );
    }

    #[test]
    fn build_apply_prompt_generic_fallback() {
        let fallback = "your project's build and test commands";
        let prompt = build_apply_prompt("- do the thing", fallback);
        assert!(
            prompt.contains(fallback),
            "prompt should contain the generic fallback string"
        );
        assert!(
            !prompt.contains("cargo"),
            "generic fallback must not name a Rust-only tool"
        );
        assert!(
            !prompt.contains("npm") && !prompt.contains("go test"),
            "generic fallback must not name any language-specific tool"
        );
    }

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
    fn extract_shallow_flag_detects_and_strips() {
        let (task, shallow) = extract_shallow_flag("add auth --shallow");
        assert!(shallow, "Should detect --shallow flag");
        assert_eq!(task, "add auth", "Should strip --shallow from the task");

        let (task, shallow) = extract_shallow_flag("do the thing");
        assert!(!shallow, "No flag → shallow=false");
        assert_eq!(task, "do the thing");
    }

    #[test]
    fn task_looks_nontrivial_short_single_clause_is_trivial() {
        assert!(!task_looks_nontrivial("fix typo in README"));
        assert!(!task_looks_nontrivial("rename foo to bar"));
        assert!(!task_looks_nontrivial("widget")); // bare single word
        assert!(!task_looks_nontrivial("")); // empty
        assert!(!task_looks_nontrivial("   ")); // whitespace only
    }

    #[test]
    fn task_looks_nontrivial_verbs_and_conjunctions_and_length() {
        // verbs + conjunctions + length
        assert!(task_looks_nontrivial(
            "refactor the risk subsystem and add a new accuracy report then wire it into /status"
        ));
        // implementation-verb cue alone (short)
        assert!(task_looks_nontrivial(
            "implement LSP-backed go-to-definition across all supported languages"
        ));
    }

    #[test]
    fn task_looks_nontrivial_word_count_signal() {
        // ≥ 12 words, no verbs/conjunctions.
        assert!(task_looks_nontrivial(
            "one two three four five six seven eight nine ten eleven twelve"
        ));
        // 11 words, no other signal → trivial.
        assert!(!task_looks_nontrivial(
            "one two three four five six seven eight nine ten eleven"
        ));
    }

    #[test]
    fn task_looks_nontrivial_conjunction_signals() {
        assert!(task_looks_nontrivial("add a button and a form"));
        assert!(task_looks_nontrivial("build the parser then run it"));
        assert!(task_looks_nontrivial("add logging, metrics"));
        assert!(task_looks_nontrivial("do step 1. and step 2."));
    }

    #[test]
    fn task_looks_nontrivial_no_substring_false_positive() {
        // "reimplemented" should NOT match the whole-word "implement" cue
        // as a standalone verb — but a short trivial phrase must stay shallow.
        assert!(!task_looks_nontrivial("clean up whitespace"));
    }

    #[test]
    fn should_plan_deep_shallow_flag_wins() {
        // Explicit --shallow must NOT auto-escalate even for a non-trivial task.
        let nontrivial = "refactor the risk subsystem and add a report then wire it in";
        assert!(task_looks_nontrivial(nontrivial));
        let (deep, auto) = should_plan_deep(nontrivial, false, true);
        assert!(!deep, "shallow flag forces shallow");
        assert!(!auto, "shallow flag never counts as auto-escalation");
    }

    #[test]
    fn should_plan_deep_explicit_deep_not_auto() {
        let (deep, auto) = should_plan_deep("fix typo", true, false);
        assert!(deep, "explicit --deep forces deep");
        assert!(!auto, "explicit deep is not auto-escalation");
    }

    #[test]
    fn should_plan_deep_auto_escalates_nontrivial() {
        let (deep, auto) = should_plan_deep("implement a new caching layer", false, false);
        assert!(deep, "non-trivial task auto-escalates to deep");
        assert!(auto, "and it's reported as auto-escalation");
    }

    #[test]
    fn should_plan_deep_stays_shallow_for_trivial() {
        let (deep, auto) = should_plan_deep("fix typo in README", false, false);
        assert!(!deep, "trivial task stays shallow");
        assert!(!auto);
    }

    #[test]
    fn should_plan_deep_shallow_wins_over_deep() {
        // If both flags somehow present, shallow wins (checked first).
        let (deep, auto) = should_plan_deep("implement a big feature here now", true, true);
        assert!(!deep, "shallow wins when both flags given");
        assert!(!auto);
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
    #[serial]
    fn test_plan_mode_toggle() {
        // Plan mode is a process global also exercised by the tool-layer
        // guard tests in tool_wrappers.rs — #[serial] prevents races.
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
    fn test_plan_near_miss_catches_typos() {
        // A typo'd subcommand must not launch a full planning run on the
        // literal typo (Day-142 bug class: wildcard fallbacks doing something).
        assert_eq!(plan_near_miss("aply"), Some("apply"));
        assert_eq!(plan_near_miss("statsu"), Some("status"));
        assert_eq!(plan_near_miss("shwo"), Some("show"));
        assert_eq!(plan_near_miss("cleer"), Some("clear"));
    }

    #[test]
    fn test_plan_near_miss_leaves_real_tasks_alone() {
        // One-word tasks not near any subcommand are legitimate.
        assert_eq!(plan_near_miss("refactor"), None);
        // Multi-word args are real tasks — never intercepted.
        assert_eq!(plan_near_miss("fix the login bug"), None);
        assert_eq!(plan_near_miss("apply the migration"), None);
        // Empty arg is handled by the usage arm, not the guard.
        assert_eq!(plan_near_miss(""), None);
    }

    #[test]
    fn test_plan_near_miss_ignores_exact_subcommands() {
        // Exact subcommands match earlier arms and must never be flagged —
        // derive from the authoritative constant, not a hand-typed list.
        for sub in PLAN_SUBCOMMANDS {
            assert_eq!(plan_near_miss(sub), None, "exact '{sub}' flagged");
        }
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
    #[serial]
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

    // -----------------------------------------------------------------
    // /plan apply is resumable: the plan is not consumed at dispatch (#630)
    // -----------------------------------------------------------------

    #[test]
    fn apply_dispatch_first_apply_is_not_a_reapply() {
        let state = apply_dispatch_state(true, false, false);
        assert_eq!(state, ApplyDispatch::First);
        assert!(state.proceeds());
        assert!(state.message().contains("Applying stored plan"));
        // Near-miss: the very first apply must NOT claim a repeat.
        assert!(!state.message().contains("Re-applying"));
        assert!(!state.message().contains("already applied"));
    }

    #[test]
    fn apply_dispatch_second_apply_says_reapply_and_still_proceeds() {
        let state = apply_dispatch_state(true, true, false);
        assert_eq!(state, ApplyDispatch::ReApply);
        // Repeats are made visible, not forbidden.
        assert!(state.proceeds());
        assert!(state.message().contains("Re-applying"));
        assert!(state.message().contains("/plan clear"));
    }

    #[test]
    fn apply_dispatch_never_stored_says_no_plan() {
        let state = apply_dispatch_state(false, false, false);
        assert_eq!(state, ApplyDispatch::Missing);
        assert!(!state.proceeds());
        assert!(state.message().contains("No plan stored"));
    }

    #[test]
    fn apply_dispatch_cleared_is_distinguishable_from_never_stored() {
        let cleared = apply_dispatch_state(false, false, true);
        assert_eq!(cleared, ApplyDispatch::Cleared);
        assert!(!cleared.proceeds());
        assert!(cleared.message().contains("cleared this session"));
        // "consumed" and "never existed" must not collapse into one message.
        assert_ne!(
            cleared.message(),
            apply_dispatch_state(false, false, false).message()
        );
    }

    #[test]
    #[serial]
    fn stored_plan_survives_being_applied() {
        clear_last_plan();
        set_last_plan("1. Step one\n2. Step two".to_string());

        // First dispatch: not previously applied, plan still there afterwards.
        assert!(!mark_plan_applied());
        let plan = get_last_plan().expect("plan must survive dispatch");
        assert!(plan.applied);
        assert_eq!(plan.steps.len(), 2);

        // Step tracking still works after apply — the point of not consuming.
        assert!(mark_step(1, true).is_ok());
        assert!(get_last_plan().unwrap().steps[0].completed);

        // Second dispatch reports the repeat.
        assert!(mark_plan_applied());
        assert!(get_last_plan().is_some());

        clear_last_plan();
    }

    #[test]
    #[serial]
    fn new_plan_resets_applied_and_cleared_flags() {
        clear_last_plan();
        set_last_plan("1. Old".to_string());
        assert!(!mark_plan_applied());
        assert!(get_last_plan().unwrap().applied);

        set_last_plan("1. New".to_string());
        let plan = get_last_plan().unwrap();
        assert!(!plan.applied, "a fresh plan starts unapplied");
        assert!(!PLAN_WAS_CLEARED.load(Ordering::Relaxed));

        clear_last_plan();
    }

    #[test]
    fn test_format_plan_status_display() {
        let plan = StructuredPlan {
            raw_text: String::new(),
            applied: false,
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
    #[serial]
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
    fn plan_is_shallow_true_for_file_list_without_approach() {
        let plan = "\
## Files to modify
- src/parser.rs
- src/lexer.rs

## Step-by-step
1. Add the new token
2. Wire it into the parser";
        assert!(
            plan_is_shallow(plan),
            "A file list with no Approach:/TDD detail should be shallow"
        );
    }

    #[test]
    fn plan_is_shallow_false_when_approach_present() {
        let plan = "\
## Files to modify
- src/parser.rs
  Approach: add a match arm for the new token, then re-run the parser tests
- src/lexer.rs
  Approach: emit the new token kind from the scanner";
        assert!(
            !plan_is_shallow(plan),
            "A plan with per-file Approach: lines is not shallow"
        );
    }

    #[test]
    fn plan_is_shallow_false_when_deep_structure_present() {
        let plan = "\
## Files to modify
- src/parser.rs

## Step 1
TDD RED: write a failing test for the new token
TDD GREEN: implement the match arm
TDD REFACTOR: none";
        assert!(
            !plan_is_shallow(plan),
            "A plan with TDD RED/GREEN structure is not shallow"
        );
    }

    #[test]
    fn plan_is_shallow_false_for_empty() {
        assert!(!plan_is_shallow(""));
        assert!(!plan_is_shallow("   \n\n  "));
    }

    #[test]
    fn plan_is_shallow_false_when_no_file_references() {
        let plan = "\
This plan describes the general approach in prose. We will improve the login flow
by validating inputs earlier and returning clearer errors. No specific files named.";
        assert!(
            !plan_is_shallow(plan),
            "Prose with no file references has nothing to deepen — don't nag"
        );
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
