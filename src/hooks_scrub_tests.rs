//! Spawn-level tests for the shell-hook credential scrub (#1064).
//!
//! These drive the real `ShellHook::run_command` spawn. The provider key and
//! the plain variable are set on the *child command's* environment through
//! `env_vars`, never on yoyo's process environment, so no test here mutates
//! process globals. The scrub list comes from the same pure decision the
//! production callers use (`hook_scrub_vars_from`), fed the value the
//! user's `YOYO_SUBPROCESS_ENV_SCRUB` would carry.

use super::{HookPhase, ShellHook};
use crate::subprocess_env::hook_scrub_vars_from;

fn echo_hook() -> ShellHook {
    ShellHook {
        name: "scrub-canary".to_string(),
        phase: HookPhase::Post,
        tool_pattern: "*".to_string(),
        // stdout is discarded by run_command, so the canary writes to stderr.
        command: "echo \"${ANTHROPIC_API_KEY:-absent} ${YOYO_TEST_PLAIN_VAR:-absent}\" >&2"
            .to_string(),
    }
}

fn run_with(scrub_value: Option<&str>) -> String {
    let env = [
        ("ANTHROPIC_API_KEY", "sk-hook-canary"),
        ("YOYO_TEST_PLAIN_VAR", "plain-value"),
    ];
    let (code, stderr) = echo_hook()
        .run_command(&env, "", &hook_scrub_vars_from(scrub_value))
        .expect("hook spawns");
    assert_eq!(code, 0, "stderr: {stderr}");
    stderr.trim().to_string()
}

#[test]
fn hook_child_loses_the_provider_key_by_default_and_keeps_plain_vars() {
    assert_eq!(run_with(None), "absent plain-value");
}

#[test]
fn hook_child_keeps_the_key_with_explicit_opt_out() {
    assert_eq!(run_with(Some("0")), "sk-hook-canary plain-value");
    assert_eq!(run_with(Some("false")), "sk-hook-canary plain-value");
}

#[test]
fn hook_child_is_scrubbed_with_explicit_opt_in() {
    assert_eq!(run_with(Some("1")), "absent plain-value");
}
