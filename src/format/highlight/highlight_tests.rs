//! Emission-point tests for the cross-line highlighter (#804).
//!
//! The state plumbing added for #804 (`HighlightState::block_comment_depth`,
//! `scan_block_comments`, `highlight_code_line_with`) is user-visible rendering
//! behaviour: a `/* … */` that spans lines mis-colours every line after it if the
//! plumbing regresses. The test module next door renders **one line** at a time, so
//! none of it can notice a line-2 problem — that is the gap this file closes.
//!
//! Two rules for every test here, both taken from the task's own wording:
//!
//! 1. **Assert at the emission point** — the string a caller receives from
//!    `highlight_code_line_with` / `MarkdownRenderer`, never a transient internal.
//!    The carried state is asserted too (so the class of a carried delimiter is
//!    pinned), but never as the whole test.
//! 2. **Every sequence test carries an anti-vacuous companion** — an assertion that
//!    the renderer really does colour the lines it is asked about, so a highlighter
//!    that emitted plain text could not pass by agreeing with itself.
//!
//! `color_enabled()` is pinned to `true` under `cfg(test)`, so the `{DIM}` /
//! `{GREEN}` / `{BOLD_CYAN}` formatting below is literal throughout.

use super::*;

/// Render a whole sequence through the shared state and join it, which is the shape a
/// code block is actually rendered in. Returns the joined output plus the final state
/// so a caller can assert nothing was left behind.
fn render_sequence(lang: &str, lines: &[&str]) -> (String, HighlightState) {
    let mut st = HighlightState::default();
    let rendered: Vec<String> = lines
        .iter()
        .map(|line| highlight_code_line_with(lang, line, &mut st))
        .collect();
    (rendered.join("\n"), st)
}

/// Render a whole markdown document through the real streaming entry point.
fn render_markdown(input: &str) -> String {
    let mut r = MarkdownRenderer::new();
    let mut out = r.render_delta(input);
    out.push_str(&r.flush());
    out
}

// --- 1. A block comment that spans lines -------------------------------------------

/// A `/*` opened on line 1 and closed mid-line-3: line 2 is comment-coloured in full,
/// and line 3's segment *before* the closer is comment-coloured while the segment
/// after it is not.
#[test]
fn block_comment_spanning_lines_colours_line_two_and_ends_mid_line_three() {
    let source = ["let x = 1; /* open", "still inside", "*/ let y = 2;"];
    let (out, st) = render_sequence("rust", &source);
    let lines: Vec<&str> = out.split('\n').collect();

    // Line 1 opens as code and turns into comment at the `/*`.
    assert!(
        lines[0].contains(&format!("{BOLD_CYAN}let{RESET}")),
        "line 1 starts as code: {:?}",
        lines[0]
    );
    assert!(
        lines[0].ends_with(&format!("{DIM}/* open{RESET}")),
        "line 1 ends inside the comment: {:?}",
        lines[0]
    );

    // Line 2 is comment-coloured in full — this is the line a per-line test cannot see.
    assert_eq!(
        lines[1],
        format!("{DIM}still inside{RESET}"),
        "line 2 is entirely comment, got {out:?}"
    );

    // Line 3: comment up to and including the closer, code after it.
    assert!(
        lines[2].starts_with(&format!("{DIM}*/{RESET}")),
        "line 3 opens comment-coloured: {:?}",
        lines[2]
    );
    assert!(
        lines[2].contains(&format!("{BOLD_CYAN}let{RESET}")),
        "code resumes just past the closer: {:?}",
        lines[2]
    );
    assert_eq!(st.block_comment_depth, 0, "the closer drains the depth");

    // Anti-vacuous: a highlighter that emitted the source unchanged cannot pass.
    assert_ne!(out, source.join("\n"));
}

// --- 2. Near-miss: the one-line `/* … */` ------------------------------------------

/// A block comment opened *and* closed on one line must not leak: line 2, plain code,
/// is not comment-coloured. This is the regression a `block_comment_depth` that never
/// decrements would cause, and it is the one every sequence test above it would still
/// pass.
#[test]
fn one_line_block_comment_does_not_leak_onto_the_next_line() {
    let mut st = HighlightState::default();

    let l1 = highlight_code_line_with("rust", "let a = 1; /* done */ ", &mut st);
    assert_eq!(
        st.block_comment_depth, 0,
        "a closed comment leaves no depth"
    );
    assert!(
        l1.contains(&format!("{DIM}/* done */{RESET}")),
        "the one-line comment is still coloured: {l1:?}"
    );

    let l2 = highlight_code_line_with("rust", "let b = 2;", &mut st);
    assert_eq!(
        l2,
        highlight_code_line("rust", "let b = 2;"),
        "line 2 must be byte-identical to the stateless render, got {l2:?}"
    );
    assert!(
        !l2.contains(DIM.0),
        "line 2 is not comment-coloured: {l2:?}"
    );
    assert!(
        l2.contains(&format!("{BOLD_CYAN}let{RESET}")),
        "line 2 is highlighted as code: {l2:?}"
    );

    // Anti-vacuous: the dim rendering really is a different string, so the negative
    // assertion above is not passing on a renderer that colours nothing.
    assert_ne!(l2, format!("{DIM}let b = 2;{RESET}"));
}

// --- 3. An unterminated string literal spanning lines -------------------------------

/// A `"` opened on line 1 and closed on line 3: line 2 is string content, and the
/// delimiter the state carries is the **plain-quote class** — not merely "something
/// other than code". Pinning the class is what keeps a raw string, a template literal
/// or a triple-quote from drifting into the slot that Rust's `"…"` owns.
#[test]
fn unterminated_string_carries_the_plain_quote_delimiter_class_across_lines() {
    let mut st = HighlightState::default();

    let l1 = highlight_code_line_with("rust", "let s = \"start {", &mut st);
    assert_eq!(
        st.open_string,
        Some(StringDelim::Normal),
        "the carried class is the plain-quote one, got {:?}",
        st.open_string
    );
    assert!(
        l1.contains(&format!("{BOLD_CYAN}let{RESET}")),
        "line 1 is still code before the quote: {l1:?}"
    );

    let line2 = "fn not_really_code() {";
    let l2 = highlight_code_line_with("rust", line2, &mut st);
    assert_eq!(
        l2,
        format!("{GREEN}{line2}{RESET}"),
        "line 2 is string content in full, got {l2:?}"
    );
    assert_eq!(
        st.open_string,
        Some(StringDelim::Normal),
        "line 2 does not close the literal"
    );

    let l3 = highlight_code_line_with("rust", "end\"; let y = 2;", &mut st);
    assert!(
        l3.starts_with(&format!("{GREEN}end\"{RESET}")),
        "the closer ends the string segment: {l3:?}"
    );
    assert!(
        l3.contains(&format!("{BOLD_CYAN}let{RESET}")),
        "code resumes just past the closer: {l3:?}"
    );
    assert_eq!(st.open_string, None, "line 3 closes the literal");

    // Anti-vacuous: rendered without the carried state, line 2 is a normal code line,
    // so the string colour on it is the carried state's doing and not the renderer's
    // default for that text.
    assert_ne!(l2, line2, "the continuation line really is coloured");
    let stateless = [
        highlight_code_line("rust", "let s = \"start {"),
        highlight_code_line("rust", line2),
        highlight_code_line("rust", "end\"; let y = 2;"),
    ]
    .join("\n");
    assert_ne!(
        [l1, l2, l3].join("\n"),
        stateless,
        "carrying the open string must change the sequence"
    );
}

// --- 4. The markdown fence reset ----------------------------------------------------

/// The one piece of the fix that lives outside `highlight_code_line_with`: a code block
/// that leaves `/*` open must not colour the next fence's block. Rendered through the
/// real `MarkdownRenderer`, so the field and both reset sites are exercised.
#[test]
fn markdown_fence_resets_carried_comment_state_between_blocks() {
    let input = "```rust\nlet x = 1; /* open\nstill inside\n```\n```rust\nlet y = 2;\n```\n";
    let out = render_markdown(input);

    // Companion: within the first block the state IS carried, so the reset assertion
    // below cannot pass on a renderer that carries nothing at all.
    assert!(
        out.contains(&format!("{DIM}/* open{RESET}")),
        "block 1 opens a comment: {out:?}"
    );
    assert!(
        out.contains(&format!("{DIM}still inside{RESET}")),
        "block 1 carries it to its second line: {out:?}"
    );

    // The second block's first line starts clean — code colour, not comment colour.
    assert_eq!(
        out.matches(&format!("{BOLD_CYAN}let{RESET}")).count(),
        2,
        "both blocks' `let` lines are code, got {out:?}"
    );
    assert!(
        !out.contains(&format!("{DIM}let y = 2;{RESET}")),
        "block 2 must not inherit block 1's open comment: {out:?}"
    );
}

// --- 5. Near-miss: the stateless entry point is byte-identical ----------------------

/// The load-bearing promise of the whole state change: a sequence of lines with no
/// cross-line construct renders exactly as it did before the state existed. Whole-string
/// equality against the stateless wrapper, not a `contains`.
#[test]
fn stateless_entry_point_is_byte_identical_for_a_sequence_with_no_cross_line_construct() {
    let lines = [
        "fn main() {",
        "    let s = \"hi\"; // note",
        "    let a = 1; /* one line */",
        "}",
    ];
    let mut st = HighlightState::default();
    let mut stateful: Vec<String> = Vec::new();
    for line in lines {
        let with_state = highlight_code_line_with("rust", line, &mut st);
        assert_eq!(
            with_state,
            highlight_code_line("rust", line),
            "line {line:?} changed for single-line callers, got {with_state:?}"
        );
        stateful.push(with_state);
    }

    assert_eq!(st.block_comment_depth, 0, "nothing was carried");
    assert_eq!(st.open_string, None, "nothing was carried");

    // Anti-vacuous: the sequence really is coloured, so "unchanged" is not "unrendered".
    let joined = stateful.join("\n");
    assert_ne!(joined, lines.join("\n"));
    assert!(
        joined.contains(&format!("{BOLD_CYAN}fn{RESET}")),
        "got {joined:?}"
    );
    assert!(
        joined.contains(&format!("{GREEN}\"hi\"{RESET}")),
        "got {joined:?}"
    );
    assert!(
        joined.contains(&format!("{DIM}/* one line */{RESET}")),
        "got {joined:?}"
    );
}
