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
    assert!(parsed(&["--verbose", "-p", "hi"]).verbose); // near-miss
                                                         // A value lookup: `--thinking` here is the system prompt, not a flag.
    let c = parsed(&["--system", "--thinking", "high", "-p", "hi"]);
    assert_eq!(c.thinking, yoagent::ThinkingLevel::Off);
    let c = parsed(&["--thinking", "high", "-p", "hi"]); // near-miss
    assert_ne!(c.thinking, yoagent::ThinkingLevel::Off);
}
