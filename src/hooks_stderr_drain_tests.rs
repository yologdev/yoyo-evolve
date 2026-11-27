//! Spawn-level tests for draining a hook's stderr while it runs (#1247).
//!
//! Before #1247, `ShellHook::run_command` read the stderr pipe only after
//! `try_wait()` reported exit. A hook writing more than the pipe buffer
//! (~64 KiB on Linux) blocked on `write`, never exited, was killed at the
//! 5-second timeout, and its feedback was dropped. These tests run real `sh`
//! hooks; the hook commands write only to their own stderr, so nothing touches
//! the repo or the cwd.

use super::{Hook, HookPhase, ShellHook, HOOK_STDERR_HEAD_BYTES, HOOK_STDERR_TAIL_BYTES};
use std::time::{Duration, Instant};

fn post_hook(command: &str) -> ShellHook {
    ShellHook {
        name: "drain-probe".to_string(),
        phase: HookPhase::Post,
        tool_pattern: "*".to_string(),
        command: command.to_string(),
    }
}

/// The #1247 shape: ~200 KB of stderr, then a clean exit. A post-hook's
/// stderr becomes feedback whatever its exit code, so `exit 0` is the case
/// whose output reaches the model.
#[test]
fn a_hook_writing_200kb_to_stderr_finishes_and_its_feedback_is_capped() {
    let hook = post_hook(
        "printf 'HEAD-MARKER\\n' >&2; \
         head -c 200000 /dev/zero | tr '\\000' x >&2; \
         printf '\\nTAIL-MARKER\\n' >&2; exit 0",
    );
    let start = Instant::now();
    let outcome = hook.run_command(&[], "", &[]);
    let elapsed = start.elapsed();

    let (code, stderr) = outcome.unwrap_or_else(|e| {
        panic!("a hook writing 200 KB to stderr must not time out (#1247), got: {e}")
    });
    assert!(
        elapsed < Duration::from_secs(3),
        "the hook exits as soon as its writes land; took {elapsed:?} (timeout is 5 s)"
    );
    assert_eq!(code, 0);
    assert!(
        stderr.starts_with("HEAD-MARKER\n"),
        "head kept: {:?}",
        &stderr.get(..40)
    );
    assert!(stderr.ends_with("\nTAIL-MARKER\n"), "tail kept");
    let marker_budget = 200;
    assert!(
        stderr.len() <= HOOK_STDERR_HEAD_BYTES + HOOK_STDERR_TAIL_BYTES + marker_budget,
        "feedback stays bounded by the #844 cap, got {} bytes",
        stderr.len()
    );
    // 12 + 200000 + 13 bytes written; 4096 + 4096 kept.
    let dropped = 12 + 200_000 + 13 - HOOK_STDERR_HEAD_BYTES - HOOK_STDERR_TAIL_BYTES;
    assert!(
        stderr.contains(&format!(
            "[yoyo: {dropped} bytes elided — hook stderr is capped at {HOOK_STDERR_HEAD_BYTES} head + {HOOK_STDERR_TAIL_BYTES} tail bytes]"
        )),
        "the cut is marked in #844's vocabulary with an honest byte count"
    );

    // And through the door that hands it to the model.
    let params = serde_json::json!({});
    let result = hook.post_execute("bash", &params, "out").unwrap();
    assert_eq!(result.output, "out");
    let feedback = result.feedback.expect("a 200 KB stderr is feedback");
    assert!(feedback.starts_with("HEAD-MARKER") && feedback.ends_with("TAIL-MARKER"));
}

/// Near-miss: a little stderr still arrives verbatim, byte for byte.
#[test]
fn a_hook_writing_a_little_stderr_gets_it_verbatim() {
    let hook = post_hook("printf 'lint: unused var é ✓\\nsecond line' >&2; exit 3");
    let (code, stderr) = hook.run_command(&[], "", &[]).expect("hook runs");
    assert_eq!(code, 3);
    assert_eq!(stderr, "lint: unused var é ✓\nsecond line");
}

/// Exactly at the cap: nothing is elided and no marker is added.
#[test]
fn stderr_exactly_at_the_cap_is_kept_whole() {
    let n = HOOK_STDERR_HEAD_BYTES + HOOK_STDERR_TAIL_BYTES;
    let hook = post_hook(&format!("head -c {n} /dev/zero | tr '\\000' y >&2"));
    let (_, stderr) = hook.run_command(&[], "", &[]).expect("hook runs");
    assert_eq!(stderr, "y".repeat(n));
}

/// A multi-byte character straddling the head cut is dropped whole, as
/// `cap_hook_stderr` does (#250): the result is valid text with no U+FFFD.
#[test]
fn a_multibyte_char_straddling_the_cut_is_not_split() {
    // 4095 ASCII bytes then a 3-byte '✓' crossing the 4096 head boundary.
    let hook = post_hook(&format!(
        "head -c {} /dev/zero | tr '\\000' a >&2; printf '✓' >&2; \
         head -c 20000 /dev/zero | tr '\\000' b >&2; printf '✓end' >&2",
        HOOK_STDERR_HEAD_BYTES - 1
    ));
    let (_, stderr) = hook.run_command(&[], "", &[]).expect("hook runs");
    assert!(!stderr.contains('\u{FFFD}'), "no split character");
    assert!(stderr.starts_with(&"a".repeat(HOOK_STDERR_HEAD_BYTES - 1)));
    assert!(stderr.ends_with("b✓end"));
}
