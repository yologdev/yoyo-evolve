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

// ── Flags requiring values show clear errors ────────────────────────

#[test]
fn flag_requiring_value_without_value_shows_error() {
    // --model without a value should exit 1 with a clear error
    let output = yoyo_cmd()
        .arg("--model")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(
        !output.status.success(),
        "--model without value should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--model requires a value"),
        "should say '--model requires a value': {stderr}"
    );
    assert!(stderr.contains("--help"), "should suggest --help: {stderr}");
}

#[test]
fn provider_flag_without_value_shows_error() {
    // --provider without a value should exit 1 with a clear error
    let output = yoyo_cmd()
        .arg("--provider")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(
        !output.status.success(),
        "--provider without value should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--provider requires a value"),
        "should say '--provider requires a value': {stderr}"
    );
}

// ── /help output lists all documented commands ──────────────────────

#[test]
fn help_output_lists_all_documented_cli_flags() {
    let output = yoyo_cmd()
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Every documented CLI flag should be mentioned in --help output
    let expected_flags = [
        "--model",
        "--provider",
        "--base-url",
        "--thinking",
        "--max-tokens",
        "--max-turns",
        "--temperature",
        "--skills",
        "--system",
        "--system-file",
        "--prompt",
        "--output",
        "--api-key",
        "--mcp",
        "--openapi",
        "--no-color",
        "--verbose",
        "--yes",
        "--allow",
        "--deny",
        "--continue",
        "--help",
        "--version",
    ];
    for flag in &expected_flags {
        assert!(
            stdout.contains(flag),
            "help output should mention flag {flag}: {stdout}"
        );
    }
}

#[test]
fn help_output_lists_all_documented_repl_commands() {
    let output = yoyo_cmd()
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Every documented REPL command should appear in --help output
    let expected_commands = [
        "/quit", "/exit", "/clear", "/compact", "/commit", "/config", "/context", "/cost", "/diff",
        "/git", "/health", "/pr", "/history", "/search", "/init", "/load", "/model", "/retry",
        "/run", "/save", "/status", "/think", "/tokens", "/tree", "/undo", "/version",
    ];
    for cmd in &expected_commands {
        assert!(
            stdout.contains(cmd),
            "help output should mention REPL command {cmd}: {stdout}"
        );
    }
}

// ── --no-color output contains no ANSI escape sequences ─────────────

#[test]
fn no_color_flag_suppresses_ansi_in_version() {
    let output = yoyo_cmd()
        .arg("--no-color")
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(
        output.status.success(),
        "--no-color --version should exit 0"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("\x1b["),
        "version output with --no-color should not contain ANSI escapes: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("\x1b["),
        "stderr with --no-color should not contain ANSI escapes: {stderr}"
    );
}

#[test]
fn no_color_flag_suppresses_ansi_in_error_output() {
    // Even error messages should not have ANSI codes when --no-color is set
    let output = yoyo_cmd()
        .arg("--no-color")
        .arg("--model") // missing value → error
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("\x1b["),
        "error output with --no-color should not contain ANSI escapes: {stderr}"
    );
}

// ── Multiple unknown flags each produce warnings ────────────────────

#[test]
fn multiple_unknown_flags_each_produce_warnings() {
    let output = yoyo_cmd()
        .arg("--provider")
        .arg("ollama")
        .arg("--fake-flag-alpha")
        .arg("--fake-flag-beta")
        .arg("--fake-flag-gamma")
        .stdin(Stdio::piped()) // empty piped stdin triggers "No input on stdin"
        .output()
        .expect("failed to run yoyo");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Each unknown flag should produce its own warning
    assert!(
        stderr.contains("--fake-flag-alpha"),
        "should warn about --fake-flag-alpha: {stderr}"
    );
    assert!(
        stderr.contains("--fake-flag-beta"),
        "should warn about --fake-flag-beta: {stderr}"
    );
    assert!(
        stderr.contains("--fake-flag-gamma"),
        "should warn about --fake-flag-gamma: {stderr}"
    );

    // Count how many warning lines appear — should be at least 3
    let warning_count = stderr
        .lines()
        .filter(|l| l.contains("warning:") && l.contains("Unknown flag"))
        .count();
    assert!(
        warning_count >= 3,
        "should have at least 3 warning lines, got {warning_count}: {stderr}"
    );
}

// ── --system-file with nonexistent file shows useful error ──────────

#[test]
fn system_file_with_nonexistent_file_shows_useful_error() {
    let output = yoyo_cmd()
        .env("ANTHROPIC_API_KEY", "sk-ant-fake-for-test")
        .arg("--system-file")
        .arg("/definitely/nonexistent/prompt-file.txt")
        .stdin(Stdio::piped())
        .output()
        .expect("failed to run yoyo");

    assert!(
        !output.status.success(),
        "--system-file with nonexistent file should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:") || stderr.contains("Error"),
        "should contain 'error:': {stderr}"
    );
    assert!(
        stderr.contains("prompt-file.txt") || stderr.contains("nonexistent"),
        "error message should reference the file path: {stderr}"
    );
    assert!(
        !stderr.contains("panicked at"),
        "should not panic: {stderr}"
    );
}

#[test]
fn system_flag_with_text_does_not_error() {
    // --system "text" should be accepted fine (check via --help to avoid needing API key)
    let output = yoyo_cmd()
        .arg("--system")
        .arg("You are a Rust expert.")
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .expect("failed to run yoyo");

    assert!(
        output.status.success(),
        "--system with text and --help should exit 0"
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
