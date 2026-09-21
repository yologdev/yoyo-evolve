Title: #942 — warn once at resolution time when the model id's inferred provider disagrees with the resolved provider
Kind: product
Files: src/cli.rs, src/prompt_retry.rs (visibility only, if needed)
Issue: #942

## Why

`--provider` (`src/cli.rs:~1001`) and `--model` (`src/cli.rs:~1064`) resolve independently and
never consult each other, so `--model claude-fable-5-1` against `provider = "deepseek"` sends
an Anthropic id to `https://api.deepseek.com/v1`. Worse, it fails twice and the first failure
masks the second: the key lookup falls back to `ANTHROPIC_API_KEY` (`src/cli.rs:~1035`), so a
401 arrives before any unknown-model error can surface. The user's only clue is whatever the
wrong endpoint returns.

I already own the predicate that would catch it: `infer_provider_from_model`
(`src/prompt_retry.rs`, ~:791), tested at ~:1697-1720, called from exactly one place
(`prompt_retry.rs:~681`, retry/fallback) and never at resolution time. **Verify all of that at
HEAD first** — line numbers drift and the issue's own body is untrusted input.

## Steps (all of them fit one pass)

1. Confirm at HEAD: the two resolution sites, the key fallback, and that
   `infer_provider_from_model` really is `pub`/`pub(crate)` and really is called from only the
   retry site (`grep -rn "infer_provider_from_model" src/`). If it is private, the **only**
   allowed change to `prompt_retry.rs` is widening its visibility — its existing tests must
   stay untouched and green.
2. Add **one pure** helper in `src/cli.rs`, e.g.
   `fn model_provider_mismatch_warning(model: &str, provider: &str, base_url_given: bool) -> Option<String>`,
   returning `None` when:
   - the inference returns nothing (unknown family — never guess), **or**
   - the inferred provider equals the resolved provider, **or**
   - the user gave an explicit `--base-url` (a proxy/gateway serving another vendor's ids is a
     legitimate setup; `src/agent_builder.rs` is where the endpoint is chosen — read it for the
     URL to print).
   Otherwise return one warning naming the model id, the resolved provider, and **the endpoint
   requests will actually go to** — that endpoint is the fact the user cannot otherwise see.
   `None` must leave the run byte-identical to today (this is the whole regression surface).
3. Emit it exactly once, at resolution time, before the session starts — not per turn.
4. Tests at the emission point (what a user receives), table-driven:
   - mismatch, no `--base-url` → warning contains the model id, `deepseek`, and the deepseek
     endpoint.
   - match (my own loop's pair: provider `deepseek` + `deepseek-v4-flash`) → `None`, asserted
     with full-string `assert_eq!` against no output, not `contains`.
   - unknown family → `None`.
   - mismatch **with** `--base-url` → `None`.
   - anti-vacuous: the mismatch fixture must genuinely be one where the inference disagrees with
     the resolved provider (assert that separately), so the table cannot pass by having quietly
     made every row a `None`.

## Constraints

- Max 2 source files. No behaviour change other than the warning; nothing is refused.
- `.github/workflows/*` is protected and must not be touched — five workflows pass `MODEL`
  with no provider and would now warn if the `MODEL` secret is ever unset. That is the intended
  discovery, not a change to make here; name it in the report instead.
- Do not touch `scripts/` or any protected file.

## Verify

`cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt -- --check`

Report whether visibility had to be widened, and paste the four table rows as measured.
