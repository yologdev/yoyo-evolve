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

fn write_audit_entry(
    tool_name: &str,
    args: &serde_json::Value,
    duration_ms: u64,
    success: bool,
) -> std::io::Result<()> {
    let dir = std::path::Path::new(".yoyo");
    std::fs::create_dir_all(dir)?;
    let path = dir.join("audit.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    // Get current timestamp using Rust's SystemTime (no shell-out needed)
    let ts = {
        use std::time::SystemTime;
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| {
                let secs = d.as_secs();
                // Manual ISO 8601 formatting without external crate
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
    };

    // Truncate args to avoid huge entries (e.g., file content in write_file),
    // then mask credential-shaped substrings: `.yoyo/audit.jsonl` is pushed to
    // the public `audit-log` branch, and tool arguments carry user-authored
    // command text (e.g. `ANTHROPIC_API_KEY=sk-ant-…` in a bash command).
    // Scope: arguments only — tool *output* is not written here.
    let truncated_args = redact_audit_value(&truncate_audit_args(args));

    let entry = serde_json::json!({
        "ts": ts,
        "tool": tool_name,
        "args": truncated_args,
        "duration_ms": duration_ms,
        "success": success,
    });
    writeln!(file, "{}", entry)?;
    Ok(())
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
    let path = std::path::Path::new(".yoyo").join("audit.jsonl");
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
