//! #1183: the REPL `/provider` door applies #1175's named-model refusal.
//! Every name the door accepts comes from the authorities (KNOWN_PROVIDERS
//! plus RETIRED_PROVIDERS), so a provider added later is covered.

use super::{handle_provider_switch, provider_switch_refusal};
use crate::providers::{
    default_model_for_provider, provider_requires_named_model, KNOWN_PROVIDERS, RETIRED_PROVIDERS,
};
use serial_test::serial;
use std::collections::HashMap;

fn config(base_url: Option<&str>) -> crate::AgentConfig {
    use crate::cli;
    crate::AgentConfig {
        model: "claude-opus-4-6".to_string(),
        api_key: "test-key".to_string(),
        provider: "anthropic".to_string(),
        base_url: base_url.map(str::to_string),
        skills: yoagent::skills::SkillSet::empty(),
        system_prompt: "Test.".to_string(),
        thinking: yoagent::ThinkingLevel::Off,
        max_tokens: None,
        temperature: None,
        max_turns: None,
        auto_approve: true,
        auto_commit: false,
        permissions: cli::PermissionConfig::default(),
        dir_restrictions: cli::DirectoryRestrictions::default(),
        context_strategy: cli::ContextStrategy::default(),
        context_window: None,
        shell_hooks: vec![],
        fallback_provider: None,
        fallback_model: None,
        auto_watch: true,
        allowed_tools: vec![],
        disallowed_tools: vec![],
        no_tools: false,
        lite: false,
        bash_cwd: None,
    }
}

fn door_names() -> Vec<&'static str> {
    let retired = RETIRED_PROVIDERS.iter().map(|(n, _)| *n);
    KNOWN_PROVIDERS.iter().copied().chain(retired).collect()
}

#[test]
fn census_every_name_needing_a_model_is_refused_and_nothing_changes() {
    let mut refused = vec![];
    for url in [None, Some("http://10.0.0.5:9000/v1")] {
        for p in door_names() {
            let mut cfg = config(url);
            let mut agent = cfg.build_agent();
            handle_provider_switch(p, &mut cfg, &mut agent);
            // github without a base URL is refused earlier (#1166) and is
            // not in this census: its own refusal already leaves things alone.
            let retired_no_url = url.is_none() && RETIRED_PROVIDERS.iter().any(|(n, _)| *n == p);
            if provider_requires_named_model(p) && !retired_no_url {
                assert_eq!(cfg.provider, "anthropic", "{p} url={url:?}");
                assert_eq!(cfg.model, "claude-opus-4-6", "{p} url={url:?}");
                assert!(provider_switch_refusal(p, &cfg).is_some(), "{p}");
                refused.push(format!("{p}@{}", url.is_some()));
            } else if retired_no_url {
                assert_eq!(
                    (cfg.provider.as_str(), cfg.model.as_str()),
                    ("anthropic", "claude-opus-4-6")
                );
            } else {
                // Near-miss: every other provider still switches to its default.
                assert_eq!(cfg.provider, p, "url={url:?}");
                assert_eq!(cfg.model, default_model_for_provider(p), "{p}");
                assert_eq!(provider_switch_refusal(p, &cfg), None, "{p}");
            }
        }
    }
    // The census as of #1183: custom both ways, github only with a base URL.
    assert_eq!(refused, ["custom@false", "custom@true", "github@true"]);
}

#[test]
fn custom_refusal_full_text_without_a_base_url() {
    let cfg = config(None);
    assert_eq!(
        provider_switch_refusal("custom", &cfg).unwrap(),
        "provider 'custom' needs a model name: yoyo cannot know which models your server \
at http://localhost:8080/v1 serves, so it does not guess one\n\
nothing was changed: still provider 'anthropic', model 'claude-opus-4-6'\n\
/provider takes no model name, so start yoyo with one:\n  \
yoyo --provider custom --model <id>\n  \
(or set provider and model in .yoyo.toml)\n\
to list the ids your server serves: curl http://localhost:8080/v1/models"
    );
}

#[test]
fn custom_refusal_full_text_with_the_sessions_base_url() {
    let cfg = config(Some("http://10.0.0.5:9000/v1/"));
    assert_eq!(
        provider_switch_refusal("custom", &cfg).unwrap(),
        "provider 'custom' needs a model name: yoyo cannot know which models your server \
at http://10.0.0.5:9000/v1 serves, so it does not guess one\n\
nothing was changed: still provider 'anthropic', model 'claude-opus-4-6'\n\
/provider takes no model name, so start yoyo with one:\n  \
yoyo --provider custom --base-url http://10.0.0.5:9000/v1/ --model <id>\n  \
(or set provider, model and base_url in .yoyo.toml)\n\
to list the ids your server serves: curl http://10.0.0.5:9000/v1/models"
    );
}

/// The remedy's command line, `<id>` filled in, split by a real shell and
/// fed to the startup parser: every word must land where the user meant.
#[cfg(unix)]
#[test]
#[serial]
fn remedy_round_trips_through_sh_and_parse_args() {
    let urls = [
        None,
        Some("http://10.0.0.5:9000/v1"),
        Some("http://h:1/v1?a=1&b=it's"), // needs quoting
    ];
    for (p, url) in [
        ("custom", urls[0]),
        ("custom", urls[1]),
        ("custom", urls[2]),
        ("github", urls[1]),
    ] {
        let msg = provider_switch_refusal(p, &config(url)).expect("refused");
        let line = msg
            .lines()
            .find(|l| l.trim_start().starts_with("yoyo "))
            .unwrap();
        let line = line
            .trim()
            .trim_start_matches("yoyo ")
            .replace("<id>", "qwen3-coder");
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf '%s\\0' {line}"))
            .output()
            .expect("sh runs");
        let mut args = vec!["yoyo".to_string()];
        args.extend(
            String::from_utf8(out.stdout)
                .unwrap()
                .split_terminator('\0')
                .map(str::to_string),
        );
        args.extend(["--api-key".to_string(), "test-key".to_string()]); // no env touched
        let c = crate::cli::parse_args_with_config(&args, HashMap::new(), String::new())
            .unwrap_or_else(|| panic!("{args:?} should parse"));
        assert_eq!(c.provider, p, "{args:?}");
        assert_eq!(c.model, "qwen3-coder", "{args:?}");
        assert_eq!(c.base_url.as_deref(), url, "{args:?}");
    }
}
