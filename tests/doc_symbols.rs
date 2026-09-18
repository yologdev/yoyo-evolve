//! Doc-symbol freshness gate: every backticked Rust-shaped symbol named in
//! `CLAUDE.md` must still occur somewhere under `src/`.
//!
//! Why this exists. `CLAUDE.md` is appended to the system prompt of **every**
//! invocation of **every** loop (`load_project_context`, `src/cli.rs`), so a
//! symbol it names in backticks reads as authoritative to every future session.
//! When the code is renamed and the prose is not, nothing fails: prose has no
//! compiler. The canonical instance is the `src/gasp.rs` "unreachable" claim,
//! which cost eight sessions (#763, #765, #782, #785, #787, #789, #803) — its
//! own gate now lives in `tests/doc_version_claims.rs`. This is the same defect
//! one level up: not a claim about a *dependency*, but a *name* in the file
//! every session reads first.
//!
//! This discharges #918, which shipped **accepted UNVERIFIED** — a census that
//! produced no artifact. Per the Day-200 lesson, *a missing test leaves the tree
//! exactly as green as a written one*, so the artifact is **the gate**, not
//! another count.
//!
//! # Scope is `CLAUDE.md` ONLY, and `ARCHITECTURE.md` is deliberately excluded
//!
//! The asymmetry is the design, not an oversight:
//!
//! * `CLAUDE.md` is prepended to every prompt of every loop, so a stale symbol
//!   there is read as authoritative by every future session. That is the whole
//!   failure mode.
//! * `ARCHITECTURE.md`'s job is to **record superseded claims**, so stale names
//!   there are the *content*, not a defect. A gate pointed at it would fire on
//!   the file doing its job and train the next reader to paste past it.
//!
//! `docs/` and the skills are out of scope for the same reason: each needs its
//! own decision about what "stale" means in a file whose purpose is
//! record-keeping.
//!
//! # The census, dated 2026-09-18, re-derived on this tree rather than quoted
//!
//! 302 backticked tokens; **81 unique** Rust-shaped candidates (96 fn-shaped
//! occurrences + 38 type-shaped); **10 absent from `src/`**, each excluded by
//! *category* rather than by a per-name verdict:
//!
//! | category | symbols |
//! |---|---|
//! | yoagent API | `CompactionStrategy`, `on_after_turn`, `on_error` |
//! | git commit sha | `cb9d9b0`, `d93e4f65` |
//! | sabotage marker word | `NEUT-ERED`, `SABO-TAGE` |
//! | const defined in `tests/` | `REGISTERED_MARKERS` |
//! | JSON field name (wire contract) | `deeper_question` |
//! | rustc/clippy lint name | `unreachable_code` |
//!
//! So the honest headline is: **`CLAUDE.md` currently carries zero stale symbol
//! claims; 10 candidates are excluded by category, each named.** If a future
//! run surfaces a real stale name, that census is the finding and **fixing the
//! prose is a follow-up task** — a gate that also rewrites prose is how a
//! finished task reverts.
//!
//! # Shape
//!
//! Copied from the four deterministic gates already in `tests/` rather than
//! invented: a pure, table-tested classifier with all filesystem walking at a
//! single call site; fatal on the *unnamed* case with both remedies verbatim;
//! a register for deliberate exceptions; and a **ratchet in the opposite
//! direction** — a registered name that now *exists* in `src/` is fatal too,
//! because an exception list only pays down if improving is also a failure.
//!
//! # What this CANNOT do — said out loud, so "could not check" never reads as
//! "checked; clean"
//!
//! 1. **It is a text scan, not a Rust parser.** Presence is a word-boundary
//!    substring match, so an occurrence inside a comment or a string literal
//!    counts exactly as a live item does.
//! 2. **Matching is by name, never by resolved path.** A symbol renamed to a
//!    name that happens to exist elsewhere in `src/` passes — the same limit
//!    `tests/orphan_modules.rs` states about its own scan.
//! 3. **Presence is all this proves.** A sentence can name a live symbol and
//!    still be flatly wrong; this gate has no opinion about meaning.
//! 4. A line whose backticks do not pair is skipped rather than mis-paired (the
//!    four code-fence lines today).
//! 5. It scans `CLAUDE.md` only, against `src/` only — see the scope note above.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Category: a name that lives in the **yoagent** crate, not in yoyo.
const CAT_YOAGENT: &str = "yoagent API";
/// Category: a hex string that is a git commit sha, not an identifier.
const CAT_SHA: &str = "git commit sha";
/// Category: a string literal a gate greps for, not a Rust identifier.
const CAT_MARKER: &str = "sabotage marker word";
/// Category: an item that lives in `tests/`, out of this gate's scope.
const CAT_TEST_ONLY: &str = "const defined in tests/, not src/";
/// Category: a JSON key in a wire contract the model is told to read.
const CAT_WIRE: &str = "JSON field name (wire contract)";
/// Category: a diagnostic name emitted by rustc/clippy, not an item.
const CAT_LINT: &str = "rustc/clippy lint name";

/// The closed set a register entry's category must come from, so a typo'd
/// category is fatal instead of silently making one entry look unlike its
/// siblings. Every member has at least one entry below.
const KNOWN_CATEGORIES: &[&str] = &[
    CAT_YOAGENT,
    CAT_SHA,
    CAT_MARKER,
    CAT_TEST_ONLY,
    CAT_WIRE,
    CAT_LINT,
];

/// Symbols named in backticks in `CLAUDE.md` that are **correctly** absent from
/// `src/`, as `(symbol, category, why)`.
///
/// **Organised by category, and that is the point.** The reason on every entry
/// is a statement about a whole *class* of token — what makes this token not a
/// Rust item of yoyo's — never a per-name verdict. Ten entries reading "known"
/// would be a list; six categories are a rule.
///
/// Keyed by *name* rather than by category for one mechanical reason: the
/// ratchet below has to name the single symbol that became live, and a
/// category key cannot say which one it was.
///
/// **Debt, not absolution.** An entry records that a name in the file every
/// session reads cannot live in `src/`; the ratchet is what stops it becoming
/// permission. It can only shrink.
///
/// The two marker words are assembled with `concat!` because
/// `tests/neutered_guards.rs` scans this file's own text for them and would
/// otherwise (correctly) flag this register as a sabotage marker.
const REGISTERED_DOC_SYMBOLS: &[(&str, &str, &str)] = &[
    (
        "CompactionStrategy",
        CAT_YOAGENT,
        "a yoagent API name: CLAUDE.md is correct to cite it and yoyo does not \
         re-export it, so it lives in the yoagent crate rather than in src/.",
    ),
    (
        "on_after_turn",
        CAT_YOAGENT,
        "a yoagent Agent-builder callback name — an API of the dependency, not an \
         item of this crate.",
    ),
    (
        "on_error",
        CAT_YOAGENT,
        "a yoagent Agent-builder callback name — an API of the dependency, not an \
         item of this crate.",
    ),
    (
        "cb9d9b0",
        CAT_SHA,
        "a git commit sha cited as history: a hex string that happens to match the \
         snake_case shape, and a commit id cannot be an item in src/.",
    ),
    (
        "d93e4f65",
        CAT_SHA,
        "a git commit sha cited as history: a hex string that happens to match the \
         snake_case shape, and a commit id cannot be an item in src/.",
    ),
    (
        concat!("NEUT", "ERED"),
        CAT_MARKER,
        "a string literal that tests/neutered_guards.rs greps every source line \
         for — a marker word, not a Rust identifier.",
    ),
    (
        concat!("SABO", "TAGE"),
        CAT_MARKER,
        "a string literal that tests/neutered_guards.rs greps every source line \
         for — a marker word, not a Rust identifier.",
    ),
    (
        "REGISTERED_MARKERS",
        CAT_TEST_ONLY,
        "a const defined in tests/neutered_guards.rs; this gate's scope is src/ \
         as the stated subject of every session, so a name defined only in \
         tests/ is out of scope by construction.",
    ),
    (
        "deeper_question",
        CAT_WIRE,
        "a JSON field name in the sub-agent contract — a wire key the model is \
         told to read in a sub-agent's reply, not a Rust item.",
    ),
    (
        "unreachable_code",
        CAT_LINT,
        "a rustc/clippy lint name: a diagnostic string the compiler emits, not an \
         item of this crate.",
    ),
];

/// One sighting of a Rust-shaped symbol in backticks: the token and the
/// **1-based** line of `CLAUDE.md` it sits on, so a failure message reads the
/// same as an editor's gutter.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mention {
    symbol: String,
    line: usize,
}

/// A way this gate can be violated. Seven values running in **two opposite
/// directions** — the same two-direction discipline every sibling gate uses.
#[derive(Debug, PartialEq, Eq)]
enum DocSymbolViolation {
    /// The walk over `src/` found no files. A scanner that inspects nothing and
    /// passes is the vacuous-green shape, so this is fatal rather than quiet.
    EmptySourceScan,
    /// The extraction found no candidates in `CLAUDE.md`. If the file stopped
    /// carrying backticked symbols, that is a finding, not a clean scan.
    EmptyCandidateScan,
    /// A backticked symbol named in `CLAUDE.md` that occurs nowhere under
    /// `src/` and that nobody registered. The defect this gate exists for.
    Unregistered {
        symbol: String,
        line: usize,
        mention_count: usize,
    },
    /// Ratchet: a registered name that now occurs under `src/` — the exclusion
    /// is no longer paid for, or the name came back.
    RegisteredButLive { symbol: String, category: String },
    /// Ratchet: a registered name `CLAUDE.md` no longer names at all.
    RegisteredSymbolVanished { symbol: String, reason: String },
    /// A register entry whose reason is empty or whitespace-only. An unnamed
    /// debt wearing a name is not a name.
    EmptyReason { symbol: String },
    /// A register entry whose category is not in `KNOWN_CATEGORIES`.
    UnknownCategory { symbol: String, category: String },
}

impl DocSymbolViolation {
    fn message(&self) -> String {
        match self {
            DocSymbolViolation::EmptySourceScan => "the walk over src/ found zero *.rs files — the walk is broken, not the \
                 repo. A scanner that inspects nothing and passes is exactly the vacuous green \
                 this gate exists to refuse."
                .to_string(),
            DocSymbolViolation::EmptyCandidateScan => "the scan of CLAUDE.md found zero Rust-shaped backticked symbols — the \
                 extraction is broken, or the file changed shape. Do not read this as a clean \
                 result: a scanner with nothing to scan passes for the wrong reason."
                .to_string(),
            DocSymbolViolation::Unregistered {
                symbol,
                line,
                mention_count,
            } => format!(
                "CLAUDE.md:{line} names `{symbol}` in backticks ({mention_count} mention(s) in \
                 the file), but no occurrence of that identifier exists anywhere under src/.\n     \
                 CLAUDE.md is prepended to the system prompt of every invocation of every loop, \
                 so a stale name here is read as authoritative by every future session — the \
                 src/gasp.rs \"unreachable\" claim that cost eight sessions is the canonical case.\n     \
                 Fix (if the prose is stale): correct or delete the sentence that names it. The \
                 code is the source of truth; prose has no compiler.\n     \
                 Fix (only if the absence from src/ is deliberate): add\n       \
                 (\"{symbol}\", \"<category>\", \"<why this name cannot live in src/>\"),\n     \
                 to REGISTERED_DOC_SYMBOLS in tests/doc_symbols.rs, where <category> is one of \
                 the categories named in that file. The gate does not forbid a symbol that \
                 lives outside src/. It forbids an unnamed one."
            ),
            DocSymbolViolation::RegisteredButLive { symbol, category } => format!(
                "REGISTERED_DOC_SYMBOLS lists `{symbol}` (category: {category}) as correctly \
                 absent from src/, but `{symbol}` now occurs under src/ — the exclusion is no \
                 longer paid for.\n     \
                 Fix: delete its entry from REGISTERED_DOC_SYMBOLS in tests/doc_symbols.rs, so \
                 the prose naming it is guarded by this gate again. Fatal on purpose: a register \
                 only ratchets down if a repair is also a failure."
            ),
            DocSymbolViolation::RegisteredSymbolVanished { symbol, reason } => format!(
                "REGISTERED_DOC_SYMBOLS lists `{symbol}` (\"{reason}\"), but CLAUDE.md no longer \
                 names it in backticks — the entry now excludes nothing.\n     \
                 Fix: delete its entry from REGISTERED_DOC_SYMBOLS in tests/doc_symbols.rs, or \
                 restore the mention if a prose edit dropped it by accident."
            ),
            DocSymbolViolation::EmptyReason { symbol } => format!(
                "the REGISTERED_DOC_SYMBOLS entry for `{symbol}` has an empty reason.\n     \
                 Fix: write why that name cannot live in src/, or delete the entry. An unnamed \
                 debt wearing a name is not a name — the reason is the only part of the entry a \
                 human can act on."
            ),
            DocSymbolViolation::UnknownCategory { symbol, category } => format!(
                "the REGISTERED_DOC_SYMBOLS entry for `{symbol}` carries the category \
                 \"{category}\", which is not one of the known categories.\n     \
                 Fix: use one of {KNOWN_CATEGORIES:?} from tests/doc_symbols.rs, or add the new \
                 category to KNOWN_CATEGORIES in the same edit. The category is what makes the \
                 reason a rule about a class of token rather than a verdict about one name."
            ),
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {} — {e}", path.display()))
}

/// Fn-shaped (`[a-z_][a-z0-9_]{3,}`) or type-shaped (`[A-Z][A-Za-z0-9_]{2,}`):
/// the two shapes `CLAUDE.md` writes when it names a Rust item.
fn is_rust_symbol_shaped(token: &str) -> bool {
    let b = token.as_bytes();
    if b.len() >= 4 && (b[0].is_ascii_lowercase() || b[0] == b'_') {
        return b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_');
    }
    if b.len() >= 3 && b[0].is_ascii_uppercase() {
        return b[1..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_');
    }
    false
}

/// Every backticked Rust-shaped token in `CLAUDE.md`, with its 1-based line.
///
/// Pure: the caller supplies the text, so this is table-testable without
/// touching the filesystem.
///
/// A token spanning a newline cannot occur (lines are paired independently),
/// and a line whose backtick count is odd is **skipped rather than mis-paired**
/// — the four code-fence lines today, where an unpaired tick would otherwise
/// shift every following pair on that line by one and invent tokens.
fn rust_symbol_mentions(md: &str) -> Vec<Mention> {
    let mut out = Vec::new();
    for (idx, line) in md.lines().enumerate() {
        if line.matches('`').count() % 2 != 0 {
            continue;
        }
        let mut parts = line.split('`');
        parts.next(); // text before the first tick
        let mut inside_code = true;
        for part in parts {
            if inside_code && is_rust_symbol_shaped(part) {
                out.push(Mention {
                    symbol: part.to_string(),
                    line: idx + 1,
                });
            }
            inside_code = !inside_code;
        }
    }
    out
}

/// One entry per distinct symbol, keeping the **first** mention's line so a
/// message points at the earliest place a reader would fix.
fn unique_mentions(mentions: &[Mention]) -> Vec<Mention> {
    let mut out: Vec<Mention> = Vec::new();
    for m in mentions {
        if !out.iter().any(|o| o.symbol == m.symbol) {
            out.push(m.clone());
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Word-boundary occurrence test for a bare identifier, which is also what
/// makes `::`-qualified (`crate::safety::redact_secrets`) and `()`-suffixed
/// (`redact_secrets(s)`) forms count: the bytes around the needle are `:`/`(`
/// rather than identifier characters, so they are boundaries too.
///
/// Deliberately ASCII: a non-ASCII byte adjacent to the needle (the tail byte
/// of a multi-byte character) is not an identifier byte, so it is a boundary.
fn identifier_occurs(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    for (idx, _) in haystack.match_indices(needle) {
        let before_ok = idx == 0 || !is_ident_byte(bytes[idx - 1]);
        let end = idx + needle.len();
        let after_ok = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Pure checker: given every backticked mention, every `src/` file as
/// `(relative path, text)`, and the register, report every violation.
///
/// No I/O, so every fatal branch is provable against **fabricated** inputs
/// rather than by renaming a real symbol in `src/`.
fn classify(
    mentions: &[Mention],
    src_files: &[(String, String)],
    register: &[(&str, &str, &str)],
) -> Vec<DocSymbolViolation> {
    if src_files.is_empty() {
        return vec![DocSymbolViolation::EmptySourceScan];
    }
    if mentions.is_empty() {
        return vec![DocSymbolViolation::EmptyCandidateScan];
    }

    let src = src_files
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let mut violations = Vec::new();

    for m in unique_mentions(mentions) {
        let live = identifier_occurs(&src, &m.symbol);
        let entry = register.iter().find(|(name, _, _)| *name == m.symbol);
        match (live, entry) {
            (false, None) => {
                let mention_count = mentions.iter().filter(|x| x.symbol == m.symbol).count();
                violations.push(DocSymbolViolation::Unregistered {
                    symbol: m.symbol.clone(),
                    line: m.line,
                    mention_count,
                });
            }
            (true, Some((_, category, _))) => {
                violations.push(DocSymbolViolation::RegisteredButLive {
                    symbol: m.symbol.clone(),
                    category: (*category).to_string(),
                });
            }
            (false, Some(_)) | (true, None) => {}
        }
    }

    for (symbol, category, reason) in register {
        if !KNOWN_CATEGORIES.contains(category) {
            violations.push(DocSymbolViolation::UnknownCategory {
                symbol: (*symbol).to_string(),
                category: (*category).to_string(),
            });
        }
        if reason.trim().is_empty() {
            violations.push(DocSymbolViolation::EmptyReason {
                symbol: (*symbol).to_string(),
            });
        }
        if !mentions.iter().any(|m| m.symbol == *symbol) {
            violations.push(DocSymbolViolation::RegisteredSymbolVanished {
                symbol: (*symbol).to_string(),
                reason: (*reason).to_string(),
            });
        }
    }

    violations
}

/// Every `*.rs` under `src/`, **recursively**, as `(relative path, text)`.
///
/// Recursive on purpose: `src/format/` is a real subdirectory, and the first
/// version of this census walked only the top level — a symbol living there
/// would have read as stale.
fn collect_src_files(dir: &Path, root: &Path) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    collect_rs_paths(dir, &mut paths);
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("could not read {} — {e}", path.display()));
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push((rel, text));
    }
    out
}

fn collect_rs_paths(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_paths(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Say out loud what the gate did **not** check, through a raw stderr handle
/// rather than `eprintln!` — libtest's capture hook discards macro output from
/// *passing* tests, and a limit disclosure that only prints on failure is a
/// disclosure nobody reads.
fn write_limits(symbols: usize, mentions: usize, src_files: usize, registered: usize) {
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "\ndoc-symbol freshness gate: {mentions} backticked Rust-shaped mention(s) in \
         CLAUDE.md, {symbols} unique candidate(s), checked against {src_files} file(s) under \
         src/; {registered} candidate(s) registered as correctly absent.\n\
         Scope: CLAUDE.md only. ARCHITECTURE.md, docs/ and the skills are deliberately NOT \
         scanned — a file whose job is to record superseded claims would fire on stale names \
         that are its content.\n\
         Limits: (1) a text scan, not a Rust parser — an occurrence inside a comment or string \
         counts as present; (2) matching is by name, never by resolved path, so a symbol renamed \
         to a name that exists elsewhere in src/ passes (the tests/orphan_modules.rs limit); \
         (3) presence is all this proves — a sentence can name a live symbol and still be \
         flatly wrong; (4) a line whose backticks do not pair is skipped rather than mis-paired.\n\
         See tests/doc_symbols.rs.\n"
    );
    let _ = err.flush();
}

#[test]
fn every_backticked_symbol_in_claude_md_still_exists_in_src() {
    let root = repo_root();
    let md = read(&root.join("CLAUDE.md"));
    let src_files = collect_src_files(&root.join("src"), &root);
    let mentions = rust_symbol_mentions(&md);

    write_limits(
        unique_mentions(&mentions).len(),
        mentions.len(),
        src_files.len(),
        REGISTERED_DOC_SYMBOLS.len(),
    );

    let violations = classify(&mentions, &src_files, REGISTERED_DOC_SYMBOLS);
    if !violations.is_empty() {
        let report = violations
            .iter()
            .map(|v| format!("  - {}", v.message()))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "doc-symbol freshness gate failed ({} violation(s)):\n{report}\n\n\
             This gate lives in tests/doc_symbols.rs. Its scope is CLAUDE.md against src/ only — \
             ARCHITECTURE.md is deliberately excluded, because recording superseded claims is \
             its job.",
            violations.len()
        );
    }
}

#[test]
fn the_scan_covers_nested_src_modules() {
    // Guard against a walker that only reads the top level: `src/format/` is a
    // real subdirectory, and a symbol defined there must not read as stale.
    let root = repo_root();
    let files = collect_src_files(&root.join("src"), &root);
    assert!(!files.is_empty(), "the src/ walk found no *.rs files");
    assert!(
        files.iter().any(|(p, _)| p == "src/format/mod.rs"),
        "the walk must descend into subdirectories: {files:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mentions(pairs: &[(&str, usize)]) -> Vec<Mention> {
        pairs
            .iter()
            .map(|(s, l)| Mention {
                symbol: (*s).to_string(),
                line: *l,
            })
            .collect()
    }

    fn files(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(p, t)| ((*p).to_string(), (*t).to_string()))
            .collect()
    }

    #[test]
    fn extraction_finds_both_shapes_and_reports_the_line() {
        let md = "prose `load_project_context` here\n\
                  and `BUILTIN_TOOL_NAMES` there\n\
                  nothing on this line\n";
        assert_eq!(
            rust_symbol_mentions(md),
            mentions(&[("load_project_context", 1), ("BUILTIN_TOOL_NAMES", 2)])
        );
    }

    /// Near-miss guards for the matcher: the tokens that surround real symbols
    /// in CLAUDE.md and must NOT become candidates — paths, flags, versions,
    /// shell fragments, short tokens, and everything below the length floor.
    #[test]
    fn extraction_ignores_tokens_that_are_not_rust_symbol_shaped() {
        let scoped = format!("`{}`", "src/cli.rs");
        let md = format!(
            "`--lite` `bash -n` `0.16.5` `#918` {scoped} `ok` `Ok` `A` `MCP_PREFLIGHT_ATTEMPTS`\n"
        );
        assert_eq!(
            rust_symbol_mentions(&md),
            mentions(&[("MCP_PREFLIGHT_ATTEMPTS", 1)])
        );
    }

    /// A line whose backticks do not pair — the four code-fence lines today —
    /// is skipped rather than mis-paired. Pairing a stray tick would shift
    /// every following pair on that line and invent tokens.
    #[test]
    fn an_odd_backtick_line_is_skipped_rather_than_mis_paired() {
        assert_eq!(rust_symbol_mentions("```bash"), vec![]);
        assert_eq!(rust_symbol_mentions("  ```rust"), vec![]);
        // The near-miss: a well-formed line beside it is still read.
        assert_eq!(
            rust_symbol_mentions("```bash\n a `load_project_context` b\n```"),
            mentions(&[("load_project_context", 2)])
        );
    }

    /// Word-boundary matching, in both directions, including the qualified and
    /// call forms CLAUDE.md actually writes.
    #[test]
    fn identifier_matching_respects_word_boundaries() {
        assert!(identifier_occurs(
            "pub(crate) fn redact_secrets(s: &str)",
            "redact_secrets"
        ));
        assert!(identifier_occurs(
            "crate::safety::redact_secrets(s)",
            "redact_secrets"
        ));
        assert!(identifier_occurs(
            "self.redact_secrets(&x)",
            "redact_secrets"
        ));
        // Substrings are not occurrences.
        assert!(!identifier_occurs("redact_secrets_v2(s)", "redact_secrets"));
        assert!(!identifier_occurs("my_redact_secrets(s)", "redact_secrets"));
        assert!(!identifier_occurs("REDACT_SECRETS", "redact_secrets_caps"));
        assert!(!identifier_occurs("", "redact_secrets"));
        // A non-ASCII neighbour is a boundary, not an identifier byte.
        assert!(identifier_occurs("✓redact_secrets", "redact_secrets"));
    }

    /// Branch 1: the defect this gate exists for. Both remedies must be on the
    /// message verbatim, and the pasteable register line most of all.
    #[test]
    fn an_unregistered_absent_symbol_is_fatal() {
        let v = classify(
            &mentions(&[("gasp_reachable", 42), ("gasp_reachable", 90)]),
            &files(&[("src/a.rs", "fn other() {}")]),
            &[],
        );
        assert_eq!(
            v,
            vec![DocSymbolViolation::Unregistered {
                symbol: "gasp_reachable".to_string(),
                line: 42,
                mention_count: 2,
            }]
        );
        let msg = v[0].message();
        assert!(
            msg.contains("CLAUDE.md:42") && msg.contains("`gasp_reachable`"),
            "{msg}"
        );
        assert!(
            msg.contains(
                "(\"gasp_reachable\", \"<category>\", \"<why this name cannot live in src/>\")"
            ),
            "the register line must be pasteable: {msg}"
        );
        assert!(msg.contains("It forbids an unnamed one"), "{msg}");
    }

    /// The near-miss guard that matters: a live symbol with no register entry
    /// is exactly the passing case, and it must stay silent.
    #[test]
    fn a_present_symbol_passes() {
        let v = classify(
            &mentions(&[("load_project_context", 1)]),
            &files(&[("src/cli.rs", "fn load_project_context() {}")]),
            &[],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn a_registered_absent_symbol_passes() {
        let v = classify(
            &mentions(&[("deeper_question", 75)]),
            &files(&[("src/a.rs", "fn other() {}")]),
            &[(
                "deeper_question",
                CAT_WIRE,
                "a JSON field name in the wire contract",
            )],
        );
        assert!(v.is_empty(), "{v:?}");
    }

    /// Ratchet, direction 1: the registered name is live again.
    #[test]
    fn a_registered_symbol_that_is_now_live_is_fatal() {
        let v = classify(
            &mentions(&[("deeper_question", 75)]),
            &files(&[("src/a.rs", "let deeper_question = 1;")]),
            &[(
                "deeper_question",
                CAT_WIRE,
                "a JSON field name in the wire contract",
            )],
        );
        assert_eq!(
            v,
            vec![DocSymbolViolation::RegisteredButLive {
                symbol: "deeper_question".to_string(),
                category: CAT_WIRE.to_string(),
            }]
        );
        assert!(
            v[0].message()
                .contains("the exclusion is no longer paid for"),
            "{}",
            v[0].message()
        );
    }

    /// Ratchet, direction 2: the prose stopped naming it.
    #[test]
    fn a_registered_symbol_no_longer_mentioned_is_fatal() {
        let v = classify(
            &mentions(&[("something_else", 1)]),
            &files(&[("src/a.rs", "fn something_else() {}")]),
            &[(
                "deeper_question",
                CAT_WIRE,
                "a JSON field name in the wire contract",
            )],
        );
        assert_eq!(
            v,
            vec![DocSymbolViolation::RegisteredSymbolVanished {
                symbol: "deeper_question".to_string(),
                reason: "a JSON field name in the wire contract".to_string(),
            }]
        );
    }

    /// An unnamed debt wearing a name is not a name.
    #[test]
    fn a_blank_reason_is_fatal() {
        let v = classify(
            &mentions(&[("deeper_question", 1)]),
            &files(&[("src/a.rs", "fn other() {}")]),
            &[("deeper_question", CAT_WIRE, "   ")],
        );
        assert_eq!(
            v,
            vec![DocSymbolViolation::EmptyReason {
                symbol: "deeper_question".to_string(),
            }]
        );
    }

    /// The category is what makes a reason a rule about a class of token; a
    /// typo'd category is fatal so it cannot silently fragment the register.
    #[test]
    fn an_unknown_category_is_fatal() {
        let v = classify(
            &mentions(&[("deeper_question", 1)]),
            &files(&[("src/a.rs", "fn other() {}")]),
            &[(
                "deeper_question",
                "wire key",
                "a JSON field name in the wire contract",
            )],
        );
        assert_eq!(
            v,
            vec![DocSymbolViolation::UnknownCategory {
                symbol: "deeper_question".to_string(),
                category: "wire key".to_string(),
            }]
        );
        let msg = v[0].message();
        assert!(msg.contains("KNOWN_CATEGORIES"), "{msg}");
        assert!(msg.contains(CAT_WIRE), "{msg}");
    }

    /// Anti-vacuous, both halves: a scan that finds nothing in either place
    /// fails loudly rather than reporting a clean tree.
    #[test]
    fn an_empty_scan_is_fatal_in_both_directions() {
        let v = classify(&mentions(&[("load_project_context", 1)]), &[], &[]);
        assert_eq!(v, vec![DocSymbolViolation::EmptySourceScan]);
        assert!(
            v[0].message().contains("vacuous green"),
            "{}",
            v[0].message()
        );

        let v = classify(&[], &files(&[("src/a.rs", "fn a() {}")]), &[]);
        assert_eq!(v, vec![DocSymbolViolation::EmptyCandidateScan]);
        assert!(
            v[0].message().contains("nothing to scan"),
            "{}",
            v[0].message()
        );
    }

    /// The live register is non-vacuous too: every entry carries a real reason
    /// and a category from the closed set, and the categories are not a
    /// vestigial field — a register with one category for everything would
    /// satisfy the ratchet while saying nothing.
    #[test]
    fn the_live_register_is_categorised_and_every_reason_is_written() {
        assert!(
            !REGISTERED_DOC_SYMBOLS.is_empty(),
            "the register ships with the 2026-09-18 census's ten entries"
        );
        for (symbol, category, reason) in REGISTERED_DOC_SYMBOLS {
            assert!(!symbol.is_empty(), "empty symbol in the register");
            assert!(
                KNOWN_CATEGORIES.contains(category),
                "`{symbol}` carries the unknown category \"{category}\""
            );
            assert!(
                reason.trim().len() > 40,
                "`{symbol}` needs a reason stating why the name cannot live in src/"
            );
        }
        let distinct: std::collections::BTreeSet<&str> =
            REGISTERED_DOC_SYMBOLS.iter().map(|(_, c, _)| *c).collect();
        assert!(
            distinct.len() >= 3,
            "the register must be organised by category, not collapsed into one bucket: {distinct:?}"
        );
    }
}
