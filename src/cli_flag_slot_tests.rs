//! #1114: the boolean scans in `parse_args_with_config` skip the value slot
//! of a value-taking flag. Driven through the parse result, because
//! auto-approve has no observable at the binary without a model turn.

use crate::cli::parse_args_with_config;
use serial_test::serial;
use std::collections::HashMap;

fn parsed(rest: &[&str]) -> crate::cli_config::Config {
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    let mut args = vec!["yoyo".to_string()];
    args.extend(rest.iter().map(|s| s.to_string()));
    // Empty config passed in, so the repo's own .yoyo.toml cannot leak in.
    parse_args_with_config(&args, HashMap::new(), String::new()).expect("should parse")
}

#[test]
#[serial]
fn yes_in_a_value_slot_does_not_turn_on_auto_approve() {
    let rows: &[(&[&str], bool)] = &[
        (&["--system", "--yes", "-p", "hi"], false), // the safety row
        (&["--system", "-y", "-p", "hi"], false),
        (&["-p", "--yes"], false), // `--yes` is the prompt
        (&["--system", "--print", "-p", "hi"], false), // --print implies --yes
        (&["--yes", "-p", "hi"], true), // near-miss: still applies
        (&["-y", "-p", "hi"], true),
        (&["--model", "x", "--yes", "-p", "hi"], true), // `x` is the value; --yes is a flag
        (&["--system", "s", "--yes", "-p", "hi"], true),
        (&["-p", "hi", "--", "--yes"], false), // #1110: after `--` is prompt text
    ];
    for (rest, want) in rows {
        let c = parsed(rest);
        assert_eq!(c.auto_approve, *want, "auto_approve for {rest:?}");
        assert_eq!(
            c.auto_edit, *want,
            "auto_edit (implied by --yes) for {rest:?}"
        );
    }
}

#[test]
#[serial]
fn other_booleans_and_value_lookups_skip_the_value_slot() {
    let c = parsed(&["--system", "--verbose", "-p", "hi"]);
    assert!(!c.verbose);
    let head: String = c.system_prompt.chars().take(40).collect();
    assert!(c.system_prompt.starts_with("--verbose"), "{head}");
    // Near-miss: a real `--verbose` still applies.
    assert!(parsed(&["--verbose", "-p", "hi"]).verbose);
    // A value lookup: `--thinking` here is the system prompt, not a flag.
    // No `-p`: with one, `high` would be a stray word and refused (#1150).
    let c = parsed(&["--system", "--thinking", "high"]);
    assert_eq!(c.thinking, yoagent::ThinkingLevel::Off);
    assert_eq!(c.prompt_arg.as_deref(), Some("high")); // the bare prompt
    let c = parsed(&["--thinking", "high", "-p", "hi"]); // near-miss
    assert_ne!(c.thinking, yoagent::ThinkingLevel::Off);
}

/// #1114, the last two readers: a repeatable flag in a value slot is not
/// collected, and every genuine occurrence still is.
#[test]
#[serial]
fn repeatable_flags_in_a_value_slot_are_not_collected() {
    let c = parsed(&["--system", "--deny", "-p", "hi"]);
    assert!(c.permissions.deny.is_empty(), "{:?}", c.permissions.deny);
    let c = parsed(&["--model", "--allow-dir", "-p", "hi"]);
    assert!(c.dir_restrictions.allow.is_empty());
    let c = parsed(&["-p", "hi", "--", "--deny", "x"]); // #1110: prompt text
    assert!(c.permissions.deny.is_empty(), "{:?}", c.permissions.deny);
    // Near-miss: every genuine occurrence is still collected.
    let c = parsed(&["--deny", "a", "--deny", "b", "-p", "hi"]);
    assert_eq!(c.permissions.deny, ["a", "b"]);
    // A genuine --deny whose value looks like a flag keeps that value (and
    // the flag-like-value warning); the word it swallowed is not a flag.
    let c = parsed(&["--deny", "--deny", "-p", "hi"]);
    assert_eq!(c.permissions.deny, ["--deny"]);
}

/// #1114: a numeric flag in a value slot is neither applied nor refused.
#[test]
#[serial]
fn numeric_flags_in_a_value_slot_are_not_applied() {
    let c = parsed(&["--system", "--max-turns", "3"]); // `3` is the bare prompt
    assert_eq!(c.max_turns, None);
    assert_eq!(c.prompt_arg.as_deref(), Some("3"));
    assert_eq!(
        parsed(&["--system", "--temperature", "0.5"]).temperature,
        None
    );
    // Near-miss: a genuine one still applies.
    assert_eq!(parsed(&["--max-turns", "3", "-p", "hi"]).max_turns, Some(3));
    // The startup refusal reads the same occurrence: no refusal for a flag
    // that is really --system's value, still one for a genuine bad value.
    let argv = |r: &[&str]| -> Vec<String> {
        std::iter::once("yoyo")
            .chain(r.iter().copied())
            .map(String::from)
            .collect()
    };
    let refusal = crate::cli_config::bad_numeric_flag_refusal;
    assert_eq!(
        refusal(&argv(&["--system", "--max-turns", "-p", "hi"])),
        None
    );
    assert!(refusal(&argv(&["--max-turns", "-p", "hi"])).is_some());
}
