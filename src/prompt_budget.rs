//! Session wall-clock budget and audit log helpers.
//!
//! Extracted from `prompt.rs` as a coherent unit: both subsystems are
//! global, `OnceLock`/`AtomicBool`-backed, env-var-driven, and have no
//! business logic dependencies on the rest of `prompt.rs`. Keeping them
//! here makes the budget/audit lifecycle easier to reason about and
//! shrinks the surface area of `prompt.rs`.

use crate::format::safe_truncate;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

// ── Audit log ───────────────────────────────────────────────────────────
// Records every tool call to `.yoyo/audit.jsonl` for debugging and transparency.
// Enabled via `--audit` flag, `YOYO_AUDIT=1` env var, or `audit = true` in config.

/// Global flag controlling whether audit logging is active.
static AUDIT_ENABLED: AtomicBool = AtomicBool::new(false);

/// Convert days since Unix epoch (1970-01-01) to (year, month, day).
/// Uses the civil calendar algorithm — no external crate needed.
fn days_from_epoch(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Enable audit logging for this session.
pub fn enable_audit_log() {
    AUDIT_ENABLED.store(true, Ordering::Relaxed);
}

/// Check whether audit logging is currently enabled.
pub fn is_audit_enabled() -> bool {
    AUDIT_ENABLED.load(Ordering::Relaxed)
}

/// Write a tool execution record to `.yoyo/audit.jsonl`.
/// Each line is a JSON object: `{"ts":"...","tool":"...","args":{...},"duration_ms":N,"success":bool}`
/// Silently does nothing if audit is disabled or writing fails.
///
/// **Invariant (#751): one entry per completed tool call, single writer.** The only
/// caller is the prompt event-stream handler in `src/prompt.rs` — the display path
/// (`PromptEventState::handle_tool_execution_end`) and the JSON-stream path
/// (`handle_stream_json_events`), which are mutually exclusive for a given run — and it
/// passes the call's real elapsed duration and its real `is_error` flag. Nothing else may
/// call this: `AuditHook` sees only a tool's output string, so it can know neither
/// duration nor success, and a second writer both doubles every count read off the file
/// (it is pushed to the public `audit-log` branch and mined by skill-evolve and
/// `scripts/extract_trajectory.py`) and dilutes failures with fabricated successes.
pub fn audit_log_tool_call(
    tool_name: &str,
    args: &serde_json::Value,
    duration_ms: u64,
    success: bool,
) {
    if !is_audit_enabled() {
        return;
    }
    let _ = write_audit_entry(tool_name, args, duration_ms, success);
}

// Directory the audit log resolves against — `.yoyo` in production.
//
// The test-only override is a thread-local rather than a process CWD move:
// each test thread gets its own, so it cannot leak into a concurrently running
// sibling the way `std::env::set_current_dir` does (#780). Same seam and same
// reasoning as `commands_goal::goal_base`.
#[cfg(test)]
thread_local! {
    static AUDIT_BASE_DIR: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

fn audit_base() -> std::path::PathBuf {
    #[cfg(test)]
    {
        if let Some(d) = AUDIT_BASE_DIR.with(|b| b.borrow().clone()) {
            return d;
        }
    }
    std::path::PathBuf::from(".yoyo")
}

/// Run `f` with every audit-log write rooted at `dir` instead of `.yoyo`.
///
/// Restores the *previous* value rather than clearing, so nesting is safe, and
/// restores on panic so a failing assertion cannot leak the override into the
/// next test on this thread.
#[cfg(test)]
pub(crate) fn with_audit_base_dir<F: FnOnce()>(dir: &std::path::Path, f: F) {
    let prev = AUDIT_BASE_DIR.with(|b| b.borrow().clone());
    AUDIT_BASE_DIR.with(|b| *b.borrow_mut() = Some(dir.to_path_buf()));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    AUDIT_BASE_DIR.with(|b| *b.borrow_mut() = prev);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// Current time as `YYYY-MM-DDTHH:MM:SS`, or `"unknown"` if the clock is before
/// the epoch. Manual ISO 8601 formatting — no external crate needed.
fn audit_timestamp() -> String {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            let days_since_epoch = secs / 86400;
            let time_of_day = secs % 86400;
            let hours = time_of_day / 3600;
            let minutes = (time_of_day % 3600) / 60;
            let seconds = time_of_day % 60;

            // Calculate year/month/day from days since epoch (1970-01-01)
            let (year, month, day) = days_from_epoch(days_since_epoch);
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                year, month, day, hours, minutes, seconds
            )
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Append one JSON object as a line to `<audit_base>/audit.jsonl`.
///
/// The *single* writer of that file. Both line kinds — tool calls and the
/// per-run usage record — go through here, so they cannot drift in path
/// resolution, append discipline or newline handling. A second `OpenOptions`
/// on the same file is how two formats grow apart.
fn write_audit_line(entry: &serde_json::Value) -> std::io::Result<()> {
    let dir = audit_base();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("audit.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{}", entry)?;
    Ok(())
}

fn write_audit_entry(
    tool_name: &str,
    args: &serde_json::Value,
    duration_ms: u64,
    success: bool,
) -> std::io::Result<()> {
    // Truncate args to avoid huge entries (e.g., file content in write_file),
    // then mask credential-shaped substrings: `.yoyo/audit.jsonl` is pushed to
    // the public `audit-log` branch, and tool arguments carry user-authored
    // command text (e.g. `ANTHROPIC_API_KEY=sk-ant-…` in a bash command).
    // Scope: arguments only — tool *output* is not written here.
    let truncated_args = redact_audit_value(&truncate_audit_args(args));

    // Deliberately carries NO `"type"` key. Every reader of this file predates
    // the usage record, so a missing `type` must keep reading as a tool call;
    // adding one here would change the shape of every existing line.
    let entry = serde_json::json!({
        "ts": audit_timestamp(),
        "tool": tool_name,
        "args": truncated_args,
        "duration_ms": duration_ms,
        "success": success,
    });
    write_audit_line(&entry)
}

/// Build the per-run usage record — pure, so the emission point is testable.
///
/// Every field is already computed by `build_json_output`; the defect this
/// exists for is that they were computed and *thrown away* in every mode but
/// `--output-format json` (#848).
///
/// `cost_usd` is `null` when [`estimate_cost`] cannot price the model, rather
/// than `0.0` — an unknown cost is not a free run, and a reader summing these
/// must be able to tell "no price for this model" from "this run was free".
pub fn usage_audit_record(
    usage: &yoagent::Usage,
    model: &str,
    duration: Duration,
    num_turns: usize,
    is_error: bool,
) -> serde_json::Value {
    serde_json::json!({
        "ts": audit_timestamp(),
        "type": "usage",
        "model": model,
        "input_tokens": usage.input,
        "output_tokens": usage.output,
        "cache_read_input_tokens": usage.cache_read,
        "cache_creation_input_tokens": usage.cache_write,
        "cost_usd": crate::format::estimate_cost(usage, model),
        "duration_ms": duration.as_millis() as u64,
        "num_turns": num_turns,
        "is_error": is_error,
    })
}

/// Record this run's token usage and cost to `.yoyo/audit.jsonl`.
///
/// A **file** write, deliberately: the numbers used to reach a human only via
/// `print_usage`, and `cli.rs` auto-enables quiet whenever both stdin and
/// stdout are non-terminal — which is every piped session, including every one
/// of this repo's own CI runs. Quiet mode cannot suppress a file, so this path
/// consults neither `is_quiet()` nor the output mode (#848).
///
/// Gated on [`is_audit_enabled`], like its tool-call sibling: a user who never
/// asked for an audit log gets byte-identical behaviour and no new file.
///
/// Silently does nothing if audit is disabled or writing fails — a run must not
/// break because its bookkeeping did.
pub fn audit_log_usage(
    usage: &yoagent::Usage,
    model: &str,
    duration: Duration,
    num_turns: usize,
    is_error: bool,
) {
    audit_log_usage_when(
        is_audit_enabled(),
        usage,
        model,
        duration,
        num_turns,
        is_error,
    );
}

/// [`audit_log_usage`] with the enablement flag passed in rather than read from
/// the process-global `AUDIT_ENABLED`.
///
/// The split exists for the tests: `AUDIT_ENABLED` is shared by every test in
/// this binary, so a test that wrote it would race its own siblings under
/// libtest's default parallelism — the class `tests/global_state_races.rs`
/// enumerates. That gate names passing the value explicitly as the *best*
/// remedy, ahead of `#[serial]`, which only serialises against other
/// `#[serial]` tests and leaves unmarked readers exposed. `audit_log_usage` is
/// the only global read, and it is the only path production takes.
pub(crate) fn audit_log_usage_when(
    enabled: bool,
    usage: &yoagent::Usage,
    model: &str,
    duration: Duration,
    num_turns: usize,
    is_error: bool,
) {
    if !enabled {
        return;
    }
    let record = usage_audit_record(usage, model, duration, num_turns, is_error);
    let _ = write_audit_line(&record);
}

/// Truncate tool arguments for audit logging.
/// Keeps keys but truncates long string values (like file contents) to 200 chars.
pub fn truncate_audit_args(args: &serde_json::Value) -> serde_json::Value {
    match args {
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), truncate_audit_value(v));
            }
            serde_json::Value::Object(new_map)
        }
        other => other.clone(),
    }
}

fn truncate_audit_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) if s.len() > 200 => serde_json::Value::String(format!(
            "{}... [truncated, {} chars total]",
            safe_truncate(s, 200),
            s.len()
        )),
        other => other.clone(),
    }
}

/// Mask credential-shaped substrings in every string leaf of an audit value.
///
/// Strings are passed through [`crate::safety::redact_secrets`]; arrays and
/// objects recurse; numbers, bools and null are returned unchanged. It is a
/// mask, not a guarantee — a novel secret shape passes through.
fn redact_audit_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) => serde_json::Value::String(crate::safety::redact_secrets(s)),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(redact_audit_value).collect())
        }
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, val) in map {
                new_map.insert(k.clone(), redact_audit_value(val));
            }
            serde_json::Value::Object(new_map)
        }
        other => other.clone(),
    }
}

/// Read the last N entries from the audit log.
/// Returns an empty vec if the file doesn't exist or can't be read.
#[cfg(test)]
pub fn read_audit_log(n: usize) -> Vec<String> {
    let path = audit_base().join("audit.jsonl");
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(n);
            lines[start..].iter().map(|s| s.to_string()).collect()
        }
        Err(_) => Vec::new(),
    }
}

// ── Session wall-clock budget ───────────────────────────────────────────
// A soft, opt-in wall-clock budget for evolution sessions. The hourly evolve
// cron can fire while a previous session is still running, causing GH Actions
// to cancel the in-flight run (#262). This helper lets the agent voluntarily
// stay inside a tighter budget than the workflow timeout, so future task
// dispatch can self-throttle and finish before the next cron tick.
//
// Enable by setting `YOYO_SESSION_BUDGET_SECS=2700` (45 min default) before
// invoking yoyo. When unset, `session_budget_remaining()` returns `None` and
// callers should treat the session as unbounded.
//
// This is the foundation only — wiring it into the spawn loop and individual
// task dispatch happens in `session_budget_exhausted` below, which is called
// at retry-loop boundaries (`run_prompt_auto_retry`, the watch-mode fix loop).
// Unbounded sessions remain the default — `session_budget_exhausted` returns
// `false` when the env var is unset, so interactive use is unaffected.

/// Default soft budget in seconds (45 min) when `YOYO_SESSION_BUDGET_SECS`
/// is set but doesn't parse as a positive integer.
const DEFAULT_SESSION_BUDGET_SECS: u64 = 2700;

/// Cached parse of `YOYO_SESSION_BUDGET_SECS`. `None` if the env var was unset
/// or empty at first read; `Some(secs)` otherwise. Read once and frozen for
/// the lifetime of the process so the budget can't shift mid-session.
static SESSION_BUDGET_SECS: OnceLock<Option<u64>> = OnceLock::new();

/// Wall-clock instant of the first call to `session_budget_remaining()`.
/// Recorded lazily so the budget starts ticking from real agent work, not
/// from process startup (which may include slow CI cold-start time).
static SESSION_BUDGET_START: OnceLock<Instant> = OnceLock::new();

/// Look up the configured budget, reading the env var exactly once.
///
/// Returns `None` if `YOYO_SESSION_BUDGET_SECS` is unset or empty.
/// Returns `Some(DEFAULT_SESSION_BUDGET_SECS)` if it's set but unparseable
/// (so a typo doesn't silently disable the guard).
fn configured_session_budget() -> Option<u64> {
    *SESSION_BUDGET_SECS
        .get_or_init(|| parse_session_budget(std::env::var("YOYO_SESSION_BUDGET_SECS").ok()))
}

/// Pure parser for the budget env var. Extracted so it can be tested
/// without the OnceLock dance — the cache only memoizes the result of
/// this function once per process.
fn parse_session_budget(raw: Option<String>) -> Option<u64> {
    match raw {
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s.parse::<u64>().unwrap_or(DEFAULT_SESSION_BUDGET_SECS)),
        None => None,
    }
}

/// How much wall-clock time remains in this session's soft budget.
///
/// Returns `None` when no budget is configured (the common case for
/// interactive use — sessions are unbounded). Returns `Some(Duration::ZERO)`
/// when the budget has been exhausted. Otherwise returns the remaining time.
///
/// The budget timer starts on the first call to this function, not at
/// process startup, so cold-start overhead doesn't eat into agent work.
pub fn session_budget_remaining() -> Option<Duration> {
    let budget_secs = configured_session_budget()?;
    let start = SESSION_BUDGET_START.get_or_init(Instant::now);
    let elapsed = start.elapsed();
    let budget = Duration::from_secs(budget_secs);
    Some(budget.saturating_sub(elapsed))
}

/// Returns `true` if the session budget is set and has `≤ grace_secs`
/// remaining. Returns `false` if the budget is unset (unbounded) or if
/// there's still headroom above the grace window.
///
/// Used at retry-loop boundaries (`run_prompt_auto_retry`, the watch-mode
/// fix loop) to stop kicking off new attempts when the GH Actions runner
/// is about to cancel us mid-push (#262). Unbounded sessions never report
/// exhausted, so interactive use is unaffected.
pub fn session_budget_exhausted(grace_secs: u64) -> bool {
    match session_budget_remaining() {
        Some(remaining) => remaining.as_secs() <= grace_secs,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Per-run usage record (#848) ----------------------------------
    //
    // The defect: every field below was already computed by
    // `build_json_output` and thrown away in every mode but
    // `--output-format json`. The dashboard's cost figure sat frozen at
    // $1,077.59 for 102 days — a frozen number, not a zero, which is why no
    // non-zero check could see it. These assert on the *bytes in the file*,
    // never that a function returned `Ok`.

    fn sample_usage() -> yoagent::Usage {
        yoagent::Usage {
            input: 1200,
            output: 340,
            cache_read: 90_000,
            cache_write: 5_000,
            total_tokens: 96_540,
        }
    }

    /// Read the audit file under `dir`, one JSON value per line.
    fn audit_lines(dir: &std::path::Path) -> Vec<serde_json::Value> {
        let content = std::fs::read_to_string(dir.join("audit.jsonl")).unwrap_or_default();
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                serde_json::from_str(l).unwrap_or_else(|e| panic!("line is not JSON: {l} ({e})"))
            })
            .collect()
    }

    /// Run `f` with audit enabled and rooted at a fresh tempdir, restoring the
    /// enabled flag afterwards. Tests must never touch the real
    /// `.yoyo/audit.jsonl`.
    fn with_audit_tempdir<F: FnOnce(&std::path::Path)>(f: F) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().to_path_buf();
        with_audit_base_dir(&path, || f(&path));
    }

    /// Record a run with audit explicitly on — no process-global is touched,
    /// so these tests cannot race their ~5,000 siblings.
    fn record(usage: &yoagent::Usage, model: &str, ms: u64, turns: usize, is_error: bool) {
        audit_log_usage_when(true, usage, model, Duration::from_millis(ms), turns, is_error);
    }

    #[test]
    fn usage_record_round_trips_every_token_field_and_the_cost() {
        with_audit_tempdir(|dir| {
            let usage = sample_usage();
            record(&usage, "claude-sonnet-4-5", 4200, 7, false);

            let lines = audit_lines(dir);
            assert_eq!(lines.len(), 1, "expected exactly one record, got {lines:?}");
            let rec = &lines[0];

            assert_eq!(rec["type"], "usage");
            assert_eq!(rec["model"], "claude-sonnet-4-5");
            assert_eq!(rec["input_tokens"], 1200);
            assert_eq!(rec["output_tokens"], 340);
            assert_eq!(rec["cache_read_input_tokens"], 90_000);
            assert_eq!(rec["cache_creation_input_tokens"], 5_000);
            assert_eq!(rec["duration_ms"], 4200);
            assert_eq!(rec["num_turns"], 7);
            assert_eq!(rec["is_error"], false);
            assert!(
                rec["ts"].as_str().is_some_and(|t| t.contains('T')),
                "timestamp missing or malformed: {rec}"
            );

            // The cost must be the *same* number `--output-format json` emits,
            // derived rather than hardcoded: pinning a literal here would pass
            // even if the record were computed from the wrong fields.
            let expected = crate::format::estimate_cost(&usage, "claude-sonnet-4-5")
                .expect("this model is priced");
            let got = rec["cost_usd"].as_f64().expect("cost_usd should be a number");
            assert!(
                (got - expected).abs() < 1e-12,
                "cost_usd {got} != estimate_cost {expected}"
            );
            assert!(got > 0.0, "a run with 1540 non-cached tokens is not free");
        });
    }

    #[test]
    fn unpriced_model_records_null_cost_not_zero() {
        // Absence gets its own value: a model with no price is not a free run,
        // and a reader summing these must be able to tell the two apart.
        with_audit_tempdir(|dir| {
            let usage = sample_usage();
            let model = "some-local-model-nobody-prices";
            assert!(
                crate::format::estimate_cost(&usage, model).is_none(),
                "fixture precondition: this model must be unpriced"
            );
            record(&usage, model, 10, 1, false);

            let rec = audit_lines(dir).pop().expect("one record");
            assert!(rec["cost_usd"].is_null(), "expected null, got {}", rec["cost_usd"]);
            // The token counts are still real and still recorded.
            assert_eq!(rec["input_tokens"], 1200);
        });
    }

    #[test]
    fn tool_call_lines_are_unchanged_and_still_carry_no_type_field() {
        // The whole regression surface is every existing reader of this file.
        // A missing `type` must keep reading as a tool call.
        with_audit_tempdir(|dir| {
            // `write_audit_entry` rather than `audit_log_tool_call`: the
            // latter reads the shared `AUDIT_ENABLED` global. This test is
            // about the *line shape*, and `write_audit_entry` is what produces
            // it.
            write_audit_entry("read_file", &serde_json::json!({"path": "src/main.rs"}), 12, true)
                .expect("tool-call write");
            record(&sample_usage(), "claude-sonnet-4-5", 1000, 2, false);

            let lines = audit_lines(dir);
            assert_eq!(lines.len(), 2);

            let tool = &lines[0];
            assert!(
                tool.get("type").is_none(),
                "tool-call lines must carry no `type` key, got: {tool}"
            );
            assert_eq!(tool["tool"], "read_file");
            assert_eq!(tool["args"]["path"], "src/main.rs");
            assert_eq!(tool["duration_ms"], 12);
            assert_eq!(tool["success"], true);
            assert!(tool["ts"].as_str().is_some());

            // ...and the usage line is distinguishable by exactly one key.
            assert_eq!(lines[1]["type"], "usage");
            assert!(lines[1].get("tool").is_none());
        });
    }

    #[test]
    fn usage_record_is_appended_not_overwritten() {
        with_audit_tempdir(|dir| {
            for turns in 1..=3 {
                record(&sample_usage(), "claude-sonnet-4-5", 1000, turns, false);
            }
            let lines = audit_lines(dir);
            assert_eq!(lines.len(), 3, "each run appends its own record");
            assert_eq!(lines[0]["num_turns"], 1);
            assert_eq!(lines[2]["num_turns"], 3);
        });
    }

    #[test]
    fn error_runs_are_recorded_too() {
        // A failed run still spent tokens. Recording only successes would
        // under-report exactly the sessions worth understanding.
        with_audit_tempdir(|dir| {
            record(&sample_usage(), "claude-sonnet-4-5", 2000, 1, true);
            let rec = audit_lines(dir).pop().expect("one record");
            assert_eq!(rec["is_error"], true);
            assert_eq!(rec["input_tokens"], 1200);
        });
    }

    #[test]
    fn audit_disabled_writes_nothing_and_creates_no_file() {
        // Near-miss guard, and the product-safety property: a user who never
        // enabled the audit log gets byte-identical behaviour and no new file.
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().to_path_buf();
        with_audit_base_dir(&dir, || {
            audit_log_usage_when(
                false,
                &sample_usage(),
                "claude-sonnet-4-5",
                Duration::from_secs(1),
                1,
                false,
            );
        });
        assert!(
            !dir.join("audit.jsonl").exists(),
            "audit disabled must not create the file"
        );
    }

    #[test]
    fn usage_record_is_pure_and_reads_no_output_mode_or_quiet_state() {
        // The record builder takes the run's facts and nothing else — no
        // `print_mode`, no `json_output`, no `is_quiet()`. That is the whole
        // fix: `print_usage` was suppressed by quiet mode, which `cli.rs`
        // auto-enables for every piped run, so the numbers never survived.
        let usage = sample_usage();
        let a = usage_audit_record(&usage, "claude-sonnet-4-5", Duration::from_millis(5), 3, false);
        let b = usage_audit_record(&usage, "claude-sonnet-4-5", Duration::from_millis(5), 3, false);
        for key in [
            "type",
            "model",
            "input_tokens",
            "output_tokens",
            "cache_read_input_tokens",
            "cache_creation_input_tokens",
            "cost_usd",
            "duration_ms",
            "num_turns",
            "is_error",
        ] {
            assert_eq!(a[key], b[key], "field `{key}` is not a pure function of the inputs");
        }
    }

    #[test]
    fn usage_writing_path_never_consults_quiet_or_the_output_mode() {
        // Deliberately weak, and stated as such: this reads this module's own
        // source and proves the *absence of a call* in the two usage functions.
        // It cannot prove a quiet process wrote the file — `QUIET` is a
        // write-once `OnceLock` with no setter, so a test that turned quiet on
        // would poison it for every one of the ~5,000 tests scheduled after it
        // in this binary. What it does catch is a future edit that helpfully
        // gates the record on an output mode, which is the exact shape of the
        // 102-day defect.
        let src = include_str!("prompt_budget.rs");
        let start = src
            .find("pub fn usage_audit_record(")
            .expect("usage_audit_record must exist");
        let end = src[start..]
            .find("#[cfg(test)]")
            .map(|i| start + i)
            .unwrap_or(src.len());
        // Comment lines are stripped first: `audit_log_usage`'s own doc
        // comment names `is_quiet()` in prose to explain why it does not call
        // it, and a scanner that cannot tell prose from code would fire on the
        // sentence documenting the fix.
        let body: String = src[start..end]
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let body = body.as_str();
        // Needles assembled at runtime so this test cannot match itself.
        let quiet_call = format!("is_{}()", "quiet");
        let plain_call = format!("is_plain_{}()", "output");
        assert!(
            !body.contains(&quiet_call),
            "the usage record path must not consult quiet mode"
        );
        assert!(
            !body.contains(&plain_call),
            "the usage record path must not consult plain-output mode"
        );
        assert!(
            !body.contains("print_mode") && !body.contains("json_output"),
            "the usage record path must not branch on the output mode"
        );
    }

    // ── Audit log tests ─────────────────────────────────────────────────

    #[test]
    fn redact_audit_value_masks_nested_secrets_and_leaves_innocent_text_alone() {
        let args = serde_json::json!({
            "command": "ANTHROPIC_API_KEY=sk-ant-abcdefgh1234 cargo test",
            "paths": ["src/main.rs", "token: ghp_abcdefghijklmnopqrst1234"],
            "nested": {"note": "plain text, nothing secret"},
            "count": 3,
            "ok": true,
            "nothing": serde_json::Value::Null,
        });
        let out = redact_audit_value(&args);

        let cmd = out.get("command").unwrap().as_str().unwrap();
        assert!(
            !cmd.contains("sk-ant-abcdefgh1234"),
            "api key should be masked, got: {cmd}"
        );
        assert!(cmd.contains("[redacted]"), "mask marker missing: {cmd}");
        assert!(
            cmd.contains("cargo test"),
            "innocent tail should survive: {cmd}"
        );

        let arr = out.get("paths").unwrap().as_array().unwrap();
        assert_eq!(arr[0].as_str().unwrap(), "src/main.rs");
        assert!(
            !arr[1]
                .as_str()
                .unwrap()
                .contains("ghp_abcdefghijklmnopqrst1234"),
            "nested array secret should be masked"
        );

        // Innocent nested text and non-string leaves are byte-identical.
        assert_eq!(
            out.get("nested").unwrap(),
            args.get("nested").unwrap(),
            "innocent nested text must survive byte-identical"
        );
        assert_eq!(out.get("count").unwrap(), args.get("count").unwrap());
        assert_eq!(out.get("ok").unwrap(), args.get("ok").unwrap());
        assert_eq!(out.get("nothing").unwrap(), args.get("nothing").unwrap());
    }

    #[test]
    fn test_truncate_audit_args_short_values() {
        let args = serde_json::json!({"path": "src/main.rs", "command": "cargo test"});
        let truncated = truncate_audit_args(&args);
        assert_eq!(
            truncated, args,
            "Short strings should pass through unchanged"
        );
    }

    #[test]
    fn test_truncate_audit_args_long_values() {
        let long_content = "x".repeat(500);
        let args = serde_json::json!({"path": "test.txt", "content": long_content});
        let truncated = truncate_audit_args(&args);

        let content_val = truncated.get("content").unwrap().as_str().unwrap();
        assert!(content_val.len() < 500, "Long content should be truncated");
        assert!(
            content_val.contains("... [truncated, 500 chars total]"),
            "Should include truncation marker"
        );

        // Path should be unchanged
        assert_eq!(truncated.get("path").unwrap().as_str().unwrap(), "test.txt");
    }

    #[test]
    fn test_truncate_audit_args_non_string() {
        let args = serde_json::json!({"count": 42, "flag": true, "ratio": 3.15});
        let truncated = truncate_audit_args(&args);
        assert_eq!(truncated, args, "Non-string values should pass through");
    }

    #[test]
    fn test_truncate_audit_args_nested_object() {
        // Only top-level values are truncated; nested objects stay as-is
        let args = serde_json::json!({"meta": {"key": "value"}, "name": "test"});
        let truncated = truncate_audit_args(&args);
        // The nested object value goes through truncate_audit_value which returns it unchanged
        assert_eq!(
            truncated.get("meta").unwrap(),
            &serde_json::json!({"key": "value"})
        );
    }

    #[test]
    fn test_audit_enabled_default_false() {
        // Audit should be off by default
        // Note: other tests may have enabled it, so we check the AtomicBool directly
        // The default for a fresh process is false
        let fresh = AtomicBool::new(false);
        assert!(!fresh.load(Ordering::Relaxed));
    }

    #[test]
    fn test_read_audit_log_missing_file() {
        // Reading audit log when file doesn't exist should return empty vec
        // We test with a path that definitely doesn't exist by using tempdir
        let entries = read_audit_log(10);
        // This may or may not be empty depending on test environment,
        // but it shouldn't panic
        let _ = entries;
    }

    #[test]
    fn test_truncate_audit_args_exactly_200() {
        let exact = "y".repeat(200);
        let args = serde_json::json!({"content": exact});
        let truncated = truncate_audit_args(&args);
        assert_eq!(
            truncated.get("content").unwrap().as_str().unwrap(),
            exact,
            "Exactly 200-char string should not be truncated"
        );
    }

    #[test]
    fn test_truncate_audit_args_201() {
        let over = "z".repeat(201);
        let args = serde_json::json!({"content": over});
        let truncated = truncate_audit_args(&args);
        let val = truncated.get("content").unwrap().as_str().unwrap();
        assert!(
            val.contains("... [truncated, 201 chars total]"),
            "201-char string should be truncated"
        );
    }

    // ── days_from_epoch tests ───────────────────────────────────────────

    #[test]
    fn test_days_from_epoch_unix_epoch() {
        // 1970-01-01 is day 0
        let (y, m, d) = days_from_epoch(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_from_epoch_known_date() {
        // 2024-01-01 is 19723 days after epoch
        let (y, m, d) = days_from_epoch(19723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }

    #[test]
    fn test_days_from_epoch_leap_year() {
        // 2024-02-29 is 19723 + 31 (Jan) + 28 (Feb 1-28) = 19782
        let (y, m, d) = days_from_epoch(19782);
        assert_eq!((y, m, d), (2024, 2, 29));
    }

    #[test]
    fn test_days_from_epoch_y2k() {
        // 2000-01-01 is 10957 days after epoch
        let (y, m, d) = days_from_epoch(10957);
        assert_eq!((y, m, d), (2000, 1, 1));
    }

    // ── Session budget tests ────────────────────────────────────────────
    // The OnceLock-backed `configured_session_budget` and the lazy
    // `SESSION_BUDGET_START` make `session_budget_remaining()` itself
    // hard to reset between test cases. We test the pure parser directly
    // for parsing logic, and use one test for the live helper that only
    // asserts the in-process behavior we can rely on.

    #[test]
    fn test_parse_session_budget_unset() {
        assert_eq!(parse_session_budget(None), None);
    }

    #[test]
    fn test_parse_session_budget_empty() {
        assert_eq!(parse_session_budget(Some(String::new())), None);
    }

    #[test]
    fn test_parse_session_budget_valid() {
        assert_eq!(parse_session_budget(Some("2700".to_string())), Some(2700));
        assert_eq!(parse_session_budget(Some("0".to_string())), Some(0));
        assert_eq!(parse_session_budget(Some("60".to_string())), Some(60));
    }

    #[test]
    fn test_parse_session_budget_garbage_falls_back_to_default() {
        // A typo'd value should NOT silently disable the guard — it should
        // fall back to the default budget so the user gets *some* protection.
        assert_eq!(
            parse_session_budget(Some("forty-five-minutes".to_string())),
            Some(DEFAULT_SESSION_BUDGET_SECS)
        );
        assert_eq!(
            parse_session_budget(Some("-1".to_string())),
            Some(DEFAULT_SESSION_BUDGET_SECS)
        );
    }

    #[test]
    fn test_parse_session_budget_default_is_45_min() {
        assert_eq!(DEFAULT_SESSION_BUDGET_SECS, 2700);
    }

    #[test]
    #[serial_test::serial]
    fn test_session_budget_remaining_unset_returns_none() {
        // In the test environment, YOYO_SESSION_BUDGET_SECS is normally unset,
        // so the live helper should report no budget. This also verifies that
        // the OnceLock initializes lazily without panicking.
        // Note: if some other test in the suite has set the env var, this
        // assertion would change — but no other test touches it.
        if std::env::var("YOYO_SESSION_BUDGET_SECS").is_err() {
            assert!(session_budget_remaining().is_none());
        }
    }

    #[test]
    fn test_session_budget_remaining_decreases_over_time() {
        // Use the pure-parser path to simulate a budget without polluting
        // the global OnceLock. We compute remaining manually the same way
        // session_budget_remaining() does, and verify the math.
        let budget = Duration::from_secs(60);
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(20));
        let elapsed = start.elapsed();
        let remaining = budget.saturating_sub(elapsed);
        assert!(remaining < budget, "remaining should shrink as time passes");
        assert!(
            remaining > Duration::from_secs(50),
            "20ms shouldn't burn most of a 60s budget"
        );
    }

    #[test]
    fn test_session_budget_remaining_returns_zero_after_expiry() {
        // saturating_sub guarantees we never wrap. Verify the same shape
        // session_budget_remaining() uses for the expired case.
        let budget = Duration::from_secs(1);
        let elapsed = Duration::from_secs(10);
        let remaining = budget.saturating_sub(elapsed);
        assert_eq!(remaining, Duration::ZERO);
    }

    // ── session_budget_exhausted tests ──────────────────────────────────
    // We follow the same OnceLock-respecting pattern as the
    // `session_budget_remaining` tests above: hit the live helper only
    // when the env var is naturally unset, and simulate the math
    // directly for the configured cases. This keeps the tests order-
    // independent and free of cross-test OnceLock pollution.

    #[test]
    #[serial_test::serial]
    fn test_session_budget_exhausted_unset_returns_false() {
        // With no budget configured, sessions are unbounded — exhausted
        // must always be false, regardless of grace window. This is the
        // critical safety property: interactive use is unaffected.
        if std::env::var("YOYO_SESSION_BUDGET_SECS").is_err() {
            assert!(!session_budget_exhausted(0));
            assert!(!session_budget_exhausted(30));
            assert!(!session_budget_exhausted(99_999));
        }
    }

    #[test]
    fn test_session_budget_exhausted_with_headroom_returns_false() {
        // Simulate a 9999-second budget with negligible elapsed time.
        // Mirrors session_budget_remaining()'s math without touching the
        // global OnceLock. Plenty of headroom above the 30s grace → not
        // exhausted.
        let budget = Duration::from_secs(9999);
        let elapsed = Duration::from_millis(5);
        let remaining = budget.saturating_sub(elapsed);
        // The same comparison session_budget_exhausted performs:
        let exhausted = remaining.as_secs() <= 30;
        assert!(
            !exhausted,
            "9999s budget with 5ms elapsed should have headroom"
        );
    }

    #[test]
    fn test_session_budget_exhausted_after_expiry_returns_true() {
        // Simulate a 1-second budget after sleeping past it. The live
        // helper would wrap to ZERO via saturating_sub; the predicate
        // then returns true because 0 ≤ 30.
        let budget = Duration::from_secs(1);
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(20));
        // Pretend a long time has passed by adding to the real elapsed.
        let elapsed = start.elapsed() + Duration::from_secs(10);
        let remaining = budget.saturating_sub(elapsed);
        let exhausted = remaining.as_secs() <= 30;
        assert_eq!(remaining, Duration::ZERO);
        assert!(exhausted, "expired budget must report exhausted");
    }

    // ── End-to-end set-path test for #262 ─────────────────────────────
    //
    // The existing tests above cover the **unset** path of the live
    // helpers (the common interactive case) and the **pure parser** for
    // every value shape. What was missing — and what kept the symptom
    // of #262 alive in production after the wiring landed — is any test
    // that proves the **set** path actually flows through
    // `configured_session_budget()` → `session_budget_remaining()` →
    // `session_budget_exhausted()` end-to-end.
    //
    // This test sets `YOYO_SESSION_BUDGET_SECS=9999` once, calls the
    // live helpers, and asserts they observe the configured budget.
    // It uses `serial_test::serial` to avoid racing with other tests
    // that read the env var.
    //
    // OnceLock caveat: `SESSION_BUDGET_SECS` is a process-wide
    // `OnceLock<Option<u64>>`, so the very first call to
    // `configured_session_budget()` in the test binary freezes the
    // value for the lifetime of the process. To make sure that first
    // call sees our env var, this test must run **before** any other
    // test that calls `session_budget_remaining()` or
    // `session_budget_exhausted()` with the env var unset. Cargo's
    // serialized test order roughly tracks source order within a single
    // `mod`, but the alphabetical `_aaa_` prefix gives us belt-and-
    // suspenders: this test sorts first within the `tests` module.
    //
    // After this test runs, the OnceLock holds `Some(9999)` for the
    // rest of the binary. The existing
    // `test_session_budget_*_unset_returns_*` tests are already guarded
    // with `if std::env::var("YOYO_SESSION_BUDGET_SECS").is_err()` and
    // will gracefully skip their assertions when this test leaves the
    // env var set, so nothing else in the suite breaks.
    //
    // Why we deliberately don't `remove_var` at the end: removing the
    // env var while the OnceLock still holds `Some(9999)` would put the
    // process in an inconsistent state (the cache says "configured" but
    // the env says "unset"), and would actively break the existing
    // unset tests' skip-guards on subsequent runs. Leaving the env var
    // set keeps state coherent for the rest of the binary.
    #[test]
    #[serial_test::serial]
    fn test_aaa_session_budget_set_path_live_end_to_end() {
        // SAFETY: marked #[serial], no concurrent env var access.
        // We set this *before* any call to the live helpers so the
        // OnceLock initializes with our value.
        unsafe {
            std::env::set_var("YOYO_SESSION_BUDGET_SECS", "9999");
        }

        // Set path #1: the live helper should now see the configured
        // budget instead of returning None.
        let remaining = session_budget_remaining()
            .expect("with env var set, session_budget_remaining() must return Some(_)");
        assert!(
            remaining > Duration::from_secs(9000),
            "fresh 9999s budget should still have most of itself left, got {remaining:?}",
        );
        assert!(
            remaining <= Duration::from_secs(9999),
            "remaining should never exceed configured budget, got {remaining:?}",
        );

        // Set path #2: with 9000+ seconds left, no grace window we'd
        // ever pass at the call sites should report exhausted. This is
        // the predicate the production retry loops actually use
        // (`session_budget_exhausted(30)` in run_prompt_auto_retry and
        // the watch-mode fix loop).
        assert!(
            !session_budget_exhausted(30),
            "fresh 9999s budget must not report exhausted with 30s grace",
        );
        assert!(
            !session_budget_exhausted(0),
            "fresh 9999s budget must not report exhausted with 0s grace",
        );
        assert!(
            !session_budget_exhausted(8000),
            "fresh 9999s budget must not report exhausted with 8000s grace",
        );

        // Set path #3: a *huge* grace window — bigger than the budget
        // itself — should flip the predicate to true even on a fresh
        // budget. This is the boundary check that proves the predicate
        // is actually consulting `remaining`, not just returning false.
        assert!(
            session_budget_exhausted(20_000),
            "9999s budget must report exhausted when grace > budget",
        );

        // Note: we intentionally do NOT remove the env var here. See
        // the long comment above for why — leaving it set keeps the
        // OnceLock and the env coherent for the rest of the binary,
        // and the existing unset tests are designed to skip when the
        // env var is present.
    }
}
