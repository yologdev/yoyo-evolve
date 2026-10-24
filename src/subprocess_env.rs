//! Opt-in credential scrub for the subprocesses the model drives (#1064).
//!
//! With `YOYO_SUBPROCESS_ENV_SCRUB=1` (or `true`), the model's bash tool
//! (`StreamingBashTool`) spawns `bash` with every provider credential
//! variable removed from its environment, so a prompt-injected
//! `echo $ANTHROPIC_API_KEY | curl …` finds nothing to send. yoyo itself
//! keeps its key: only the child's environment is edited, never yoyo's own.
//! The idea and the opt-in shape follow Claude Code's
//! `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1`.
//!
//! **Why it is opt-in, in present-tense user terms:** anyone who runs yoyo,
//! or any other provider-calling tool or test suite, *from inside* yoyo's
//! bash depends on that child inheriting the key. yoyo's own evolve
//! assessment runs `./target/debug/yoyo -p …` through the bash tool, and a
//! product user doing the same thing would break the same way. With the
//! variable unset, nothing changes, byte for byte: `scrub_vars` stays empty
//! and no `env_remove` is issued.
//!
//! **What is kept on purpose:** `GITHUB_TOKEN` and `GH_TOKEN`. `gh` in bash
//! needs them, and the reference scrub keeps them too. That holds even though
//! yoyo's own `--provider github` reads `GITHUB_TOKEN`: with that provider and
//! the scrub on, the key the child keeps is the provider key.
//!
//! **Scope today: the model's bash tool (opt-in) and shell hooks (on by
//! default).** Hooks (`[hooks]` in `.yoyo.toml`, run by `ShellHook` in
//! `hooks.rs`) are a different population from the model's bash: a hook is a
//! program the *repository* or the user's config wrote, and it never needs
//! the provider key to do its job, so the scrub is on for hooks unless
//! `YOYO_SUBPROCESS_ENV_SCRUB=0` (or `false`) turns it off
//! ([`hook_scrub_enabled_from`]). The model's bash keeps the opt-in decision
//! ([`scrub_enabled_from`]) unchanged, because a user running yoyo (or a
//! provider-calling test suite) from inside it needs the key. `/run`, `/bg`,
//! `/watch`, MCP stdio servers, and sub-agent child bash (built from
//! yoagent's own `BashTool`, not this constructor) still inherit the full
//! environment; they stay on #1064.

/// The opt-in switch, read once when the session's bash tool is built.
pub(crate) const SCRUB_ENV_VAR: &str = "YOYO_SUBPROCESS_ENV_SCRUB";

/// Names that are never scrubbed even when a provider table lists one.
const KEPT_VARS: &[&str] = &["GITHUB_TOKEN", "GH_TOKEN"];

/// Is the scrub on for this value of [`SCRUB_ENV_VAR`]? Only `1` and `true`
/// (any case) turn it on; unset, empty, `0`, `false` and anything else keep
/// today's behaviour.
pub(crate) fn scrub_enabled_from(value: Option<&str>) -> bool {
    match value {
        Some(v) => {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true")
        }
        None => false,
    }
}

/// The scrub flag as the process environment sets it right now.
pub(crate) fn scrub_enabled() -> bool {
    scrub_enabled_from(std::env::var(SCRUB_ENV_VAR).ok().as_deref())
}

/// Is the scrub on for **shell hooks** for this value of [`SCRUB_ENV_VAR`]?
/// The inverse default of [`scrub_enabled_from`]: unset scrubs, and only an
/// explicit `0` or `false` (any case, trimmed) opts out. Any other value,
/// including empty or a typo, keeps the scrub on, so a mistyped opt-out
/// fails closed rather than handing a repo-authored hook the key.
pub(crate) fn hook_scrub_enabled_from(value: Option<&str>) -> bool {
    match value {
        Some(v) => {
            let v = v.trim();
            !(v == "0" || v.eq_ignore_ascii_case("false"))
        }
        None => true,
    }
}

/// The names a shell hook's child should have stripped, for this value of
/// [`SCRUB_ENV_VAR`]: [`credential_vars`] unless the user opted out.
pub(crate) fn hook_scrub_vars_from(value: Option<&str>) -> Vec<&'static str> {
    if hook_scrub_enabled_from(value) {
        credential_vars()
    } else {
        Vec::new()
    }
}

/// [`hook_scrub_vars_from`] as the process environment sets it right now.
/// Read at each hook spawn, so the answer cannot go stale mid-session.
pub(crate) fn hook_scrub_vars() -> Vec<&'static str> {
    hook_scrub_vars_from(std::env::var(SCRUB_ENV_VAR).ok().as_deref())
}

/// Every credential variable the scrub removes: yoagent's own list
/// (`yoagent::provider::api_key_variables`, every name its key resolution
/// reads) unioned with every name yoyo's provider lookup reads
/// (`providers::provider_api_key_env` over `KNOWN_PROVIDERS`, plus the
/// `API_KEY` fallback in `cli.rs`), de-duplicated, minus [`KEPT_VARS`].
pub(crate) fn credential_vars() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    let yoyo_names = crate::providers::KNOWN_PROVIDERS
        .iter()
        .filter_map(|p| crate::providers::provider_api_key_env(p));
    let all = yoagent::provider::api_key_variables()
        .into_iter()
        .chain(yoyo_names)
        .chain(std::iter::once("API_KEY"));
    for name in all {
        if !KEPT_VARS.contains(&name) && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The names the session's bash tool should strip: [`credential_vars`] when
/// the scrub is on, nothing when it is off.
pub(crate) fn scrub_vars_for_session() -> Vec<&'static str> {
    if scrub_enabled() {
        credential_vars()
    } else {
        Vec::new()
    }
}

/// Remove each named variable from the child's environment. The parent's
/// environment is untouched.
pub(crate) fn apply_scrub(cmd: &mut tokio::process::Command, names: &[&str]) {
    for name in names {
        cmd.env_remove(name);
    }
}

/// [`apply_scrub`] for a `std::process::Command` (the hook spawn site is
/// synchronous). The parent's environment is untouched.
pub(crate) fn apply_scrub_std(cmd: &mut std::process::Command, names: &[&str]) {
    for name in names {
        cmd.env_remove(name);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn scrub_enabled_only_for_one_and_true() {
        let table: &[(Option<&str>, bool)] = &[
            (None, false),
            (Some(""), false),
            (Some("0"), false),
            (Some("false"), false),
            (Some("FALSE"), false),
            (Some("yes"), false),
            (Some("2"), false),
            (Some("1"), true),
            (Some("true"), true),
            (Some("TRUE"), true),
            (Some("True"), true),
            (Some(" 1 "), true),
        ];
        for (value, want) in table {
            assert_eq!(scrub_enabled_from(*value), *want, "value {value:?}");
        }
    }

    /// One table, both consumers: the model's bash stays opt-in (unset means
    /// no scrub, byte-identical to before), while hooks scrub unless opted
    /// out. Empty and unrecognised values split the same way: off for bash,
    /// on for hooks (fail closed).
    #[test]
    fn bash_and_hook_scrub_decisions_per_value() {
        // (value, bash scrubs, hook scrubs)
        let table: &[(Option<&str>, bool, bool)] = &[
            (None, false, true),
            (Some("1"), true, true),
            (Some("true"), true, true),
            (Some("0"), false, false),
            (Some("false"), false, false),
            (Some(" FALSE "), false, false),
            (Some(""), false, true),
            (Some("no"), false, true),
        ];
        for (value, bash, hook) in table {
            assert_eq!(scrub_enabled_from(*value), *bash, "bash, value {value:?}");
            assert_eq!(
                hook_scrub_enabled_from(*value),
                *hook,
                "hook, value {value:?}"
            );
            assert_eq!(
                hook_scrub_vars_from(*value).is_empty(),
                !*hook,
                "hook vars, value {value:?}"
            );
        }
        assert!(hook_scrub_vars_from(None).contains(&"ANTHROPIC_API_KEY"));
    }

    #[test]
    fn credential_vars_cover_both_tables_and_keep_github_tokens() {
        let names = credential_vars();
        assert!(names.contains(&"ANTHROPIC_API_KEY"), "{names:?}");
        assert!(names.contains(&"OPENAI_API_KEY"), "{names:?}");
        assert!(names.contains(&"API_KEY"), "{names:?}");
        // Derived from yoyo's own table, not hand-listed.
        for p in crate::providers::KNOWN_PROVIDERS {
            if let Some(var) = crate::providers::provider_api_key_env(p) {
                if KEPT_VARS.contains(&var) {
                    continue;
                }
                assert!(names.contains(&var), "{p}'s {var} missing: {names:?}");
            }
        }
        // And from yoagent's.
        for var in yoagent::provider::api_key_variables() {
            assert!(names.contains(&var), "yoagent's {var} missing: {names:?}");
        }
        assert!(!names.contains(&"GITHUB_TOKEN"), "{names:?}");
        assert!(!names.contains(&"GH_TOKEN"), "{names:?}");
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "a name listed twice: {names:?}");
    }

    fn canary_cmd() -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg("echo ${ANTHROPIC_API_KEY-unset} ${GH_TOKEN-unset}")
            .env("ANTHROPIC_API_KEY", "canary")
            .env("GH_TOKEN", "ghcanary");
        cmd
    }

    #[tokio::test]
    async fn apply_scrub_removes_provider_key_and_keeps_gh_token() {
        let mut cmd = canary_cmd();
        apply_scrub(&mut cmd, &credential_vars());
        let out = cmd.output().await.expect("sh runs");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "unset ghcanary\n");
    }

    #[tokio::test]
    async fn without_scrub_the_child_inherits_the_key() {
        let out = canary_cmd().output().await.expect("sh runs");
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Anti-vacuous: the fixture really does carry the canary.
        assert!(stdout.contains("canary"), "{stdout:?}");
        assert_eq!(stdout, "canary ghcanary\n");
    }

    /// Tool-level wiring, with no `set_var`: scrub a variable the test
    /// process already has (`HOME`), so the only thing that can make the
    /// child see it unset is `StreamingBashTool::scrub_vars`.
    #[tokio::test]
    async fn bash_tool_applies_its_scrub_vars_at_spawn() {
        use yoagent::types::AgentTool;
        let home = std::env::var("HOME").unwrap_or_default();
        let params = serde_json::json!({"command": "echo \"${HOME-unset}\""});
        let run = |tool: crate::tools::StreamingBashTool| {
            let params = params.clone();
            async move {
                let ctx = yoagent::types::ToolContext::new("call-scrub", "bash");
                let result = tool.execute(params, ctx).await.expect("bash runs");
                result
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        yoagent::types::Content::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<String>()
            }
        };
        let plain = run(crate::tools::StreamingBashTool::default()).await;
        let scrubbed = run(crate::tools::StreamingBashTool {
            scrub_vars: vec!["HOME"],
            ..Default::default()
        })
        .await;
        if !home.is_empty() {
            // Anti-vacuous: the near-miss really sees HOME.
            assert!(plain.contains(&home), "{plain:?}");
            assert!(!plain.contains("unset"), "{plain:?}");
        }
        assert!(scrubbed.contains("unset"), "{scrubbed:?}");
        assert!(home.is_empty() || !scrubbed.contains(&home), "{scrubbed:?}");
    }
}
