//! The honest half of an OpenAI-compatible model id with no preset (Day 224).
//!
//! Day 223 gave `gpt-6-astra/sol/luna` exact-match presets on provider
//! `openai` (yoagent 0.25.8 added `gpt-6.1-sol`, #1037). Exact matching was
//! right, and it routes every OTHER id into
//! yoagent's base config: a 128K window and `max_tokens = 4096`, which caps
//! reasoning plus answer on every turn and used to say nothing. This module
//! does not invent limits for unknown ids (there is no source for them); it
//! makes the fallback say what it is assuming. `AgentConfig::build_agent`'s
//! OpenAI-compatible and google branches are the consumers (google since
//! Day 238, #1021, when membership moved from values to provenance).

/// Providers whose server the user runs and configures themselves, so their
/// limits are the user's to set there: never warned about, though
/// `ModelConfig::local`/`ollama` give every id the same numbers.
pub(crate) const USER_RUN_SERVER_PROVIDERS: &[&str] = &["ollama", "custom"];

/// Two ids no provider has a preset for. Whatever a provider resolves them to
/// is, by construction, its guess for an id it does not know.
const PROBE_IDS: [&str; 2] = ["zz-unknown-a", "zz-unknown-b"];

/// `true` when `model` on `provider` runs on that provider's guess (#1021,
/// Day 238). Membership is provenance, not value (Day 224): the id resolves
/// to exactly the limits two unknown probe ids resolve to, the probes agree,
/// and nothing declares real limits for it (`declared_output_ceiling`, which
/// covers the anthropic/openai presets and deepseek's provider-wide one, or a
/// `vendor_preset`). Until Day 238 membership was `== 128K && == 4096` on a
/// hand list, which left google's 1M / 8192 guess and minimax's 1M / 4096
/// silent. Unknown providers are never resolved here.
pub(crate) fn runs_on_guessed_limits(provider: &str, model: &str) -> bool {
    use crate::agent_builder::{create_model_config, declared_output_ceiling, vendor_preset};
    if !crate::providers::KNOWN_PROVIDERS.contains(&provider)
        || USER_RUN_SERVER_PROVIDERS.contains(&provider)
    {
        return false;
    }
    let limits = |id: &str| {
        let c = create_model_config(provider, id, None);
        (c.context_window, c.max_tokens)
    };
    let mine = limits(model);
    if declared_output_ceiling(provider, model, mine.1).is_some()
        || vendor_preset(provider, model).is_some()
    {
        return false;
    }
    let guess = limits(PROBE_IDS[0]);
    guess == limits(PROBE_IDS[1]) && mine == guess
}

/// Bytes of a model id shown in [`unknown_model_limits_note`] before the cut.
const UNKNOWN_MODEL_ID_MAX_BYTES: usize = 200;

/// The one-line warning for an OpenAI-compatible model id that has no preset
/// and therefore runs on yoagent's guessed limits, or `None` when there is
/// nothing to say.
///
/// `window` and `max_tokens` are the **resolved** config's values (whatever
/// the base config actually set), never typed copies. Returns `None` when:
/// the user set `max_tokens` themselves (`user_max_tokens`: they chose, do not
/// nag), or [`runs_on_guessed_limits`] is false. When the user set
/// `context_window` but not `max_tokens`, only the output cap is reported,
/// because claiming a guessed window would be false.
///
/// The id is user/config-authored, so it is sanitized and capped on a char
/// boundary with an ASCII marker. Glyph-free (pure ASCII apart from the id's
/// own escaped text) under `plain`.
pub(crate) fn unknown_model_limits_note(
    provider: &str,
    model: &str,
    window: u32,
    max_tokens: u32,
    user_max_tokens: Option<u32>,
    user_context_window: Option<u32>,
    plain: bool,
) -> Option<String> {
    if user_max_tokens.is_some() || !runs_on_guessed_limits(provider, model) {
        return None;
    }
    let shown = crate::cli::sanitize_for_display(model);
    let shown = if shown.len() > UNKNOWN_MODEL_ID_MAX_BYTES {
        let head = crate::format::safe_truncate(&shown, UNKNOWN_MODEL_ID_MAX_BYTES);
        format!("{head}...[{} bytes elided]", shown.len() - head.len())
    } else {
        shown
    };
    let marker = if plain { "warning: " } else { "⚠ " };
    let assumed = match user_context_window {
        Some(_) => format!("max_tokens = {max_tokens}"),
        None => format!("a {window}-token context window and max_tokens = {max_tokens}"),
    };
    Some(format!(
        "{marker}no preset is known for {provider} model '{shown}', so yoyo is using guessed \
limits: {assumed}. Every reply (reasoning included) is capped at {max_tokens} output tokens. \
If the model allows more, set max_tokens and context_window in .yoyo.toml \
(or pass --max-tokens / --context-window)."
    ))
}

/// Print [`unknown_model_limits_note`] on stderr, once per `(provider, model)`
/// per process, for a resolved `config`. Not quiet-gated: stdout is untouched,
/// so `-p` consumers parse the same bytes. `build_agent` calls this from the
/// google and OpenAI-compatible arms; the anthropic and bedrock arms do not
/// (open on #1021: bedrock's every id, anthropic's non-preset ids, run on a
/// guess and are not told).
pub(crate) fn warn_once_if_guessing(
    provider: &str,
    model: &str,
    config: &yoagent::provider::ModelConfig,
    user_max_tokens: Option<u32>,
    user_context_window: Option<u32>,
) {
    let plain = crate::format::is_plain_output();
    let (w, m) = (config.context_window, config.max_tokens);
    if let Some(note) = unknown_model_limits_note(
        provider,
        model,
        w,
        m,
        user_max_tokens,
        user_context_window,
        plain,
    ) {
        if first_unknown_limits_warning(&WARNED_UNKNOWN_MODEL_LIMITS, provider, model) {
            eprintln!("{note}");
        }
    }
}

/// Keys `(provider, model)` already warned about by
/// [`unknown_model_limits_note`], so a rebuild (`/model`, `/think`, fallback)
/// does not repeat the line. Once per pair per process.
pub(crate) static WARNED_UNKNOWN_MODEL_LIMITS: std::sync::Mutex<Vec<(String, String)>> =
    std::sync::Mutex::new(Vec::new());

/// Record `(provider, model)` in `store`; `true` only the first time. Takes
/// the store as a parameter so tests drive a local `Mutex`.
pub(crate) fn first_unknown_limits_warning(
    store: &std::sync::Mutex<Vec<(String, String)>>,
    provider: &str,
    model: &str,
) -> bool {
    let mut seen = crate::sync_util::lock_or_recover(store);
    if seen.iter().any(|(p, m)| p == provider && m == model) {
        return false;
    }
    seen.push((provider.to_string(), model.to_string()));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_builder::create_model_config;

    /// The base config's real numbers, read from yoagent, never typed copies.
    fn base() -> (u32, u32) {
        let c = yoagent::provider::ModelConfig::openai("x", "x");
        (c.context_window, c.max_tokens)
    }

    /// The note exactly as `build_agent` would compute it for a given id on a
    /// given provider with no user overrides: limits come from the resolved
    /// `create_model_config`, the same producer the call site reads.
    fn note_for(provider: &str, model: &str, plain: bool) -> Option<String> {
        let c = create_model_config(provider, model, None);
        unknown_model_limits_note(
            provider,
            model,
            c.context_window,
            c.max_tokens,
            None,
            None,
            plain,
        )
    }

    /// The note for a provider that is never resolved (the check must refuse
    /// before calling `create_model_config`, which logs unknown providers).
    fn note_for_unresolved(provider: &str) -> Option<String> {
        unknown_model_limits_note(provider, "m", 1, 1, None, None, true)
    }

    #[test]
    fn unknown_openai_id_warns_with_real_base_limits_and_override_keys() {
        let (window, max) = base();
        // Premise, so a yoagent change to the base guess is noticed here.
        assert_eq!((window, max), (128_000, 4096), "base guess changed");
        let out = note_for("openai", "gpt-6.9-test", true).expect("gpt-6.9-test must warn");
        assert_eq!(
            out,
            format!(
                "warning: no preset is known for openai model 'gpt-6.9-test', so yoyo is using \
guessed limits: a {window}-token context window and max_tokens = {max}. Every reply (reasoning \
included) is capped at {max} output tokens. If the model allows more, set max_tokens and \
context_window in .yoyo.toml (or pass --max-tokens / --context-window)."
            )
        );
    }

    #[test]
    fn day_223_preset_ids_stay_silent_near_miss() {
        for id in ["gpt-6-astra", "gpt-6-sol", "gpt-6-luna", "gpt-6.1-sol"] {
            assert_eq!(note_for("openai", id, false), None, "{id} has a preset");
        }
        // One character off a preset id: no preset, so it must warn. (Until
        // yoagent 0.25.8 this row was `gpt-6.1-sol`, which now has a preset.)
        assert!(note_for("openai", "gpt-6.9-test", false).is_some());
        assert!(note_for("openai", "gpt-6.1-sol-2", false).is_some());
        assert!(note_for("openai", "gpt-6-sol-2", false).is_some());
    }

    #[test]
    fn explicit_max_tokens_silences_it() {
        let (window, max) = base();
        assert_eq!(
            unknown_model_limits_note(
                "openai",
                "gpt-6.9-test",
                window,
                max,
                Some(4096),
                None,
                true
            ),
            None
        );
    }

    #[test]
    fn explicit_context_window_drops_the_window_claim_only() {
        let (window, max) = base();
        let out = unknown_model_limits_note(
            "openai",
            "gpt-6.9-test",
            window,
            max,
            None,
            Some(400_000),
            true,
        )
        .expect("output cap still applies");
        assert!(!out.contains(&window.to_string()), "{out}");
        assert!(out.contains(&format!("max_tokens = {max}")), "{out}");
    }

    /// #1021, Day 238: the four gemini-2.x ids on google's known list have no
    /// preset, so `ModelConfig::google` hands them its per-id-agnostic guess
    /// (1M window, max_tokens 8192). They warn with the numbers they really run
    /// on, read from the resolver, never typed here.
    #[test]
    fn google_ids_on_the_guess_warn_and_preset_ids_stay_silent() {
        let probe = create_model_config("google", PROBE_IDS[0], None);
        for id in [
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-2.5-flash-lite",
            "gemini-2.0-flash",
        ] {
            let out = note_for("google", id, true).unwrap_or_else(|| panic!("{id} must warn"));
            assert_eq!(
                out,
                format!(
                    "warning: no preset is known for google model '{id}', so yoyo is using \
guessed limits: a {w}-token context window and max_tokens = {m}. Every reply (reasoning \
included) is capped at {m} output tokens. If the model allows more, set max_tokens and \
context_window in .yoyo.toml (or pass --max-tokens / --context-window).",
                    w = probe.context_window,
                    m = probe.max_tokens
                )
            );
        }
        // Near-miss: the current google default has a vendor preset.
        assert_eq!(note_for("google", "gemini-3.8-flash", true), None);
        assert_eq!(note_for("google", "gemini-4-argon", true), None);
        // Anthropic default (fleet preset) stays silent.
        assert_eq!(note_for("anthropic", "claude-opus-5-5", true), None);
    }

    /// Anti-vacuous premise of the provenance check: for every provider the
    /// check can return `true` on, the two probe ids really resolve to equal
    /// limits, and for google those are the guess the gemini-2.x ids sit on.
    #[test]
    fn probe_ids_resolve_to_one_guess_per_provider() {
        for p in crate::providers::KNOWN_PROVIDERS {
            let a = create_model_config(p, PROBE_IDS[0], None);
            let b = create_model_config(p, PROBE_IDS[1], None);
            assert_eq!(
                (a.context_window, a.max_tokens),
                (b.context_window, b.max_tokens),
                "{p}: probes disagree, so 'same as an unknown id' means nothing here"
            );
        }
        let g = create_model_config("google", PROBE_IDS[0], None);
        let flash = create_model_config("google", "gemini-2.5-flash", None);
        assert_eq!(
            (g.context_window, g.max_tokens),
            (flash.context_window, flash.max_tokens)
        );
        assert_eq!(g.max_tokens, 8192, "google's guess changed; re-read #1021");
    }

    #[test]
    fn user_run_servers_and_declared_providers_stay_silent() {
        // ollama/custom: the user runs that server and sets its limits there.
        for p in USER_RUN_SERVER_PROVIDERS {
            assert_eq!(note_for(p, "some-unknown-model", true), None, "{p}");
        }
        // deepseek: one provider-wide preset yoyo declares (`declared_output_ceiling`).
        assert_eq!(note_for("deepseek", "some-unknown-model", true), None);
        // An unknown provider is not resolved at all (no stray stderr line).
        assert_eq!(note_for_unresolved("no-such-provider"), None);
        // Another arm on a guess does warn, naming its provider.
        let out = note_for("openrouter", "vendor/some-model", true).unwrap();
        assert!(
            out.contains("openrouter model 'vendor/some-model'"),
            "{out}"
        );
        // Unknown google ids now warn too (they were silent by value test).
        assert!(note_for("google", "some-unknown-model", true).is_some());
    }

    /// Population guard, by provenance (Day 224's lesson: membership is
    /// "this id gets the same limits as any unknown id", not "== 128K/4096").
    /// The set of providers whose unknown ids warn is derived through the
    /// note itself over `KNOWN_PROVIDERS`; the expected list is a counted
    /// register so a change shows up here by name.
    #[test]
    fn providers_whose_unknown_ids_run_on_a_guess_are_a_counted_register() {
        let mut guessing: Vec<&str> = crate::providers::KNOWN_PROVIDERS
            .iter()
            .copied()
            .filter(|p| note_for(p, "no-such-model-xyz", true).is_some())
            .collect();
        guessing.sort_unstable();
        let mut expected = vec![
            "anthropic",
            "bedrock",
            "cerebras",
            "github",
            "google",
            "groq",
            "minimax",
            "mistral",
            "openai",
            "openrouter",
            "xai",
            "zai",
        ];
        expected.sort_unstable();
        assert_eq!(guessing, expected);
    }

    /// Consumer-side register (#1037, Day 231): which providers' OWN built-in
    /// default model trips this warning. A user who only picks `--provider P`
    /// is warned on every run about a model yoyo chose for them, and capped at
    /// the base max_tokens. Read through the warning's own predicate over
    /// `KNOWN_PROVIDERS`, so the residue can only shrink by the number fixed.
    #[test]
    fn provider_defaults_that_trip_the_guessed_limits_warning_are_a_counted_register() {
        let mut tripping: Vec<&str> = Vec::new();
        for p in crate::providers::KNOWN_PROVIDERS {
            // ollama: local, the model id is whatever the user pulled.
            if *p == "ollama" {
                continue;
            }
            let model = crate::providers::default_model_for_provider(p);
            if note_for(p, &model, true).is_some() {
                tripping.push(p);
            }
        }
        tripping.sort_unstable();
        // Day 238 (#1021): `minimax` and `bedrock` joined when membership
        // moved from "== 128K/4096" to provenance. minimax's default runs on
        // its constructor's per-id guess (1M / 4096, the same harmful cap);
        // bedrock's on 200K / 16000 for every id.
        let mut expected = vec![
            "openrouter",
            "xai",
            "groq",
            "mistral",
            "github",
            "zai",
            "cerebras",
            "minimax",
            "bedrock",
        ];
        expected.sort_unstable();
        assert_eq!(
            tripping, expected,
            "fixing a provider's default removes it from this list; a new provider here \
means its default trips the guessed-limits warning (#1037)"
        );
        // Residue, stated rather than implied: the anthropic and bedrock arms
        // of `build_agent` do not call the note, so bedrock's row above is a
        // guess its user is NOT told about (open on #1021).
        for p in &tripping {
            let wired = !["anthropic", "bedrock"].contains(p);
            assert!(wired || *p == "bedrock", "{p}: unwired and tripping");
        }
        for silent in ["openai", "anthropic", "google", "deepseek"] {
            assert!(!tripping.contains(&silent), "{silent}");
        }
    }

    #[test]
    fn plain_mode_is_ascii_and_fancy_mode_has_marker() {
        let plain = note_for("openai", "gpt-6.9-test", true).unwrap();
        assert!(plain.is_ascii(), "{plain}");
        let fancy = note_for("openai", "gpt-6.9-test", false).unwrap();
        assert!(fancy.starts_with("⚠ "), "{fancy}");
    }

    #[test]
    fn hostile_id_is_sanitized_and_capped_on_a_char_boundary() {
        let hostile = "gpt-\x1b[2Jevil";
        assert!(hostile.as_bytes().contains(&0x1b), "fixture premise");
        let out = note_for("openai", hostile, true).unwrap();
        assert!(!out.as_bytes().contains(&0x1b), "{out:?}");

        let long = "é".repeat(300); // 600 bytes, 2-byte chars
        let out = note_for("openai", &long, true).unwrap();
        assert!(out.contains("bytes elided]"), "{out}");
        assert!(out.len() < long.len(), "id was capped");
    }

    #[test]
    fn warns_once_per_provider_model_pair() {
        let store = std::sync::Mutex::new(Vec::new());
        assert!(first_unknown_limits_warning(&store, "openai", "a"));
        assert!(!first_unknown_limits_warning(&store, "openai", "a"));
        assert!(first_unknown_limits_warning(&store, "openai", "b"));
        assert!(first_unknown_limits_warning(&store, "groq", "a"));
    }

    /// Weak source-level guard: `build_agent` calls the emitter in exactly
    /// two arms (google since Day 238, and the OpenAI-compatible one), and
    /// neither the call sites nor the emitter is gated on `is_quiet()`.
    /// Proves the calls are present in the source, not that they fire.
    #[test]
    fn call_site_is_not_quiet_gated() {
        let quiet = ["is_", "quiet()"].concat();
        let src = include_str!("agent_builder.rs");
        let needle = ["model_limits_note::", "warn_once_if_guessing("].concat();
        assert_eq!(
            src.matches(&needle).count(),
            2,
            "google + openai-compat arms"
        );
        for (at, _) in src.match_indices(&needle) {
            let window = &src[src[..at].rfind("let output_ceiling").unwrap()..at + 300];
            assert!(!window.contains(&quiet), "{window}");
        }
        let me = include_str!("model_limits_note.rs");
        let body_at = me
            .find(&["fn ", "warn_once_if_guessing("].concat())
            .unwrap();
        let body = &me[body_at..body_at + me[body_at..].find("\n}\n").unwrap()];
        assert!(!body.contains(&quiet), "{body}");
    }
}
