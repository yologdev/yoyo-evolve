//! Integration tests that dogfood yoyo by spawning it as a subprocess.
//!
//! These tests verify real CLI behavior — argument parsing, error handling,
//! and output formatting — without requiring an API key or network access
//! (unless marked `#[ignore]`).
//!
//! Addresses Issue #69: dogfood yourself via subprocess.

use std::process::{Command, Stdio};

/// Build args for running the yoyo binary via `cargo run --`.
fn yoyo_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    // Clear API key env vars so tests don't accidentally use real keys
    cmd.env_remove("ANTHROPIC_API_KEY");
    cmd.env_remove("OPENAI_API_KEY");
    cmd.env_remove("GOOGLE_API_KEY");
    cmd.env_remove("API_KEY");
    cmd.env_remove("GROQ_API_KEY");
    cmd.env_remove("XAI_API_KEY");
    cmd.env_remove("DEEPSEEK_API_KEY");
    cmd.env_remove("OPENROUTER_API_KEY");
    cmd.env_remove("MISTRAL_API_KEY");
    cmd.env_remove("CEREBRAS_API_KEY");
    // Prevent config files from affecting tests
    cmd.env("HOME", "/nonexistent-yoyo-test-home");
    cmd.env_remove("XDG_CONFIG_HOME");
    cmd.env_remove("XDG_DATA_HOME");
    // Ensure NO_COLOR is not set (we test --no-color explicitly)
    cmd.env_remove("NO_COLOR");
    cmd
}

// ── --help ──────────────────────────────────────────────────────────

#[test]
fn help_flag_prints_usage_and_exits_zero() {
    let output = yoyo_cmd()
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(output.status.success(), "--help should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage:"),
        "help output should contain 'Usage:': {stdout}"
    );
    assert!(
        stdout.contains("--model"),
        "help output should mention --model flag"
    );
    assert!(
        stdout.contains("--help"),
        "help output should mention --help flag"
    );
}

#[test]
fn help_short_flag_prints_usage_and_exits_zero() {
    let output = yoyo_cmd()
        .arg("-h")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(output.status.success(), "-h should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Usage:"),
        "-h output should contain 'Usage:'"
    );
}

// ── --version ───────────────────────────────────────────────────────

#[test]
fn version_flag_prints_version_and_exits_zero() {
    let output = yoyo_cmd()
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(output.status.success(), "--version should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("yoyo v"),
        "version output should start with 'yoyo v': {stdout}"
    );
    // Should contain a semver-ish version number
    assert!(
        stdout.contains('.'),
        "version should contain a dot: {stdout}"
    );
}

#[test]
fn version_short_flag_prints_version_and_exits_zero() {
    let output = yoyo_cmd()
        .arg("-V")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(output.status.success(), "-V should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("yoyo v"),
        "-V output should start with 'yoyo v': {stdout}"
    );
}

// ── Empty stdin (piped mode) ────────────────────────────────────────

#[test]
fn empty_stdin_piped_mode_prints_error_and_exits_one() {
    let output = yoyo_cmd()
        // Provide a dummy API key so we get past the key check
        // and reach the piped-mode empty-stdin check
        .env("ANTHROPIC_API_KEY", "sk-ant-fake-for-test")
        .stdin(Stdio::piped())
        .output()
        .expect("failed to run yoyo");

    assert!(!output.status.success(), "empty stdin should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No input on stdin"),
        "should print 'No input on stdin.' on stderr: {stderr}"
    );
}

// ── Unknown flags ───────────────────────────────────────────────────

#[test]
fn unknown_flag_produces_warning_on_stderr() {
    // Use --provider ollama (no API key needed) with piped empty stdin
    // so we get past the key check and reach warn_unknown_flags.
    // The process will exit 1 due to empty stdin, but the warning should appear.
    let output = yoyo_cmd()
        .arg("--provider")
        .arg("ollama")
        .arg("--nonexistent-flag-xyz")
        .stdin(Stdio::piped()) // empty piped stdin triggers "No input on stdin"
        .output()
        .expect("failed to run yoyo");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning:") && stderr.contains("--nonexistent-flag-xyz"),
        "should warn about unknown flag on stderr: {stderr}"
    );
}

// ── --no-color suppresses ANSI codes ────────────────────────────────

#[test]
fn no_color_flag_suppresses_ansi_in_help() {
    let output = yoyo_cmd()
        .arg("--no-color")
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(output.status.success(), "--no-color --help should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // ANSI escape sequences start with \x1b[
    assert!(
        !stdout.contains("\x1b["),
        "help output with --no-color should not contain ANSI escapes: {stdout}"
    );
}

#[test]
fn no_color_env_suppresses_ansi_in_help() {
    let output = yoyo_cmd()
        .env("NO_COLOR", "1")
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(output.status.success(), "NO_COLOR=1 --help should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("\x1b["),
        "help output with NO_COLOR should not contain ANSI escapes: {stdout}"
    );
}

// ── Missing API key ────────────────────────────────────────────────

#[test]
fn missing_api_key_shows_helpful_error() {
    // Use piped stdin so it doesn't try to open a REPL
    let output = yoyo_cmd()
        .stdin(Stdio::piped())
        .output()
        .expect("failed to run yoyo");

    assert!(
        !output.status.success(),
        "missing API key should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should mention setting the env var, not panic
    assert!(
        stderr.contains("API") || stderr.contains("api_key") || stderr.contains("error"),
        "should show a helpful error about missing API key, not a panic: {stderr}"
    );
    // Should NOT contain a panic backtrace
    assert!(
        !stderr.contains("panicked at"),
        "should not panic: {stderr}"
    );
}

#[test]
fn missing_api_key_for_openai_shows_provider_specific_hint() {
    let output = yoyo_cmd()
        .arg("--provider")
        .arg("openai")
        .stdin(Stdio::piped())
        .output()
        .expect("failed to run yoyo");

    assert!(
        !output.status.success(),
        "missing OpenAI key should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("OPENAI_API_KEY"),
        "should hint about OPENAI_API_KEY: {stderr}"
    );
}

#[test]
fn ollama_provider_does_not_require_api_key() {
    // ollama/custom providers should not fail on missing API key
    // They'll fail on connection instead, but that's different from a key error.
    // Just check that --help still works with --provider ollama
    let output = yoyo_cmd()
        .arg("--provider")
        .arg("ollama")
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(
        output.status.success(),
        "--provider ollama --help should exit 0"
    );
}

// ── Piped input with bad API key (needs network) ────────────────────

#[test]
#[ignore] // Requires network access — run with `cargo test -- --ignored`
fn piped_input_with_bad_api_key_shows_auth_error_gracefully() {
    use std::io::Write;

    let mut child = yoyo_cmd()
        .env("ANTHROPIC_API_KEY", "sk-ant-this-is-not-a-real-key")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn yoyo");

    // Send input via stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(b"say hello")
            .expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to wait on yoyo");

    // Should exit 0 (graceful handling) or at least not panic
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{stderr}");

    assert!(
        !combined.contains("panicked at"),
        "should not panic on bad API key: {combined}"
    );

    // Should contain some indication of an auth/API error
    let has_error_indication = combined.contains("401")
        || combined.contains("auth")
        || combined.contains("invalid")
        || combined.contains("error")
        || combined.contains("Error")
        || combined.contains("API");
    assert!(
        has_error_indication,
        "should show auth error, got: {combined}"
    );
}
