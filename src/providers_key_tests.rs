//! #1064 point 1: which environment variables yoyo reads a provider's own key
//! from. The pure rows drive `find_provider_key` through an env-lookup closure
//! (no process-global env). The mirror guard checks `provider_api_key_vars`
//! against yoagent's real `resolve_api_key`, whose table is private: it re-runs
//! this test binary as a child that sets env vars one at a time, because a
//! child running a single test is the only place `set_var` cannot race.

use super::*;

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |var| {
        pairs
            .iter()
            .find(|(k, _)| *k == var)
            .map(|(_, v)| v.to_string())
    }
}

/// The lookup cli.rs and dispatch_sub.rs run: provider's own variables, then
/// the `ANTHROPIC_API_KEY` > `API_KEY` fallback they keep unchanged.
fn full_chain(provider: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    find_provider_key(provider, env).or_else(|| {
        ["ANTHROPIC_API_KEY", "API_KEY"]
            .iter()
            .find_map(|v| env(v).filter(|k| !k.is_empty()))
    })
}

/// (provider, env pairs, expected key).
type PresenceRow<'a> = (&'a str, &'a [(&'a str, &'a str)], Option<&'a str>);

#[test]
fn provider_key_presence_table() {
    #[rustfmt::skip]
    let rows: &[PresenceRow] = &[
        // The bug: Google's documented name alone used to be "No API key found".
        ("google", &[("GEMINI_API_KEY", "gem")], Some("gem")),
        // Near-miss: the old name alone still works.
        ("google", &[("GOOGLE_API_KEY", "goo")], Some("goo")),
        // An empty GEMINI_API_KEY counts as unset, as the old lookup did.
        ("google", &[("GEMINI_API_KEY", ""), ("GOOGLE_API_KEY", "goo")], Some("goo")),
        ("anthropic", &[("ANTHROPIC_API_KEY", "ant")], Some("ant")),
        // The load-bearing fallback, byte-identical.
        ("openai", &[("API_KEY", "generic")], Some("generic")),
        ("openai", &[("OPENAI_API_KEY", "oai"), ("API_KEY", "generic")], Some("oai")),
        ("github", &[("GITHUB_TOKEN", "gh")], Some("gh")),
        ("bedrock", &[("AWS_ACCESS_KEY_ID", "AKIA")], Some("AKIA")),
        // Nothing set: the "No API key found" path must still fire.
        ("google", &[], None),
        // A Google name does not leak into another provider.
        ("openai", &[("GEMINI_API_KEY", "gem")], None),
    ];
    for (provider, env, want) in rows {
        let got = full_chain(provider, &env_of(env));
        assert_eq!(
            got.as_deref(),
            *want,
            "provider {provider} with env {env:?}"
        );
    }
}

#[test]
fn provider_api_key_env_is_unchanged_in_this_slice() {
    // Hints and the wizard still name one variable (residue on #1064).
    assert_eq!(provider_api_key_env("google"), Some("GOOGLE_API_KEY"));
    assert!(provider_api_key_vars("google").contains(&"GOOGLE_API_KEY"));
}

const PROBE_ENV: &str = "YOYO_PROVIDER_KEY_PROBE";
const PROBE_TEST: &str = "providers::providers_key_tests::yoagent_resolver_probe_child";
/// Not a yoagent provider name, so its resolution is yoagent's generic fallback.
const UNKNOWN: &str = "zz-yoyo-probe-unknown";

/// Child half of the mirror guard: a no-op unless the parent set PROBE_ENV.
/// For each provider it prints the variables that resolve it ALONE and not an
/// unknown provider (yoagent's provider-specific names), then which value
/// wins when all of them are set (yoagent's precedence).
#[test]
fn yoagent_resolver_probe_child() {
    if std::env::var(PROBE_ENV).is_err() {
        return;
    }
    let all = yoagent::provider::api_key_variables();
    let clear = || all.iter().for_each(|v| std::env::remove_var(v));
    for provider in KNOWN_PROVIDERS {
        clear();
        if yoagent::provider::resolve_api_key(provider).is_some() {
            println!("PROBE\t{provider}\tkeyless\t");
            continue;
        }
        let mut own = Vec::new();
        for var in &all {
            clear();
            std::env::set_var(var, format!("v-{var}"));
            let mine = yoagent::provider::resolve_api_key(provider).is_some();
            let generic = yoagent::provider::resolve_api_key(UNKNOWN).is_some();
            if mine && !generic {
                own.push(*var);
            }
        }
        clear();
        own.iter()
            .for_each(|v| std::env::set_var(v, format!("v-{v}")));
        let winner = yoagent::provider::resolve_api_key(provider).unwrap_or_default();
        println!("PROBE\t{provider}\t{}\t{winner}", own.join(","));
    }
    clear();
}

/// Population guard (derived, not hand-listed): for every provider yoyo knows,
/// every name yoagent's resolver reads for it is in `provider_api_key_vars`,
/// and with all of them set yoyo picks the same value yoagent does. A variable
/// a future yoagent adds therefore fails here by name instead of being
/// invisible (the GEMINI_API_KEY shape).
#[test]
fn provider_api_key_vars_mirror_yoagents_resolver() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([PROBE_TEST, "--exact", "--nocapture", "--test-threads=1"])
        .env(PROBE_ENV, "1")
        .output()
        .expect("re-run the test binary as the probe child");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "probe child failed:\n{stdout}");
    let rows: Vec<Vec<&str>> = stdout
        .lines()
        // `split_once`, not `strip_prefix`: the harness prints its own "test … "
        // prefix on the same line as the child's first row.
        .filter_map(|l| l.split_once("PROBE\t").map(|(_, row)| row))
        .map(|l| l.split('\t').collect())
        .collect();
    // Anti-vacuous: one row per known provider, and google really is probed.
    assert_eq!(rows.len(), KNOWN_PROVIDERS.len(), "{stdout}");
    let google = rows.iter().find(|r| r[0] == "google").unwrap();
    assert!(google[1].contains("GEMINI_API_KEY"), "{google:?}");

    for row in &rows {
        let (provider, own, winner) = (row[0], row[1], row[2]);
        if own == "keyless" || own.is_empty() {
            continue;
        }
        // bedrock: yoagent's bearer-token arm is not mirrored (see the
        // yoyo-own comment in providers.rs); residue on #1064.
        if provider == "bedrock" {
            continue;
        }
        let mine = provider_api_key_vars(provider);
        for var in own.split(',') {
            assert!(
                mine.contains(&var),
                "yoagent resolves {provider} from {var}, but provider_api_key_vars({provider}) = {mine:?}"
            );
        }
        let pairs: Vec<(String, String)> = own
            .split(',')
            .map(|v| (v.to_string(), format!("v-{v}")))
            .collect();
        let lookup = |v: &str| pairs.iter().find(|(k, _)| k == v).map(|(_, x)| x.clone());
        assert_eq!(
            find_provider_key(provider, lookup).as_deref(),
            Some(winner),
            "{provider}: yoyo's precedence differs from yoagent's"
        );
    }
}
