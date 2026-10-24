//! A loud warning for an oversized project instruction file (Part of #1070).
//!
//! A 2,160,001-byte `CLAUDE.md` used to reach the system prompt of every
//! request (about 540K tokens) with an empty stderr: the only size warning,
//! `agent_builder::system_prompt_token_budget`, fires only when the whole
//! composed prompt is bigger than the model's entire context window.
//!
//! This is a warning, not a cap: the file still loads unchanged. Whether to
//! cap or truncate is undecided and stays on #1070.

/// Per-file threshold, in bytes. Chosen from the precedent table, not by feel:
/// Claude Code warns at 150.0k chars (it used to warn at 40.0k). 40k is ruled
/// out here because this repo's own CLAUDE.md is ~41.7 KB, so it would fire on
/// every evolve run and on any medium-sized product file (cry-wolf).
///
/// Bytes rather than chars keeps this simple, and it errs early, never late:
/// a UTF-8 string has at least as many bytes as chars, so this fires at or
/// before the point a 150k-char rule would.
pub(crate) const INSTRUCTION_FILE_WARN_BYTES: usize = 150_000;

/// The warning for an instruction file `name` whose loaded content is `bytes`
/// long, or `None` at or below [`INSTRUCTION_FILE_WARN_BYTES`].
///
/// The token figure is a bytes/4 estimate and says so. `name` comes from the
/// repository, so it is sanitized; `plain` output is glyph-free (no ⚠, no em
/// dash), matching `context::instruction_file_refused_warning`.
pub(crate) fn oversized_instruction_warning(
    name: &str,
    bytes: usize,
    plain: bool,
) -> Option<String> {
    if bytes <= INSTRUCTION_FILE_WARN_BYTES {
        return None;
    }
    let name = crate::cli::sanitize_for_display(name);
    let tokens = bytes / 4;
    let body = format!(
        "project instruction file {name} is {bytes} bytes (~{tokens} tokens, estimated), \
         over the {INSTRUCTION_FILE_WARN_BYTES}-byte warning size. The whole file is sent with \
         every request, so it costs money each turn and shrinks the room left for the \
         conversation. Trim it or split it into smaller files."
    );
    Some(if plain {
        format!("warning: {body}")
    } else {
        format!("⚠ {body}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_table() {
        // (bytes, fires)
        let rows = [
            (0, false),
            (41_713, false), // this repo's own CLAUDE.md on Day 238
            (149_999, false),
            (150_000, false),
            (150_001, true),
            (2_160_001, true),
        ];
        for (bytes, fires) in rows {
            for plain in [false, true] {
                let got = oversized_instruction_warning("CLAUDE.md", bytes, plain);
                assert_eq!(got.is_some(), fires, "bytes={bytes} plain={plain}: {got:?}");
                if let Some(msg) = got {
                    assert!(msg.contains("CLAUDE.md"), "{msg}");
                    assert!(msg.contains(&format!("{bytes} bytes")), "{msg}");
                    assert!(msg.contains(&format!("~{} tokens", bytes / 4)), "{msg}");
                }
            }
        }
    }

    #[test]
    fn full_string_both_modes() {
        let expected_body = "project instruction file CLAUDE.md is 2160001 bytes \
             (~540000 tokens, estimated), over the 150000-byte warning size. The whole file \
             is sent with every request, so it costs money each turn and shrinks the room \
             left for the conversation. Trim it or split it into smaller files.";
        assert_eq!(
            oversized_instruction_warning("CLAUDE.md", 2_160_001, true).unwrap(),
            format!("warning: {expected_body}")
        );
        assert_eq!(
            oversized_instruction_warning("CLAUDE.md", 2_160_001, false).unwrap(),
            format!("⚠ {expected_body}")
        );
    }

    #[test]
    fn plain_is_glyph_free() {
        let msg = oversized_instruction_warning("AGENTS.md", 200_000, true).unwrap();
        assert!(!msg.contains('⚠') && !msg.contains('—'), "{msg}");
    }

    #[test]
    fn name_is_sanitized() {
        let hostile = "CLAUDE\x1b[2J.md";
        assert!(hostile.as_bytes().contains(&0x1b), "fixture must carry ESC");
        let msg = oversized_instruction_warning(hostile, 200_000, false).unwrap();
        assert!(!msg.as_bytes().contains(&0x1b), "{msg:?}");
    }
}
