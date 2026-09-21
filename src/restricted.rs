//! The `--restricted` switch's **decision seam** (#879 question 5): one place
//! that answers "is this run restricted?" from the flag **and** the
//! environment.
//!
//! Why a separate module rather than four more functions in `src/cli.rs`: the
//! env half is what makes the composite switch reachable from a **wrapper
//! script**, and it arrives with its own table, its own scoped env read and its
//! own "one door" source guard. `src/cli.rs` sits at its register entry with no
//! room under the 100-line drift band, and the gate's own remedy for that is to
//! move new code to a smaller module rather than to paste a larger number. The
//! seam shape is copied from `src/prompt_budget.rs::resolve_cost_threshold`
//! (flag → env → config, resolved once by a pure function whose call site lives
//! in `cli.rs`) rather than invented here.
//!
//! **What this does not do:** it does not decide *what* restriction means. Clause
//! A (safe mode), clause B (the directory fence, in `restricted_mode_effects`)
//! and clause D (`RESTRICTED_REMOVED_TOOLS`) all live in `src/cli.rs` and are
//! unchanged; this module only decides whether they fire.

/// The environment variable that turns `--restricted` on without the flag, so the
/// composite switch is reachable from a **wrapper script** — the actual use case
/// #879 question 5 names. (Claude Code ships `CLAUDE_CODE_RESTRICTED=1` for the
/// same reason.)
pub(crate) const RESTRICTED_ENV_VAR: &str = "YOYO_RESTRICTED";

/// The accepted spellings of [`RESTRICTED_ENV_VAR`] that mean **on** — exactly `1`
/// and `true`, trimmed and case-insensitive. **Every** other value reads off:
/// `0`, `false`, `""`, and nonsense like `yes-please` are all off, and an unset
/// variable is off. Nonsense never means "on with a value I did not understand",
/// which is the `parse_cost_threshold` rule — a value the user mistyped must not
/// silently change the session's confinement on the first run. The set is stated
/// here rather than left to be inferred from the match arms.
pub(crate) fn env_flag_is_on(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1") | Some("true")
    )
}

/// Read the environment source once, at the single call site in
/// `cli::parse_args`.
///
/// Separated out so the env read is testable through a **scoped guard** rather
/// than by mutating the process environment and walking away (Day 190: every
/// class that has eaten my sessions lives in the test's *setup*). Reading here
/// rather than inside [`restricted_from_sources`] keeps that function pure.
pub(crate) fn restricted_env_source() -> Option<String> {
    std::env::var(RESTRICTED_ENV_VAR).ok()
}

/// The single decision for "is this run restricted?" — the flag and the
/// environment, OR-ed. **This is the only place the boolean is decided**, so there
/// is one policy rather than two doors that happen to agree today.
///
/// **Monotonic:** the environment can only ever turn restricted **on**. A `true`
/// flag is never turned off by the environment, and a value the environment does
/// not accept is *absent*, not a contradiction. Pure — the env value arrives as a
/// parameter — which is also why the semantics are table-testable without any
/// process-environment mutation.
pub(crate) fn restricted_from_sources(flag: bool, env: Option<&str>) -> bool {
    flag || env_flag_is_on(env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// A scoped env mutation. Day 190's rule is that every class that has eaten
    /// my sessions lives in the test's *setup*, so this restores the variable it
    /// touched on the way out instead of mutating the process environment and
    /// walking away — a leaked `YOYO_RESTRICTED=1` would silently confine every
    /// test that runs after it in this binary.
    struct EnvScope(&'static str, Option<String>);

    impl EnvScope {
        fn set(name: &'static str, value: &str) -> Self {
            let prev = std::env::var(name).ok();
            std::env::set_var(name, value);
            EnvScope(name, prev)
        }
    }

    impl Drop for EnvScope {
        fn drop(&mut self) {
            match self.1.take() {
                Some(prev) => std::env::set_var(self.0, prev),
                None => std::env::remove_var(self.0),
            }
        }
    }

    #[test]
    fn restricted_from_sources_table() {
        // The whole decision, as a table. `env` arrives as a parameter, so none
        // of these rows touch the process environment.
        let cases: &[(bool, Option<&str>, bool, &str)] = &[
            (false, None, false, "neither source: off is the default"),
            (
                false,
                Some(""),
                false,
                "empty is off, never 'on with no value'",
            ),
            (false, Some("0"), false, "0 is off"),
            (false, Some("false"), false, "false is off"),
            (
                false,
                Some("FALSE"),
                false,
                "false is off, case-insensitively",
            ),
            (
                false,
                Some("yes-please"),
                false,
                "nonsense must not alarm: an unparseable value is off, not on",
            ),
            (false, Some("1"), true, "1 turns it on"),
            (false, Some("true"), true, "true turns it on"),
            (false, Some("TRUE"), true, "case-insensitive"),
            (false, Some(" 1 "), true, "trimmed"),
            (false, Some(" True "), true, "trimmed and case-insensitive"),
            (true, None, true, "the flag alone"),
            (true, Some("1"), true, "both sources agree"),
            (
                true,
                Some("0"),
                true,
                "MONOTONIC: the env var may never turn the flag off",
            ),
            (
                true,
                Some("no"),
                true,
                "MONOTONIC: nor may an unparseable value",
            ),
        ];
        for (flag, env, want, why) in cases {
            assert_eq!(
                restricted_from_sources(*flag, *env),
                *want,
                "flag={flag} env={env:?}: {why}"
            );
        }

        // Anti-vacuous: the table must contain at least one row of each polarity,
        // or a constant-folded function would pass it.
        assert!(
            cases.iter().any(|(_, _, want, _)| *want) && cases.iter().any(|(_, _, want, _)| !*want),
            "the table must exercise both outcomes"
        );
    }

    #[test]
    fn env_flag_is_on_accepts_exactly_one_and_true() {
        // The accepted set is `{1, true}` and nothing else — stated so that
        // widening it is a deliberate edit rather than an accident.
        for on in ["1", "true", "TRUE", "True", "1 ", " true "] {
            assert!(env_flag_is_on(Some(on)), "{on:?} must read as on");
        }
        for off in [
            "",
            "0",
            "false",
            "no",
            "yes",
            "on",
            "enabled",
            "2",
            "t",
            "y",
            "-1",
            "1.0",
            "yes-please",
        ] {
            assert!(!env_flag_is_on(Some(off)), "{off:?} must read as off");
        }
        assert!(!env_flag_is_on(None), "absent must read as off");
    }

    /// The env read itself, and the one thing a pure table cannot prove: that
    /// [`restricted_env_source`] reads *this* variable. Scoped, so the variable is
    /// restored even if the assertion fails.
    #[test]
    #[serial]
    fn restricted_env_source_reads_the_named_variable() {
        let guard = EnvScope::set(RESTRICTED_ENV_VAR, "1");
        assert_eq!(restricted_env_source().as_deref(), Some("1"));
        // And it flows through the one decision function unchanged.
        assert!(restricted_from_sources(
            false,
            restricted_env_source().as_deref()
        ));

        // Drop the guard early so the removal branch is the one this test's
        // second half exercises, rather than leaving it to the next test.
        drop(guard);
        assert_eq!(
            restricted_env_source(),
            None,
            "the scoped guard must restore the variable it touched"
        );
    }

    /// The **near-miss** half of the scoped guard: an unrelated, unset variable
    /// must not be read as though it were the restricted knob, and a leftover
    /// value must not survive the guard. Asserted as whole values rather than a
    /// `contains`, because a partial assertion is a green light over the
    /// fragment it does not inspect.
    #[test]
    #[serial]
    fn env_scope_restores_the_previous_value_and_reads_only_its_own_name() {
        // Pre-seed a value, set another through the guard, and prove the
        // restore put the *previous* value back rather than deleting the name.
        std::env::set_var(RESTRICTED_ENV_VAR, "SEED");
        {
            let _guard = EnvScope::set(RESTRICTED_ENV_VAR, "true");
            assert_eq!(restricted_env_source().as_deref(), Some("true"));
        }
        assert_eq!(restricted_env_source().as_deref(), Some("SEED"));
        // Leave the process as we found it.
        std::env::remove_var(RESTRICTED_ENV_VAR);
        assert_eq!(restricted_env_source(), None);
    }

    #[test]
    fn restricted_env_var_name_is_pinned() {
        // Wrapper scripts and docs name this string. Pinned by value so a rename
        // cannot silently break a caller that is not in this repo.
        assert_eq!(RESTRICTED_ENV_VAR, "YOYO_RESTRICTED");
    }

    #[test]
    fn restricted_env_var_is_documented_in_help() {
        // #745/#767/#769 were all "a live switch an operator cannot discover".
        let help = crate::help::cli_help_text();
        assert!(
            help.contains("YOYO_RESTRICTED"),
            "--help must document YOYO_RESTRICTED, in BOTH the flag paragraph \
             and the Environment section"
        );
        // The accepted spelling, not just the name.
        assert!(
            help.contains("YOYO_RESTRICTED=1"),
            "--help must show the accepted spelling next to --restricted"
        );
    }

    /// The "two doors, one policy, one deaf" guard, aimed at the caller.
    ///
    /// Deliberately a **source-level** check, and it proves only what a source
    /// scan can: that the production half of `cli.rs` consults both sources and
    /// routes them through the one decision function. It cannot prove the env var
    /// was read at runtime — [`restricted_env_source_reads_the_named_variable`]
    /// covers that half.
    #[test]
    fn only_one_place_decides_whether_the_run_is_restricted() {
        let src = include_str!("cli.rs");
        let (production, _tests) = src
            .split_once("#[cfg(test)]")
            .expect("src/cli.rs must keep its test module at the end");

        let call_sites: Vec<&str> = production.matches("restricted_from_sources(").collect();
        assert_eq!(
            call_sites.len(),
            1,
            "exactly one production call site decides the restricted boolean; \
             a second one would be the two-doors shape this repo has shipped six times"
        );

        // The call site must consult BOTH sources, or the env half is wired to
        // nothing rather than to the flag.
        let call = production
            .split("restricted_from_sources(")
            .nth(1)
            .expect("a call site exists");
        assert!(
            call.contains("--restricted"),
            "the call site must still scan argv for --restricted"
        );
        assert!(
            call.contains("restricted_env_source()"),
            "the call site must read the env source through its named reader"
        );
        assert_eq!(
            production.matches("restricted_env_source()").count(),
            1,
            "the env var must be read once per process — a second read could \
             shift the fence mid-session"
        );
        // The variable NAME has one authority: the const. Any other production
        // occurrence of the literal would be a second door onto it.
        assert_eq!(
            production.matches("\"YOYO_RESTRICTED\"").count(),
            0,
            "cli.rs must never spell the variable name: read RESTRICTED_ENV_VAR \
             through this module, so the name has exactly one authority"
        );
    }
}
