//! Provider constants and utilities — known providers, API key env vars, default models.

/// Known provider names for the --provider flag.
pub const KNOWN_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "google",
    "openrouter",
    "ollama",
    "xai",
    "groq",
    "deepseek",
    "mistral",
    "cerebras",
    "zai",
    "minimax",
    "bedrock",
    "github",
    "custom",
];

/// Get the environment variable name that holds the API key for a provider.
pub fn provider_api_key_env(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("OPENAI_API_KEY"),
        "google" => Some("GOOGLE_API_KEY"),
        "groq" => Some("GROQ_API_KEY"),
        "xai" => Some("XAI_API_KEY"),
        "deepseek" => Some("DEEPSEEK_API_KEY"),
        "openrouter" => Some("OPENROUTER_API_KEY"),
        "mistral" => Some("MISTRAL_API_KEY"),
        "cerebras" => Some("CEREBRAS_API_KEY"),
        "zai" => Some("ZAI_API_KEY"),
        "minimax" => Some("MINIMAX_API_KEY"),
        "bedrock" => Some("AWS_ACCESS_KEY_ID"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "github" => Some("GITHUB_TOKEN"),
        _ => None,
    }
}

/// Every variable yoyo reads a provider's own key from, first match wins
/// (#1064). The names are yoagent's `resolve_api_key` table for that provider,
/// in its order: that table is private in yoagent 0.25, so it is mirrored here
/// and `providers_key_tests.rs` checks the mirror against the real resolver in
/// a child process <!-- yoagent-version-claim: 0.27 -->. `provider_api_key_env`
/// still returns one name (the pre-#1064 one) for hints and the wizard.
pub fn provider_api_key_vars(provider: &str) -> &'static [&'static str] {
    match provider {
        "anthropic" => &["ANTHROPIC_API_KEY"],
        "openai" => &["OPENAI_API_KEY"],
        "google" => &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        "xai" => &["XAI_API_KEY"],
        "groq" => &["GROQ_API_KEY"],
        "deepseek" => &["DEEPSEEK_API_KEY"],
        "mistral" => &["MISTRAL_API_KEY"],
        "zai" => &["ZAI_API_KEY"],
        "minimax" => &["MINIMAX_API_KEY"],
        "openrouter" => &["OPENROUTER_API_KEY"],
        "cerebras" => &["CEREBRAS_API_KEY"],
        // yoyo-own: yoagent has no github provider; GitHub Models takes a GitHub token.
        "github" => &["GITHUB_TOKEN"],
        // yoyo-own: main.rs `apply_bedrock_credentials` joins this access key with
        // AWS_SECRET_ACCESS_KEY; yoagent's bearer-token name would be misjoined there.
        "bedrock" => &["AWS_ACCESS_KEY_ID"],
        _ => &[],
    }
}

/// The provider's own key from `lookup`, trying `provider_api_key_vars` in
/// order; empty values count as unset. `None` sends the caller to its
/// `ANTHROPIC_API_KEY` > `API_KEY` fallback.
pub fn find_provider_key(
    provider: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    provider_api_key_vars(provider)
        .iter()
        .find_map(|var| lookup(var).filter(|k| !k.is_empty()))
}

/// Get well-known model names for a provider (for diagnostic suggestions).
/// Returns a slice of commonly-used model identifiers.
///
/// Membership also silences the "Unknown model" warning, so an id written
/// here from memory hides from the one check that would catch it. Ids
/// checked against vendor pages and dropped are in `IDS_NO_VENDOR_SERVES`
/// (tests below, #1147); the non-Claude rows are otherwise unsourced yet.
pub fn known_models_for_provider(provider: &str) -> &'static [&'static str] {
    match provider {
        // The default leads: the 404 hint's switch line names `[0]`, and the
        // `Unknown model` warning shows the first five. Every id here must be a
        // yoagent preset or a yoagent price row
        // (`every_offered_claude_id_is_known_to_yoagent`), so a retired or
        // made-up id (#1144: sonnet-4-5, sonnet-4-7, sonnet-4-20250514) fails a
        // test instead of being offered. `claude-fable-5-1` is left off on
        // purpose: the preset path resolves it (cli.rs #941 test).
        "anthropic" => &[
            "claude-opus-5-5",
            "claude-sonnet-5-5",
            "claude-haiku-5-5",
            "claude-opus-5",
            "claude-fable-5",
            "claude-opus-4-8",
            "claude-sonnet-5",
            "claude-opus-4-7",
            "claude-opus-4-6",
            "claude-sonnet-4-6",
            "claude-haiku-4-5",
            "claude-haiku-4-5-20251001",
        ],
        "openai" => &[
            "gpt-5",
            "gpt-5-mini",
            "gpt-5.5",
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "codex-mini",
            "o3",
            "o3-mini",
            "o4-mini",
        ],
        // #1118: every id here is a yoagent preset or on Google's models page
        // (`every_offered_gemini_id_is_a_yoagent_preset_or_on_googles_page`).
        // 2.5 Pro/Flash are deprecated but served to existing users until
        // 2026-12-01; current ids come first so the warning's five name them.
        "google" => &[
            "gemini-4-argon",
            "gemini-3.8-flash",
            "gemini-3.6-flash",
            "gemini-3.1-pro-preview",
            "gemini-3.5-flash-lite",
            "gemini-3.1-flash-lite",
            "gemini-3-flash-preview",
            "gemini-2.5-flash-lite",
            "gemini-2.5-pro",
            "gemini-2.5-flash",
        ],
        "groq" => &[
            "llama-4-maverick-17b",
            "llama-4-scout-17b",
            "llama-3.3-70b-versatile",
            "llama-3.1-8b-instant",
        ],
        "xai" => &["grok-4.7", "grok-4", "grok-4-mini", "grok-3", "grok-3-mini"],
        // `deepseek-flash` is the id the evolve loop actually runs on (2026-09-15);
        // `deepseek-v4-pro` remains the legacy id for the same routed backend.
        // Listed first because it is the current one — this slice is only used for
        // membership tests and "did you mean" suggestions, so order is cosmetic.
        "deepseek" => &["deepseek-flash", "deepseek-v4-pro", "deepseek-v4-flash"],
        "mistral" => &[
            "mistral-large-4",
            "mistral-large-latest",
            "mistral-small-latest",
            "codestral-latest",
        ],
        "cerebras" => &["llama-3.3-70b"],
        "zai" => &["glm-4-plus", "glm-4-air", "glm-4-flash"],
        "minimax" => &[
            "MiniMax-M3",
            "MiniMax-M2.7",
            "MiniMax-M2.7-highspeed",
            "MiniMax-M2.5",
            "MiniMax-M2.5-highspeed",
            "MiniMax-M1",
        ],
        "bedrock" => &[
            "anthropic.claude-sonnet-4-6",
            "anthropic.claude-opus-4-7",
            "anthropic.claude-haiku-4-5-20251001-v1:0",
            "amazon.nova-pro-v1:0",
            "amazon.nova-lite-v1:0",
        ],
        "ollama" => &["llama3.2", "llama3.1", "codellama", "mistral"],
        "github" => &[
            "openai/gpt-4o",
            "openai/gpt-4o-mini",
            "openai/o3-mini",
            "openai/o4-mini",
            "meta/llama-4-scout-17b-16e-instruct",
            "mistral/mistral-large-latest",
            "deepseek/deepseek-v4-pro",
            "anthropic/claude-sonnet-4-6",
        ],
        _ => &[],
    }
}

/// OpenRouter's `anthropic/<id>` as the anthropic preset id (#1037): the
/// prefix dropped and each dot between two digits made a dash, so
/// `anthropic/claude-sonnet-5.5` (OpenRouter's only spelling) and the dashed
/// form both reach `claude-sonnet-5-5`. `None` without the prefix, for an
/// empty id, and for a `:variant` (`:batch` is priced differently).
pub(crate) fn openrouter_anthropic_preset_id(model: &str) -> Option<String> {
    let id = model.strip_prefix("anthropic/")?;
    if id.is_empty() || id.contains(':') {
        return None;
    }
    let b = id.as_bytes();
    let digit = |i: Option<usize>| i.and_then(|i| b.get(i)).is_some_and(u8::is_ascii_digit);
    Some(
        id.char_indices()
            .map(|(i, c)| match c {
                '.' if digit(i.checked_sub(1)) && digit(Some(i + 1)) => '-',
                _ => c,
            })
            .collect(),
    )
}

/// yoagent's OpenRouter preset for `model`, trying an `anthropic/` id's dotted
/// twin when yoagent refuses the dashed spelling (#1117): yoagent 0.27.2 knows
/// `anthropic/claude-opus-4.6` (price-table limits, no named preset) but not
/// `anthropic/claude-opus-4-6`, the spelling a config may carry. The twin is
/// yoagent's own OpenRouter rule (a trailing `-{major}-{minor}` is written
/// `-{major}.{minor}`), so a dashed id resolves exactly when its dotted twin
/// does: no hand-kept list. `:variant`s (`:batch` is half price) are never
/// respelled, and other vendors keep yoagent's refusal.
pub(crate) fn openrouter_routed_preset(model: &str) -> Option<yoagent::provider::ModelConfig> {
    use yoagent::provider::ModelConfig;
    ModelConfig::preset("openrouter", model).or_else(|| {
        let id = model.strip_prefix("anthropic/")?;
        let num = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let (head, minor) = id.rsplit_once('-')?;
        if id.contains(':') || !num(minor) || !head.rsplit_once('-').is_some_and(|(_, m)| num(m)) {
            return None;
        }
        let mut p = ModelConfig::preset("openrouter", &format!("anthropic/{head}.{minor}"))?;
        (p.id, p.name) = (model.to_string(), model.to_string());
        Some(p)
    })
}

/// Whether `model` is a **known** id for `provider`.
///
/// One matching rule, consulted by every surface that asks "is this id known?"
/// — the startup warning (`cli::unknown_model_warning`) and the model-config
/// path (`agent_builder::anthropic_preset`) must not disagree, because they
/// take the same input. #941: `--model claude-fable-5-1` resolved to the Fable
/// preset and priced correctly, while the warning used an *exact* list match
/// and told the user the model was unknown on every turn.
///
/// The rule is the union of two sources: the provider's own suggested list, or
/// — **for `anthropic` only** — the fleet preset table, which is prefix-matched
/// so a dated or suffixed id (`claude-haiku-4-5-20251001`, `claude-fable-5-1`)
/// resolves. The prefix list is never copied here; it is read through the same
/// lookup `create_model_config` uses, so the next rename cannot make the two
/// rules drift apart again.
///
/// The provider scoping is the half that matters: a Claude id under
/// `provider = "deepseek"` is **not** known and must keep warning. The presets
/// are Anthropic's, so resolving them under another provider would trade the
/// false positive this fixes for a false negative in its place.
pub fn model_is_known_for_provider(provider: &str, model: &str) -> bool {
    let known = known_models_for_provider(provider);
    if !known.is_empty() && known.contains(&model) {
        return true;
    }
    (provider == "anthropic" && crate::agent_builder::anthropic_preset(model).is_some())
        || (provider == "openai" && crate::agent_builder::openai_preset(model).is_some())
        || crate::agent_builder::vendor_preset(provider, model).is_some()
        || yoagent::provider::ModelConfig::preset(provider, model).is_some()
}

/// [`known_models_for_provider`] plus every yoagent preset id for `provider`
/// that is not already on it, appended so the existing order (and the first
/// five the `Unknown model` warning shows) is unchanged. Derived from
/// `ModelConfig::presets()`, never hand-typed (#1037).
pub fn known_model_ids_with_presets(provider: &str) -> Vec<String> {
    let mut ids: Vec<String> = known_models_for_provider(provider)
        .iter()
        .map(|id| id.to_string())
        .collect();
    for preset in yoagent::provider::ModelConfig::presets() {
        if preset.provider == provider && !ids.contains(&preset.id) {
            ids.push(preset.id);
        }
    }
    ids
}

/// Get the default model for a given provider.
pub fn default_model_for_provider(provider: &str) -> String {
    match provider {
        // The openai default is `gpt-6-sol` (#1037, Day 231), chosen by the same
        // three checks as the anthropic default below: `openai_preset` matches it
        // exactly, it is priced in yoagent's bundled prices, and the preset's
        // max_tokens is within `GPT_6_OUTPUT_MAXIMUM`. So a user who only picks
        // `--provider openai` gets real limits and a real `/cost`. The old `gpt-5`
        // had no preset and tripped yoyo's own guessed-limits warning (4096 output
        // tokens) on every run. `gpt-6.1-sol` (Codex's default) gained a preset and
        // a price in yoagent 0.25.8; switching the default to it is a separate
        // slice of #1037, not yet made.
        // Sol over astra/luna because sol is the tier the reference peer defaults to.
        "openai" => "gpt-6-sol".into(),
        // The google default is `gemini-3.8-flash` (#1021, Day 237) by the same
        // three checks: `vendor_preset` covers it, the bundled table prices it,
        // and its request default fits the declared ceiling (65,536). The old
        // `gemini-2.5-flash` has no yoagent preset, so it ran on guessed limits.
        "google" => "gemini-3.8-flash".into(),
        // #1037 (Day 247): OpenRouter lists Anthropic ids with dots only.
        // Both sonnet-5.5 and sonnet-4.6 now map onto an anthropic preset
        // (#1116), so limits no longer separate them; the reason sonnet-5.5
        // stays is price: OpenRouter lists it at $2/$10 per M in/out against
        // sonnet-4.6's $3/$15 (its /api/v1/models, read 2026-11-05).
        "openrouter" => "anthropic/claude-sonnet-5.5".into(),
        // No preset by nature: a user-run server, limits come from it (#1119,
        // `DEFAULT_WITHOUT_PRESET` in this file's tests holds the reason).
        "ollama" => "llama3.2".into(),
        // #1119 (Day 250): `grok-4.7` over the old `grok-4`, by the three
        // checks: `vendor_preset` matches it exactly (500K window, 64K
        // output; xAI publishes no output cap, so there is no ceiling to
        // exceed), and the bundled table prices it at $2/$6 per M (yoagent's
        // preset; $4/$12 for a prompt of 200K or more, +10% on us.api.x.ai).
        // `grok-4` had no preset: it ran on a guessed 128K / 4096-output
        // config, which cuts a long file write short, and /cost billed it
        // at an estimated $3/$15. So the move is cheaper AND gives real
        // limits. A user with `provider = "xai"` and no `model` silently
        // moves to grok-4.7; `--model grok-4` still reaches the old one.
        "xai" => "grok-4.7".into(),
        // No preset by necessity: yoagent ships none for any groq id, so the
        // default runs on guessed limits and says so (#1119, `DEFAULT_WITHOUT_PRESET`).
        "groq" => "llama-3.3-70b-versatile".into(),
        // The default stays `deepseek-v4-pro` on purpose. `deepseek-flash`
        // (V4.1-Flash) is where requests are routed from 2026-09-14, so promoting
        // it is defensible — but a default is product-visible: anyone who relies
        // on `provider = "deepseek"` with no explicit `model` would silently move
        // to a different model. This task makes the configured model *visible and
        // priced*; changing what an existing user gets is a separate, deliberate
        // decision and is not bundled into a lookup-table fix (the module-size
        // task's own lesson about widening a verified narrow change).
        "deepseek" => "deepseek-v4-pro".into(),
        // #1119 (Day 250): `mistral-large-4` over `mistral-large-latest`.
        // It has a `vendor_preset` (1M window, 32K output; Mistral publishes
        // no output cap) and a bundled price of $0.70/$2.10 per M, against
        // the $0.50/$1.50 yoyo's table bills any `mistral-large*` id. So this
        // costs a no-config user more, and it is a Public Preview model. Why
        // move anyway: the alias ran on a guessed 4096-token output limit,
        // too small for a coding agent's file writes, and Mistral's models
        // page (read 2026-11-05) lists Mistral Large 3 (`mistral-large-2512`)
        // as deprecated since 2026-10-31, retiring 2027-04-30, with Large 4
        // named as its replacement. Users with `provider = "mistral"` and no
        // `model` silently move; `--model mistral-large-latest` still works.
        "mistral" => "mistral-large-4".into(),
        // No preset by necessity: yoagent ships none for any cerebras id, so the
        // default runs on guessed limits and says so (#1119, `DEFAULT_WITHOUT_PRESET`).
        "cerebras" => "llama-3.3-70b".into(),
        // #1119 (Day 250): `glm-5.3`, Z.ai's current flagship, over the
        // 2024 flagship `glm-4-plus`. It has a `vendor_preset` (1M window,
        // 128K output, Z.ai's published cap) and a bundled price of
        // $1.40/$4.40 per M against glm-4-plus's $0.70/$0.70 in yoyo's table,
        // so output costs a no-config user ~6x more, and GLM-5.3 always
        // reasons. Why move anyway: glm-4-plus ran on a guessed 4096-token
        // output limit, and it is absent from Z.ai's pricing page (read
        // 2026-11-05), so it may not be served at all. A cheaper preset
        // exists (`glm-5.3-flash`, $0.15/$0.50) for users who pick it.
        // Users with `provider = "zai"` and no `model` silently move.
        "zai" => "glm-5.3".into(),
        // No preset by necessity: yoagent ships none for any minimax id, so the
        // default runs on guessed limits and says so (#1119, `DEFAULT_WITHOUT_PRESET`).
        "minimax" => "MiniMax-M2.7".into(),
        "bedrock" => "anthropic.claude-sonnet-4-6".into(),
        // No preset by necessity: yoagent ships none for any github id, so the
        // default runs on guessed limits and says so (#1119, `DEFAULT_WITHOUT_PRESET`).
        "github" => "openai/gpt-4o".into(),
        // The anthropic (and unknown-provider) default is `claude-opus-5-5` by
        // decision, checked Day 230 (#1037) rather than picked as "newest": it has
        // a yoagent preset (`anthropic_preset`, most-specific arm) with bundled
        // pricing ($4/$20, below the $5/$25 Opus 4.6 it replaces), and a
        // no-config run sends the preset's 64K `max_tokens`, under the 128000
        // `anthropic_output_maximum`. A future change needs the same three checks.
        _ => "claude-opus-5-5".into(),
    }
}

/// #1103: `Some(refusal)` when `provider` is not a known name AND no base URL
/// was given, so startup can refuse (exit 2) before any network call.
///
/// Who reaches the refused branch: anyone who mistypes a provider, on the
/// `--provider` flag or `provider = "..."` in `.yoyo.toml`. Nobody chooses it:
/// before this, the typo silently became an OpenAI-compatible client aimed at
/// `localhost:8080`, carrying whatever key the fallback chain found
/// (`ANTHROPIC_API_KEY`, then `API_KEY`), retried for ~80s and blamed the
/// network. An unknown name WITH a base URL is a choice (the user pointed it
/// somewhere), so it returns `None` and keeps that path.
///
/// Did-you-mean only within edit distance 2, via the one shared
/// `commands::closest_match`; otherwise the known list. The name is
/// sanitized (a project `.yoyo.toml` is repository-authored). Glyph-free
/// except the em dash; callers add colour.
pub fn unknown_provider_refusal(provider: &str, base_url_given: bool) -> Option<String> {
    if KNOWN_PROVIDERS.contains(&provider) || base_url_given {
        return None;
    }
    let shown = crate::cli::sanitize_for_display(provider);
    let mut msg = format!("unknown provider '{shown}'");
    match crate::commands::closest_match(provider, KNOWN_PROVIDERS, 2) {
        Some(near) => msg.push_str(&format!(" — did you mean '{near}'?")),
        None => msg.push_str(&format!(
            ". Known providers: {}",
            KNOWN_PROVIDERS.join(", ")
        )),
    }
    msg.push_str("\nfor an OpenAI-compatible server use: --provider custom --base-url <url>");
    Some(msg)
}

/// #1157: limits for offered `google` ids yoagent has no preset for, each
/// read from that id's own page (2026-11-08): `(id, input token limit,
/// output token limit, source)`. Without a row the run path would use
/// `ModelConfig::google`'s 1,000,000 / 8192 guess, cutting replies at 8192.
/// A row must never shadow a yoagent preset (a test checks both ways).
pub(crate) const GOOGLE_PUBLISHED_LIMITS: &[(&str, u32, u32, &str)] = &[
    (
        "gemini-3.5-flash-lite",
        1_048_576,
        65_536,
        "https://ai.google.dev/gemini-api/docs/models/gemini-3.5-flash-lite",
    ),
    (
        "gemini-3.1-flash-lite",
        1_048_576,
        65_536,
        "https://ai.google.dev/gemini-api/docs/models/gemini-3.1-flash-lite",
    ),
    (
        "gemini-3-flash-preview",
        1_048_576,
        65_536,
        "https://ai.google.dev/gemini-api/docs/models/gemini-3-flash-preview",
    ),
    (
        "gemini-2.5-flash-lite",
        1_048_576,
        65_536,
        "https://ai.google.dev/gemini-api/docs/models/gemini-2.5-flash-lite",
    ),
    (
        "gemini-2.5-pro",
        1_048_576,
        65_536,
        "https://ai.google.dev/gemini-api/docs/models/gemini-2.5-pro",
    ),
    (
        "gemini-2.5-flash",
        1_048_576,
        65_536,
        "https://ai.google.dev/gemini-api/docs/models/gemini-2.5-flash",
    ),
];

/// `(context_window, max_output)` from [`GOOGLE_PUBLISHED_LIMITS`].
pub(crate) fn google_published_limits(id: &str) -> Option<(u32, u32)> {
    GOOGLE_PUBLISHED_LIMITS
        .iter()
        .find(|(row, ..)| *row == id)
        .map(|&(_, window, out, _)| (window, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1147 (Day 251): ids yoyo used to offer that no vendor serves, each with
    /// the page that says so (all read 2026-11-06). Kept as a register so a
    /// later session re-adding one from memory fails here, by name.
    const IDS_NO_VENDOR_SERVES: &[(&str, &str, &str)] = &[
        (
            "openai",
            "o4-mini-high",
            "not an API id: developers.openai.com/api/docs/models/o4-mini-high is a 404, \
             and /models/all lists o4-mini and o4-mini-deep-research only",
        ),
        (
            "openai",
            "gpt-5.5-mini",
            "developers.openai.com/api/docs/models/gpt-5.5-mini is a 404; /models/all lists \
             gpt-5.5 and gpt-5.5-pro only",
        ),
        (
            "groq",
            "mixtral-8x7b-32768",
            "retired: console.groq.com/docs/deprecations, 'March 20, 2025: Mixtral 8x7B'",
        ),
        (
            "xai",
            "grok-2",
            "retired: absent from docs.x.ai/developers/models; the grok-2 family answers \
             'Model not found - retired by xAI' (reported in BerriAI/litellm#38179)",
        ),
        (
            "minimax",
            "MiniMax-M1-40k",
            "not an API id: platform.minimax.io/docs/api-reference/text-post names \
             MiniMax-M1, never -40k",
        ),
        (
            "google",
            "gemini-2.0-flash",
            "shut down: ai.google.dev/gemini-api/docs/models lists it under Previous \
             models as '(Shut down)' (read 2026-11-08, #1118)",
        ),
        (
            "google",
            "gemini-3.0-pro",
            "not an API id: absent from ai.google.dev/gemini-api/docs/models (read \
             2026-11-08); Google's ids are gemini-3-pro-preview (shut down) and 3.1-pro-preview",
        ),
        (
            "google",
            "gemini-3.0-flash",
            "not an API id: absent from ai.google.dev/gemini-api/docs/models (read \
             2026-11-08); Google's id is gemini-3-flash-preview",
        ),
    ];

    /// A yoagent preset or bundled price row for `google`/`id`.
    fn yoagent_knows_gemini(id: &str) -> bool {
        yoagent::provider::ModelConfig::preset("google", id).is_some()
            || yoagent::provider::PriceTable::builtin()
                .iter()
                .any(|(p, model, _)| p == "google" && model == id)
    }

    /// #1118: a Gemini id on the picker or the known list silences the
    /// `Unknown model` warning, so each one needs an outside source: yoagent,
    /// or a row above naming Google's page. `gemini-3.0-pro` (offered from
    /// memory, never a Google id) would fail here by name.
    #[test]
    fn every_offered_gemini_id_is_a_yoagent_preset_or_on_googles_page() {
        let picker = crate::commands::KNOWN_MODELS
            .iter()
            .filter(|id| id.starts_with("gemini-"));
        for id in known_models_for_provider("google").iter().chain(picker) {
            // #1157: the page rows now live in the production table.
            let sourced = google_published_limits(id).is_some();
            assert!(
                yoagent_knows_gemini(id) || sourced,
                "google offers {id}: no yoagent preset/price row and no Google-page row"
            );
        }
        // A page row must not shadow a yoagent row (it would outlive its premise).
        for (id, _, _, url) in GOOGLE_PUBLISHED_LIMITS {
            assert!(
                !yoagent_knows_gemini(id),
                "{id} is in yoagent now; drop its row ({url})"
            );
            assert!(
                known_models_for_provider("google").contains(id),
                "{id} row is unused"
            );
        }
        // Every offered id's window: 1,048,576 on each id's Google page.
        for id in known_models_for_provider("google") {
            assert_eq!(
                crate::commands_info::model_context_window(id),
                Some(1_048_576),
                "{id}"
            );
        }
        // Recall control: the dead ids really fail the authority.
        for dead in ["gemini-3.0-pro", "gemini-3.0-flash", "gemini-2.0-flash"] {
            assert!(!yoagent_knows_gemini(dead), "{dead} must fail");
        }
    }

    #[test]
    fn ids_no_vendor_serves_are_not_offered_and_warn_when_typed() {
        for (provider, id, why) in IDS_NO_VENDOR_SERVES {
            for p in KNOWN_PROVIDERS {
                assert!(
                    !known_models_for_provider(p).contains(id),
                    "{p} offers {id} ({why})"
                );
            }
            assert!(
                !crate::commands::KNOWN_MODELS.contains(id),
                "/model completion offers {id} ({why})"
            );
            assert_ne!(
                default_model_for_provider(provider),
                *id,
                "{id} is {provider}'s default"
            );
            let warning = crate::cli::unknown_model_warning(provider, id)
                .unwrap_or_else(|| panic!("typing {id} on {provider} must warn ({why})"));
            assert!(
                warning.contains(&format!("Unknown model '{id}'")),
                "{warning}"
            );
        }
        // Near-miss: the real sibling of each stays offered and silent.
        for (provider, real) in [
            ("openai", "o4-mini"),
            ("openai", "gpt-5.5"),
            ("minimax", "MiniMax-M1"),
            ("groq", "llama-3.3-70b-versatile"),
            ("xai", "grok-4.7"),
            ("google", "gemini-3.1-pro-preview"),
            ("google", "gemini-3-flash-preview"),
        ] {
            assert_eq!(crate::cli::unknown_model_warning(provider, real), None);
        }
    }

    /// #1119: providers whose built-in default runs WITHOUT a preset, each
    /// with the reason in user terms. Every other provider's default must
    /// resolve through one (checked by the guessed-limits warning's own
    /// predicate, so this test and that warning agree on what "has a preset"
    /// means). A row whose provider gains a preset fails too, so a reason
    /// cannot outlive its premise (the openrouter "(had no preset then)" shape).
    const DEFAULT_WITHOUT_PRESET: &[(&str, &str)] = &[
        ("ollama", "user-run server: the model is whatever the user pulled, and limits come from that server"),
        ("custom", "user-run server: the user names the model their endpoint serves; no vendor preset can describe it"),
        ("groq", "yoagent ships no preset for any groq id, so no default could resolve through one"),
        ("cerebras", "yoagent ships no preset for any cerebras id, so no default could resolve through one"),
        ("minimax", "yoagent ships no preset for any minimax id, so no default could resolve through one"),
        ("github", "yoagent ships no preset for any github-models id, so no default could resolve through one"),
    ];

    #[test]
    fn every_provider_default_has_a_preset_or_a_stated_reason() {
        use crate::model_limits_note::{runs_on_guessed_limits, USER_RUN_SERVER_PROVIDERS};
        let mut failures = Vec::new();
        for p in KNOWN_PROVIDERS {
            let model = default_model_for_provider(p);
            // The warning's predicate is silent for user-run servers by
            // construction, so they cannot count as "has a preset".
            let has_preset =
                !USER_RUN_SERVER_PROVIDERS.contains(p) && !runs_on_guessed_limits(p, &model);
            let reason = DEFAULT_WITHOUT_PRESET
                .iter()
                .find(|(q, _)| q == p)
                .map(|(_, r)| *r);
            match (has_preset, reason) {
                (true, None) => {}
                (false, Some(r)) if !r.trim().is_empty() => {}
                (false, _) => failures.push(format!(
                    "{p}: default '{model}' has no preset and no stated reason"
                )),
                (true, Some(_)) => failures.push(format!(
                    "{p}: default '{model}' now has a preset; drop its DEFAULT_WITHOUT_PRESET row"
                )),
            }
        }
        // Every table row names a provider the code knows (no dead rows).
        for (q, _) in DEFAULT_WITHOUT_PRESET {
            if !KNOWN_PROVIDERS.contains(q) {
                failures.push(format!("{q}: not in KNOWN_PROVIDERS"));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// #1119: each moved default is a model the provider's own known list
    /// names, so the Unknown-model warning never fires on a no-config run.
    #[test]
    fn every_provider_default_is_a_known_model() {
        // `custom` has no known list: any id is the user's own.
        for p in KNOWN_PROVIDERS.iter().filter(|p| **p != "custom") {
            let model = default_model_for_provider(p);
            assert!(
                model_is_known_for_provider(p, &model),
                "{p}: default '{model}' is not a known model"
            );
        }
    }

    #[test]
    fn unknown_provider_refusal_table() {
        // (provider, base_url_given, Some(expected first line) | None)
        let rows: &[(&str, bool, Option<&str>)] = &[
            (
                "anthropc",
                false,
                Some("unknown provider 'anthropc' — did you mean 'anthropic'?"),
            ),
            (
                "opnai",
                false,
                Some("unknown provider 'opnai' — did you mean 'openai'?"),
            ),
            // Near-miss rows: known names are never refused, with or without a URL.
            ("anthropic", false, None),
            ("custom", false, None),
            ("ollama", false, None),
            ("openai", true, None),
            // A typo plus a base URL is a choice: keep the OpenAI-compatible path.
            ("anthropc", true, None),
            ("zzzz", true, None),
        ];
        for (p, url, want) in rows {
            let got = unknown_provider_refusal(p, *url);
            match want {
                None => assert_eq!(got, None, "{p} url={url} must not be refused"),
                Some(first) => {
                    let msg = got.unwrap_or_else(|| panic!("{p} must be refused"));
                    assert_eq!(
                        msg,
                        format!(
                            "{first}\nfor an OpenAI-compatible server use: \
                             --provider custom --base-url <url>"
                        )
                    );
                }
            }
        }
    }

    #[test]
    fn unknown_provider_far_from_everything_lists_the_known_ones() {
        let msg = unknown_provider_refusal("zzzz", false).expect("zzzz is refused");
        assert!(!msg.contains("did you mean"), "{msg}");
        let first = msg.lines().next().unwrap();
        assert_eq!(
            first,
            format!(
                "unknown provider 'zzzz'. Known providers: {}",
                KNOWN_PROVIDERS.join(", ")
            )
        );
        // Anti-vacuous: the list really is the authority, not an empty join.
        assert!(first.contains("anthropic") && first.contains("custom"));
    }

    #[test]
    fn unknown_provider_refusal_escapes_control_bytes() {
        let msg = unknown_provider_refusal("bad\u{1b}[31m", false).expect("refused");
        assert!("bad\u{1b}[31m".as_bytes().contains(&0x1b));
        assert!(!msg.as_bytes().contains(&0x1b), "{msg:?}");
    }

    #[test]
    fn test_known_providers_has_at_least_10() {
        assert!(
            KNOWN_PROVIDERS.len() >= 10,
            "expected at least 10 providers, got {}",
            KNOWN_PROVIDERS.len()
        );
    }

    #[test]
    fn test_every_provider_has_default_model() {
        for provider in KNOWN_PROVIDERS {
            let model = default_model_for_provider(provider);
            assert!(
                !model.is_empty(),
                "provider '{}' should have a non-empty default model",
                provider
            );
        }
    }

    #[test]
    fn test_every_non_custom_provider_has_known_models() {
        for provider in KNOWN_PROVIDERS {
            if *provider == "custom" || *provider == "openrouter" {
                // custom/openrouter don't have a fixed model list
                continue;
            }
            let models = known_models_for_provider(provider);
            assert!(
                !models.is_empty(),
                "provider '{}' should have at least one known model",
                provider
            );
        }
    }

    #[test]
    fn test_minimax_provider_api_key_env() {
        assert_eq!(provider_api_key_env("minimax"), Some("MINIMAX_API_KEY"));
    }

    #[test]
    fn test_minimax_default_model() {
        // Deliberate pin: MiniMax-M3 is in the suggested list (issue #611) but
        // the default stays M2.7 — changing the default is a separate decision.
        assert_eq!(default_model_for_provider("minimax"), "MiniMax-M2.7");
    }

    #[test]
    fn test_deepseek_flash_is_a_known_model_and_the_default_is_unchanged() {
        // ANTI-VACUOUS, ASSERTED FIRST: the fixture id really is the string the
        // table is keyed on, and the pre-existing control id really does resolve
        // before we touch anything — otherwise every assertion below could pass
        // by both sides agreeing on nothing.
        let models = known_models_for_provider("deepseek");
        assert!(
            models.contains(&"deepseek-v4-pro"),
            "control id must resolve before the fix is measured"
        );

        // The #923 fix: the loop's configured model is no longer "unknown", so
        // the `Unknown model ... Proceeding anyway` warning stops firing on it.
        assert!(
            models.contains(&"deepseek-flash"),
            "deepseek-flash must be a known model or the warning keeps firing"
        );

        // NEAR-MISS GUARD: the pre-existing ids stay listed, whole-value equality.
        assert_eq!(
            models
                .iter()
                .copied()
                .filter(|m| m.starts_with("deepseek-v4"))
                .collect::<Vec<_>>(),
            vec!["deepseek-v4-pro", "deepseek-v4-flash"],
            "the pre-existing deepseek ids must be untouched"
        );

        // The default is deliberately NOT changed — pinned so a later reader has
        // to make the choice out loud rather than drifting it. See the comment on
        // `default_model_for_provider`.
        assert_eq!(default_model_for_provider("deepseek"), "deepseek-v4-pro");
    }

    #[test]
    fn test_minimax_known_models() {
        let models = known_models_for_provider("minimax");
        assert!(!models.is_empty(), "minimax should have known models");
        // Newest flagship first (issue #611)
        assert_eq!(models.first(), Some(&"MiniMax-M3"));
        assert!(models.contains(&"MiniMax-M3"));
        assert!(models.contains(&"MiniMax-M1"));
        // #1147: `MiniMax-M1-40k` is no API id (see IDS_NO_VENDOR_SERVES).
        assert!(!models.contains(&"MiniMax-M1-40k"));
    }

    #[test]
    fn test_bedrock_in_known_providers() {
        assert!(
            KNOWN_PROVIDERS.contains(&"bedrock"),
            "bedrock should be in KNOWN_PROVIDERS"
        );
    }

    #[test]
    fn test_bedrock_provider_api_key_env() {
        assert_eq!(provider_api_key_env("bedrock"), Some("AWS_ACCESS_KEY_ID"));
    }

    /// The anthropic default is a decision with three checks (#1037, Day 230):
    /// it has a preset, the preset is priced, and the `max_tokens` a no-config
    /// run sends fits the model's output maximum. Asserted against whatever the
    /// default is, so moving it to an unchecked id goes red here.
    #[test]
    fn anthropic_default_model_is_preset_priced_and_within_its_output_maximum() {
        let model = default_model_for_provider("anthropic");
        assert_eq!(model, "claude-opus-5-5");
        let preset = crate::agent_builder::anthropic_preset(&model)
            .expect("the anthropic default must have a preset");
        assert!(
            preset.cost.is_some(),
            "the anthropic default must be priced, or /cost reports nothing"
        );
        let max = crate::agent_builder::anthropic_output_maximum(&model)
            .expect("the anthropic default must declare an output maximum");
        assert!(
            preset.max_tokens <= max,
            "a no-config run sends {} max_tokens, above {model}'s {max}",
            preset.max_tokens
        );
        assert!(
            model_is_known_for_provider("anthropic", &model),
            "the default must not trip the unknown-model warning"
        );
    }

    /// The openai default gets the same three checks as the anthropic one
    /// (#1037, Day 231): preset, priced, within its output maximum.
    #[test]
    fn openai_default_model_is_preset_priced_and_within_its_output_maximum() {
        let model = default_model_for_provider("openai");
        assert_eq!(model, "gpt-6-sol");
        let preset = crate::agent_builder::openai_preset(&model)
            .expect("the openai default must have a preset, or it runs on guessed limits (#1037)");
        assert!(
            preset.cost.is_some(),
            "the openai default must be priced, or /cost reports nothing"
        );
        let max =
            crate::agent_builder::openai_compat_output_ceiling("openai", &model, preset.max_tokens)
                .expect("the openai default must declare an output maximum");
        assert!(
            preset.max_tokens <= max,
            "a no-config run sends {} max_tokens, above {model}'s {max}",
            preset.max_tokens
        );
        assert!(
            model_is_known_for_provider("openai", &model),
            "the default must not trip the unknown-model warning"
        );
    }

    /// The google default gets the same three checks (#1021, Day 237):
    /// vendor preset, priced, within the declared output ceiling. Written
    /// over whatever the default is, so moving it to an unchecked id goes red.
    /// `gemini-2.5-flash` (the old default) fails the first check: yoagent
    /// 0.25 has google presets only for `gemini-4-argon` and `gemini-3.8-flash`.
    #[test]
    fn google_default_model_is_preset_priced_and_within_its_output_maximum() {
        let model = default_model_for_provider("google");
        assert_eq!(model, "gemini-3.8-flash");
        let preset = crate::agent_builder::vendor_preset("google", &model)
            .expect("the google default must have a vendor preset, or it runs on guessed limits");
        assert!(
            preset.cost.is_some(),
            "the google default must be priced, or /cost reports an estimate"
        );
        if let Some(max) =
            crate::agent_builder::declared_output_ceiling("google", &model, preset.max_tokens)
        {
            assert!(
                preset.max_tokens <= max,
                "a no-config run sends {} max_tokens, above {model}'s {max}",
                preset.max_tokens
            );
        }
        assert!(
            model_is_known_for_provider("google", &model),
            "the default must not trip the unknown-model warning"
        );
        // Near miss: the old default really does fail check (a), so the move
        // was forced by the checks, not by taste.
        assert!(crate::agent_builder::vendor_preset("google", "gemini-2.5-flash").is_none());
    }

    #[test]
    fn test_bedrock_default_model() {
        assert_eq!(
            default_model_for_provider("bedrock"),
            "anthropic.claude-sonnet-4-6"
        );
    }

    #[test]
    fn test_bedrock_known_models() {
        let models = known_models_for_provider("bedrock");
        assert!(!models.is_empty(), "bedrock should have known models");
        // The default is listed; the retired 2025-05-14 Sonnet 4 is not (#1121).
        assert!(models.contains(&"anthropic.claude-sonnet-4-6"));
        assert!(!models.contains(&"anthropic.claude-sonnet-4-20250514-v1:0"));
        assert!(models.contains(&"amazon.nova-pro-v1:0"));
    }

    #[test]
    fn test_minimax_in_known_providers() {
        assert!(
            KNOWN_PROVIDERS.contains(&"minimax"),
            "minimax should be in KNOWN_PROVIDERS"
        );
    }

    #[test]
    fn test_openai_known_models_includes_gpt5() {
        let models = known_models_for_provider("openai");
        assert!(models.contains(&"gpt-5"), "openai should include gpt-5");
        assert!(
            models.contains(&"gpt-5-mini"),
            "openai should include gpt-5-mini"
        );
        assert!(models.contains(&"gpt-5.5"), "openai should include gpt-5.5");
        // #1147: OpenAI ships no gpt-5.5-mini (see IDS_NO_VENDOR_SERVES).
        assert!(!models.contains(&"gpt-5.5-mini"));
    }

    #[test]
    fn test_xai_known_models_includes_grok4() {
        let models = known_models_for_provider("xai");
        assert!(models.contains(&"grok-4"), "xai should include grok-4");
    }

    #[test]
    fn test_google_known_models_includes_flash_lite() {
        let models = known_models_for_provider("google");
        assert!(
            models.contains(&"gemini-2.5-flash-lite"),
            "google should include gemini-2.5-flash-lite"
        );
    }

    #[test]
    fn test_anthropic_known_models_includes_fleet_models() {
        // yoagent 0.9 fleet presets (issue #568)
        let models = known_models_for_provider("anthropic");
        for fleet in ["claude-fable-5", "claude-opus-4-8", "claude-sonnet-5"] {
            assert!(
                models.contains(&fleet),
                "anthropic should include fleet model {fleet}"
            );
        }
    }

    #[test]
    fn test_anthropic_known_models_includes_new_variants() {
        let models = known_models_for_provider("anthropic");
        assert!(
            models.contains(&"claude-opus-4-7"),
            "anthropic should include claude-opus-4-7"
        );
        assert!(
            models.contains(&"claude-sonnet-4-6"),
            "anthropic should include claude-sonnet-4-6"
        );
        // Sonnet 4.5 retires 2026-11-30 and yoagent has no row for it (#1144).
        assert!(
            !models.contains(&"claude-sonnet-4-5"),
            "anthropic must no longer offer claude-sonnet-4-5"
        );
        assert!(
            models.contains(&"claude-haiku-4-5"),
            "anthropic should include claude-haiku-4-5"
        );
        // Older models still present
        assert!(
            models.contains(&"claude-opus-4-6"),
            "anthropic should still include claude-opus-4-6"
        );
        // The 2025-05-14 Sonnet 4 snapshot retired 2026-06-15 (#1144).
        assert!(
            !models.contains(&"claude-sonnet-4-20250514"),
            "anthropic must no longer offer claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_bedrock_known_models_includes_new_variants() {
        let models = known_models_for_provider("bedrock");
        assert!(
            models.contains(&"anthropic.claude-opus-4-7"),
            "bedrock should include anthropic.claude-opus-4-7"
        );
        assert!(
            models.contains(&"anthropic.claude-sonnet-4-6"),
            "bedrock should include anthropic.claude-sonnet-4-6"
        );
    }

    #[test]
    fn test_openrouter_default_uses_dotted_sonnet_5_5() {
        assert_eq!(
            default_model_for_provider("openrouter"),
            "anthropic/claude-sonnet-5.5"
        );
    }

    #[test]
    fn test_anthropic_known_models_no_longer_offers_retired_sonnet_4_20250514() {
        let models = known_models_for_provider("anthropic");
        assert!(
            !models.is_empty(),
            "anthropic should have a non-empty known models list"
        );
        assert!(
            !models.contains(&"claude-sonnet-4-20250514"),
            "anthropic known models must not offer the retired claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_unknown_provider_returns_empty_known_models() {
        let models = known_models_for_provider("nonexistent-provider");
        assert!(
            models.is_empty(),
            "unknown provider should return empty known models list"
        );
    }

    #[test]
    fn test_github_in_known_providers() {
        assert!(
            KNOWN_PROVIDERS.contains(&"github"),
            "github should be in KNOWN_PROVIDERS"
        );
    }

    #[test]
    fn test_github_provider_api_key_env() {
        assert_eq!(provider_api_key_env("github"), Some("GITHUB_TOKEN"));
    }

    #[test]
    fn test_github_default_model() {
        assert_eq!(default_model_for_provider("github"), "openai/gpt-4o");
    }

    #[test]
    fn test_github_known_models() {
        let models = known_models_for_provider("github");
        assert!(!models.is_empty(), "github should have known models");
        assert!(models.contains(&"openai/gpt-4o"));
        assert!(models.contains(&"anthropic/claude-sonnet-4-6"));
    }
}

#[cfg(test)]
#[path = "google_limits_tests.rs"]
mod google_limits_tests;

#[cfg(test)]
#[path = "providers_key_tests.rs"]
mod providers_key_tests;

#[cfg(test)]
#[path = "openrouter_preset_tests.rs"]
mod openrouter_preset_tests;
