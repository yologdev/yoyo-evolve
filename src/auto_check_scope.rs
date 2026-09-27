//! Scope rule for the per-edit auto-check (#961).
//!
//! `AutoCheckTool` (in `tool_wrappers.rs`) runs the first watch command after
//! every successful `write_file` / `edit_file`. Under this repo's auto-watch
//! that command is the whole `cargo clippy ... && cargo test` gate, so a
//! `.md` or `.py` edit paid minutes per tool call for a check its edit cannot
//! change. The full watch still runs after the turn, so skipping here moves
//! latency only, never the final verdict.
//!
//! The rule is deliberately narrow and product-safe:
//! - a command that does not invoke `cargo` (as a whitespace token) always
//!   runs — every non-Cargo project keeps today's behaviour byte-identically;
//! - an empty path always runs — fail toward checking, never toward skipping;
//! - under a cargo command, only paths that can change a cargo result run it.

/// File names that change a cargo result even though they are not `.rs`.
const CARGO_INPUT_FILE_NAMES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "build.rs",
    "rust-toolchain",
    "rust-toolchain.toml",
];

/// Does `check_cmd` invoke cargo? Token match after splitting on whitespace
/// and shell separators, so `mycargo-lint` does not count but
/// `cd x && cargo test` does.
fn invokes_cargo(check_cmd: &str) -> bool {
    check_cmd
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | ')'))
        .any(|tok| tok == "cargo")
}

/// Could editing `edited_path` change the outcome of `check_cmd`?
/// `true` means "run the check" — the default for anything uncertain.
pub(crate) fn edit_can_affect_check(edited_path: &str, check_cmd: &str) -> bool {
    if !invokes_cargo(check_cmd) {
        return true;
    }
    let path = edited_path.trim();
    if path.is_empty() {
        return true;
    }
    let p = std::path::Path::new(path);
    if p.extension().is_some_and(|e| e == "rs") {
        return true;
    }
    if p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| CARGO_INPUT_FILE_NAMES.contains(&n))
    {
        return true;
    }
    p.components().any(|c| c.as_os_str() == ".cargo")
}

#[cfg(test)]
mod tests {
    use super::edit_can_affect_check;

    const CARGO: &str = "cargo clippy --all-targets -- -D warnings && cargo test";

    #[test]
    fn edit_can_affect_check_table() {
        let rows: &[(&str, &str, bool)] = &[
            // Non-code edits under a cargo command skip the per-edit check.
            ("README.md", CARGO, false),
            ("scripts/tool.py", CARGO, false),
            ("scripts/x.sh", CARGO, false),
            ("docs/src/a.json", CARGO, false),
            // Anything that can change a cargo result runs it.
            ("src/a.rs", CARGO, true),
            ("Cargo.toml", CARGO, true),
            ("crates/x/Cargo.toml", CARGO, true),
            ("Cargo.lock", CARGO, true),
            ("build.rs", CARGO, true),
            ("rust-toolchain", CARGO, true),
            ("rust-toolchain.toml", CARGO, true),
            (".cargo/config.toml", CARGO, true),
            ("/abs/proj/.cargo/config", CARGO, true),
            ("cd sub && cargo check", "", true), // empty path below
            // Near-miss: non-cargo commands keep today's behaviour for every path.
            ("README.md", "npm test", true),
            ("README.md", "pytest", true),
            ("README.md", "mycargo-lint", true),
            ("README.md", "make cargo-check", true),
            // Unknown/empty path fails toward running.
            ("", CARGO, true),
            ("   ", CARGO, true),
            // Cargo invoked after a shell separator is still cargo.
            ("README.md", "cd sub && cargo check", false),
            ("README.md", "(cargo test)", false),
        ];
        for (path, cmd, want) in rows {
            assert_eq!(
                edit_can_affect_check(path, cmd),
                *want,
                "edit_can_affect_check({path:?}, {cmd:?})"
            );
        }
    }

    /// A mock write tool: succeeds with fixed text, no side effects.
    struct OkTool;

    #[async_trait::async_trait]
    impl yoagent::types::AgentTool for OkTool {
        fn name(&self) -> &str {
            "write_file"
        }
        fn label(&self) -> &str {
            "write_file"
        }
        fn description(&self) -> &str {
            "mock"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: yoagent::types::ToolContext,
        ) -> Result<yoagent::types::ToolResult, yoagent::types::ToolError> {
            Ok(yoagent::types::ToolResult {
                content: vec![yoagent::Content::Text {
                    text: "written".to_string(),
                }],
                details: serde_json::Value::Null,
            })
        }
    }

    /// Emission point: drive the real `AutoCheckTool` with a watch command
    /// that invokes cargo AND touches a marker, then observe the marker.
    /// `true cargo` makes `cargo` a token without needing cargo on PATH.
    #[tokio::test]
    #[serial_test::serial]
    async fn auto_check_skips_cargo_command_for_non_code_edit_only() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran");
        let cmd = format!("true cargo; touch '{}'", marker.display());
        crate::watch::set_watch_command(&cmd);
        let tool = crate::tool_wrappers::with_auto_check(Box::new(OkTool));
        let ctx = || yoagent::types::ToolContext::new("t", "t");

        let md = tool
            .execute(serde_json::json!({"path": "README.md"}), ctx())
            .await
            .unwrap();
        assert!(!marker.exists(), ".md edit must not run the cargo check");
        assert_eq!(md.content.len(), 1);

        tool.execute(serde_json::json!({"path": "src/a.rs"}), ctx())
            .await
            .unwrap();
        let rs_ran = marker.exists();
        crate::watch::clear_watch_command();
        assert!(rs_ran, ".rs edit must run the cargo check");
    }
}
