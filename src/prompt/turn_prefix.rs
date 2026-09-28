//! Turn-prefix helpers: the per-turn text/blocks prepended to a prompt
//! before it is sent (the `[Effort: …]` hint and the one-shot
//! external-tool-failure note). Moved verbatim out of `src/prompt.rs`
//! (Day 212) to pay down that file's module-size drift; re-exported from
//! `prompt` so no call site changed.

use yoagent::Content;

/// Prepend the effort-level hint to user input when the effort level is not Medium.
///
/// Medium (default) returns an empty hint, so the input passes through unchanged.
/// Low and High prepend a bracketed instruction that guides the agent's response style.
/// This is applied per-turn so `/effort` changes take effect immediately.
pub(crate) fn apply_effort_hint(input: &str) -> String {
    apply_effort_hint_with(crate::cli_config::effort_level(), input)
}

/// Decision half of [`apply_effort_hint`], with the effort level injected.
///
/// The level lives in a process-global `AtomicU8` (`cli_config::EFFORT_LEVEL`), so a
/// test that exercised the wrapper had to *write* that global — and three such tests
/// raced each other on libtest's parallel threads, turning a green suite red at random
/// (`cargo test --bin yoyo apply_effort_hint` failed 6 of 12 runs; CI run 32748888895).
/// The global read now lives at the wrapper and the decision is pure, so the tests need
/// no process-global write at all. Same shape as `context_budget_warning_with`.
pub(crate) fn apply_effort_hint_with(level: crate::cli_config::EffortLevel, input: &str) -> String {
    let hint = level.system_hint();
    if hint.is_empty() {
        input.to_string()
    } else {
        format!("[Effort: {}]\n\n{}", hint, input)
    }
}

/// Prepend the external-tool-failure note to a turn's text input.
///
/// The **text-path** seam. `note` is injected rather than read from the
/// process-global one-shot so tests drive it explicitly — the remedy
/// `tests/global_state_races.rs` states as its best, and the shape
/// `apply_effort_hint_with` / `context_budget_warning_with` already use.
///
/// **`None` returns the input unchanged**, byte-identically. That is every
/// user whose configured servers all connect (and every user who configured
/// none), i.e. the whole regression surface.
///
/// The note leads because it is *session*-level context, while `[Effort: …]`
/// is a per-turn instruction that should sit adjacent to the request.
pub(crate) fn apply_external_failure_note_with(note: Option<String>, input: String) -> String {
    match note {
        Some(n) => format!("{n}\n\n{input}"),
        None => input,
    }
}

/// Prepend the external-tool-failure note as a leading content block.
///
/// The **image/content-path** seam, and it must exist separately: this path
/// composes its own `Content::Text` blocks and never goes through the text
/// path, so wiring one and not the other is exactly the "two doors, one policy,
/// one deaf" defect this whole change is about, one layer down.
///
/// A **new** block, never spliced into an existing one — `content` may carry
/// non-text blocks (images) and rewriting one would mangle them.
///
/// **`None` returns the blocks unchanged**, so the no-failure case is
/// byte-identical.
pub(crate) fn prepend_external_failure_block_with(
    note: Option<String>,
    blocks: Vec<Content>,
) -> Vec<Content> {
    match note {
        Some(text) => {
            let mut out = vec![Content::Text { text }];
            out.extend(blocks);
            out
        }
        None => blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_failure_note_absent_leaves_both_paths_byte_identical() {
        // The near-miss guard and the whole product-safety property: a user
        // whose servers all connect (or who configured none) must get exactly
        // the prompt they got before this change. assert_eq!, not contains.
        let input = "Refactor the parser".to_string();
        assert_eq!(
            apply_external_failure_note_with(None, input.clone()),
            input,
            "text path must be byte-identical with no failures"
        );

        let blocks = vec![
            Content::Text {
                text: "[Effort: think harder]".to_string(),
            },
            Content::Text {
                text: "look at this".to_string(),
            },
        ];
        assert_eq!(
            prepend_external_failure_block_with(None, blocks.clone()),
            blocks,
            "content path must be byte-identical with no failures"
        );
    }

    #[test]
    fn external_failure_note_reaches_the_text_path_emission_point() {
        let note = crate::agent_builder::external_tool_failure_note(&[
            crate::agent_builder::FailedServer {
                kind: "mcp",
                id: "npx -y @modelcontextprotocol/server-github".to_string(),
            },
        ]);
        let out = apply_external_failure_note_with(note, "Open the PR".to_string());

        assert!(
            out.contains("npx -y @modelcontextprotocol/server-github"),
            "the model must be able to name the server: {out}"
        );
        assert!(
            out.contains("unavailable") && out.contains("does not exist"),
            "must say unavailable, not nonexistent: {out}"
        );
        assert!(
            out.ends_with("Open the PR"),
            "the user's own request must survive intact: {out}"
        );
    }

    #[test]
    fn external_failure_note_reaches_the_content_path_and_carries_the_kind() {
        let note = crate::agent_builder::external_tool_failure_note(&[
            crate::agent_builder::FailedServer {
                kind: "openapi",
                id: "./specs/petstore.yaml".to_string(),
            },
        ]);
        let original = vec![Content::Text {
            text: "describe this image".to_string(),
        }];
        let out = prepend_external_failure_block_with(note, original.clone());

        assert_eq!(
            out.len(),
            2,
            "a NEW leading block, nothing spliced: {out:?}"
        );
        match &out[0] {
            Content::Text { text } => {
                assert!(text.contains("./specs/petstore.yaml"), "names it: {text}");
                // A note that misattributes the source is worse than none.
                assert!(text.contains("openapi"), "reports openapi: {text}");
                assert!(
                    !text.contains("mcp"),
                    "must not report an openapi failure as mcp: {text}"
                );
            }
            other => panic!("leading block must be text, got {other:?}"),
        }
        assert_eq!(out[1], original[0], "the caller's blocks are untouched");
    }

    #[test]
    fn external_failure_note_fires_once_at_the_emission_point() {
        // Turn 1 carries it; the one-shot is drained, so turn 2 is handed None
        // and must be byte-identical to the no-failure case.
        let note = crate::agent_builder::external_tool_failure_note(&[
            crate::agent_builder::FailedServer {
                kind: "mcp",
                id: "server-a".to_string(),
            },
        ]);
        let turn1 = apply_external_failure_note_with(note, "first".to_string());
        assert!(turn1.contains("server-a"), "turn 1 speaks: {turn1}");

        let turn2 = apply_external_failure_note_with(None, "second".to_string());
        assert_eq!(turn2, "second", "turn 2 must not re-fire a stale note");
    }

    #[test]
    fn test_apply_effort_hint_low_prepends() {
        use crate::cli_config::EffortLevel;
        // Level injected, not stored: these three tests used to write the process-global
        // EFFORT_LEVEL and raced each other on libtest's threads (6 of 12 runs red).
        let result = apply_effort_hint_with(EffortLevel::Low, "Hello agent");
        assert!(result.starts_with("[Effort: "));
        assert!(result.contains("concise"));
        assert!(result.ends_with("Hello agent"));
    }

    #[test]
    fn test_apply_effort_hint_medium_noop() {
        use crate::cli_config::EffortLevel;
        let result = apply_effort_hint_with(EffortLevel::Medium, "Hello agent");
        assert_eq!(result, "Hello agent");
    }

    #[test]
    fn test_apply_effort_hint_high_prepends() {
        use crate::cli_config::EffortLevel;
        let result = apply_effort_hint_with(EffortLevel::High, "Hello agent");
        assert!(result.starts_with("[Effort: "));
        assert!(result.contains("thorough"));
        assert!(result.ends_with("Hello agent"));
    }

    /// The wrapper must still read the process-global level, or the split would have
    /// silently disconnected `/effort` from every prompt. Pinned at the **source** level
    /// rather than by driving the global: writing `EFFORT_LEVEL` from a test is the exact
    /// hazard this split removed (three tests raced on it, 6 of 12 runs red), and one
    /// test doing it would leave the class alive for the next one. Needle built at
    /// runtime so it cannot match itself. **This is a weak check** — it proves the call
    /// is present, not that its result is used; the pure half above carries the real
    /// assertions.
    #[test]
    fn test_apply_effort_hint_wrapper_reads_the_global_level() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/prompt/turn_prefix.rs"
        ))
        .expect("read src/prompt/turn_prefix.rs");
        let body = src
            .split("pub(crate) fn apply_effort_hint(input: &str) -> String {")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("locate apply_effort_hint body");
        let needle = format!("{}{}", "effort_level", "()");
        assert!(
            body.contains(&needle),
            "apply_effort_hint no longer calls `{needle}`. The wrapper is the one place \
             the process-global effort level is read; without it `/effort` reaches no \
             prompt. Body was:\n{body}"
        );
    }
}
