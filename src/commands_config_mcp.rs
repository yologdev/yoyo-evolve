//! `/mcp list`'s failure disclosure — the user's half of the "third door".
//!
//! Split out of `commands_config.rs` rather than appended to it (Day 202): that
//! file was 29 lines past `MAX_MODULE_LINES` and this work is ~300 lines of pure
//! rendering plus its tests, which would have blown the register-drift band and
//! made the gate fatal. The split is the gate's own stated remedy, and it is a
//! clean seam — these functions read nothing but their arguments, so no private
//! `commands_config` helper had to be widened to move them.
//!
//! The fact itself is recorded elsewhere. `agent_builder::record_failed_server`
//! is the single write site and always has been; on Day 181 the MODEL started
//! reading it (`external_tool_failure_note`) while the USER kept only a dim
//! startup line they had already scrolled past, so `2 of 3 failed` rendered
//! identically to `3 of 3 failed`. This module adds the **second audience** for
//! a fact already recorded — it moves neither store and reconnects nothing.

use crate::commands_config::mcp_not_connected_message;

/// Marker appended to **one** `/mcp list` row whose server failed to connect.
///
/// This is the *user's* half of the third door: the model learned about failed
/// connects on Day 181 (`external_tool_failure_note`), the user was left with a
/// dim startup line they had already scrolled past, so `2 of 3 failed` rendered
/// exactly like `3 of 3 failed`. The fact was already recorded — this only adds
/// the second audience for it, and moves neither store.
///
/// Pure, so the table test pins the rule without touching the global: it takes
/// the never-drained `mcp_failed` snapshot and the identity string of one row.
///
/// **`None` for an empty `mcp_failed`** is the whole regression surface: every
/// user whose servers all connect, and every user who configured none, renders
/// byte-identically to before.
///
/// **It marks only what it can prove.** `mcp_connected` is a *count*, not a set,
/// so nothing says which of the remaining rows connected; a per-row "connected"
/// claim would be the confident-wrong-diagnosis defect this surface exists to
/// avoid. The aggregate tail keeps carrying the count, unchanged.
///
/// **It names the server, never a tool.** A failed connect means yoyo never
/// learned which tools that server exposes, so no tool name is invented here.
///
/// **MCP only.** `openapi_failed` is a different surface with its own rows and
/// deliberately does not appear here — the same rule `connections_lost_note`
/// follows when it carries `kind` rather than folding two kinds together.
///
/// Glyph-free when `plain`, matching `project_mcp_refusal_message`,
/// `collision_guard_skipped_message` and `connections_lost_note`.
pub(crate) fn mcp_failure_marker(
    mcp_failed: &[String],
    row_id: &str,
    plain: bool,
) -> Option<String> {
    // Exact match against the string the single write site records: nothing
    // looser (a name prefix, a label) can distinguish two `npx`-based servers,
    // and marking the wrong row is worse than marking none.
    if !mcp_failed.iter().any(|id| id == row_id) {
        return None;
    }
    let shown = crate::agent_builder::capped_failure_id(row_id);
    Some(if plain {
        format!("  failed to connect: {shown}")
    } else {
        format!("  ✗ failed to connect: {shown}")
    })
}

/// Render the whole `/mcp list` body (no color codes — the caller wraps it in
/// the dim block, so this stays pure and the byte assertions below are stable).
///
/// **The one honest difference from the previous inline printing, stated rather
/// than glossed:** the old code emitted `{RESET}` *before* its trailing
/// newlines, so this returns that newline inside the text and the caller's
/// `{RESET}` now lands after it. Visible characters and the colour state at the
/// end of the call are identical; the newline is merely inside the dim span
/// instead of outside it. That is the same shape `/config show` already uses
/// (`print!("{DIM}{output}{RESET}")`), and it is why the tests here pin the
/// colour-code-free **block** and say so, rather than claiming byte-identity of
/// the escape-positioned stream.
///
/// Extracted so a test can pin the **full block** rather than one row: the
/// no-failure case must be byte-identical to before, because that is every user
/// whose servers all connect. The only per-row difference a failure makes is the
/// marker from [`mcp_failure_marker`]; header, columns and the aggregate tail are
/// untouched, and `mcp_count == 0` still falls back to
/// [`mcp_not_connected_message`] exactly as it did.
pub(crate) fn mcp_list_text(
    cli_servers: &[String],
    server_configs: &[crate::cli::McpServerConfig],
    mcp_count: u32,
    mcp_failed: &[String],
    plain: bool,
) -> String {
    if cli_servers.is_empty() && server_configs.is_empty() {
        return concat!(
            "  No MCP servers configured.\n",
            "\n",
            "  Add servers to .yoyo.toml:\n",
            "    [mcp_servers.myserver]\n",
            "    command = \"npx\"\n",
            "    args = [\"-y\", \"@modelcontextprotocol/server-fetch\"]\n",
            "\n",
            "  See /mcp help for more details.\n",
        )
        .to_string();
    }

    let mut out = String::from("  MCP Servers:\n");

    // Structured configs first
    for cfg in server_configs {
        let full_cmd = if cfg.args.is_empty() {
            cfg.command.clone()
        } else {
            format!("{} {}", cfg.command, cfg.args.join(" "))
        };
        // The row's identity is the string `record_failed_server` wrote for this
        // loop — built by the same helper the writer uses, never a second copy of
        // the format.
        let row_id = crate::agent_builder::mcp_failed_server_id(&cfg.name, &cfg.command);
        let marker = mcp_failure_marker(mcp_failed, &row_id, plain).unwrap_or_default();
        out.push_str(&format!("    {:<14}{}{marker}\n", cfg.name, full_cmd));
    }

    // CLI --mcp servers: the recorded id is the raw command string itself.
    for cmd in cli_servers {
        let label = cmd.split_whitespace().next().unwrap_or("unknown");
        let marker = mcp_failure_marker(mcp_failed, cmd, plain).unwrap_or_default();
        out.push_str(&format!("    {:<14}{cmd}{marker}\n", label));
    }

    let total = cli_servers.len() + server_configs.len();
    out.push('\n');
    if mcp_count > 0 {
        out.push_str(&format!(
            "  {total} server(s) configured, {mcp_count} connected\n\n"
        ));
    } else {
        out.push_str(&format!("{}\n", mcp_not_connected_message(total)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Day 202: `/mcp list` names which server failed to connect ------------
    // The user's half of the third door (the model's half is
    // `external_tool_failure_note`, Day 181). Asserted on the string a caller
    // receives, never on an eyeballed terminal.

    /// Two structured servers and one `--mcp` server, exercising both row
    /// columns and both recorded-id shapes in one fixture. Note the `--mcp` row
    /// is labelled by the command's **first word** (`npx`), not by a server name
    /// — `<name>` only exists for `[mcp_servers.*]` entries, which is why the
    /// recorded id for those carries the name and the command beside it.
    fn mcp_list_fixture() -> (Vec<String>, Vec<crate::cli::McpServerConfig>) {
        let configs = vec![
            crate::cli::McpServerConfig {
                name: "fetch".to_string(),
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-fetch".to_string(),
                ],
                env: vec![],
            },
            crate::cli::McpServerConfig {
                name: "postgres".to_string(),
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-postgres".to_string(),
                ],
                env: vec![],
            },
        ];
        let cli = vec!["npx -y @modelcontextprotocol/server-memory".to_string()];
        (cli, configs)
    }

    #[test]
    fn mcp_list_block_is_byte_identical_when_nothing_failed() {
        let (cli, configs) = mcp_list_fixture();
        let expected = concat!(
            "  MCP Servers:\n",
            "    fetch         npx -y @modelcontextprotocol/server-fetch\n",
            "    postgres      npx -y @modelcontextprotocol/server-postgres\n",
            "    npx           npx -y @modelcontextprotocol/server-memory\n",
            "\n",
            "  3 server(s) configured, 2 connected\n",
            "\n",
        );
        // The whole regression surface: no failures (and no servers at all) must
        // render exactly as before, pinned byte-for-byte rather than by name.
        assert_eq!(mcp_list_text(&cli, &configs, 2, &[], false), expected);
        assert_eq!(mcp_list_text(&cli, &configs, 2, &[], true), expected);

        // The `mcp_count == 0` tail still falls back to the not-connected
        // message, whose count line is the only difference from the block above.
        let none = mcp_list_text(&cli, &configs, 0, &[], false);
        assert_eq!(
            none,
            expected.replace(
                "  3 server(s) configured, 2 connected\n\n",
                &format!("{}\n", mcp_not_connected_message(3)),
            )
        );
        assert!(!none.contains("failed to connect"), "{none}");

        // No servers configured: the early-return block, unchanged.
        assert_eq!(
            mcp_list_text(&[], &[], 0, &[], false),
            concat!(
                "  No MCP servers configured.\n",
                "\n",
                "  Add servers to .yoyo.toml:\n",
                "    [mcp_servers.myserver]\n",
                "    command = \"npx\"\n",
                "    args = [\"-y\", \"@modelcontextprotocol/server-fetch\"]\n",
                "\n",
                "  See /mcp help for more details.\n",
            )
        );
    }

    #[test]
    fn mcp_list_marks_only_the_row_that_failed() {
        let (cli, configs) = mcp_list_fixture();
        // Built through the writer's own id builder: if the recorded string ever
        // changes shape, this test reddens instead of the row quietly going
        // unmarked.
        let failed = vec![crate::agent_builder::mcp_failed_server_id(
            "postgres", "npx",
        )];

        // Anti-vacuous companion: the fixture's id really is the one in the
        // slice, so a transcription slip cannot make the assertions below pass
        // by agreeing with themselves.
        assert!(
            failed.iter().any(|id| id == "postgres (npx)"),
            "fixture must carry the recorded id: {failed:?}"
        );

        let text = mcp_list_text(&cli, &configs, 2, &failed, false);
        assert!(
            text.contains(
                "    postgres      npx -y @modelcontextprotocol/server-postgres  \
                 ✗ failed to connect: postgres (npx)\n"
            ),
            "the failed row must carry the marker: {text}"
        );
        // The other rows are unmarked, and their lines end where they always did.
        assert!(
            text.contains("    fetch         npx -y @modelcontextprotocol/server-fetch\n"),
            "{text}"
        );
        assert!(
            text.contains("    npx           npx -y @modelcontextprotocol/server-memory\n"),
            "{text}"
        );
        assert_eq!(
            text.matches("failed to connect").count(),
            1,
            "exactly one row failed: {text}"
        );
        // The aggregate tail is untouched.
        assert!(
            text.contains("  3 server(s) configured, 2 connected\n"),
            "{text}"
        );
    }

    #[test]
    fn mcp_list_marks_every_failed_row_and_leaves_the_count_line_alone() {
        let (cli, configs) = mcp_list_fixture();
        // One structured (a different row from the single-failure test, so the
        // match is on identity and not on position) and the `--mcp` server.
        let failed = vec![
            crate::agent_builder::mcp_failed_server_id("fetch", "npx"),
            cli[0].clone(),
        ];
        let text = mcp_list_text(&cli, &configs, 1, &failed, false);
        assert_eq!(text.matches("✗ failed to connect").count(), 2, "{text}");
        assert!(
            text.contains("failed to connect: fetch (npx)\n"),
            "the structured row is marked: {text}"
        );
        assert!(
            text.contains("failed to connect: npx -y @modelcontextprotocol/server-memory\n"),
            "the --mcp row is marked by its raw command: {text}"
        );
        assert!(
            !text.contains("failed to connect: postgres (npx)"),
            "the row that connected is not marked: {text}"
        );
        assert!(
            text.contains("  3 server(s) configured, 1 connected\n"),
            "{text}"
        );
    }

    #[test]
    fn mcp_failure_marker_is_glyph_free_when_plain() {
        let (_, _) = mcp_list_fixture();
        let failed = vec!["postgres (npx)".to_string()];
        let plain = mcp_failure_marker(&failed, "postgres (npx)", true).expect("the row failed");
        let fancy = mcp_failure_marker(&failed, "postgres (npx)", false).expect("the row failed");
        // Anti-vacuous pair: the fancy marker really does carry a glyph, so
        // "plain is ASCII" is a statement about the flag and not about a string
        // that was ASCII either way.
        assert!(
            !fancy.is_ascii(),
            "the non-plain marker carries a glyph: {fancy}"
        );
        assert!(
            plain.is_ascii(),
            "plain output must not emit the ✗ glyph: {plain:?}"
        );
        assert_eq!(plain, "  failed to connect: postgres (npx)");
        // And it reaches the block the caller prints.
        let (cli, configs) = mcp_list_fixture();
        let block = mcp_list_text(&cli, &configs, 2, &failed, true);
        let marked: String = block
            .lines()
            .find(|l| l.contains("failed to connect"))
            .expect("a marked row")
            .chars()
            .filter(|c| !c.is_ascii())
            .collect();
        assert_eq!(marked, "", "no non-ASCII byte in the plain marker: {block}");
    }

    #[test]
    fn mcp_failure_marker_caps_an_over_long_id_on_a_char_boundary() {
        // Multi-byte characters only: a raw byte index would split one (or panic,
        // #250) and the in-band marker would then lie about what it dropped.
        let long = "✓".repeat(300);
        assert!(long.len() > crate::agent_builder::EXTERNAL_FAILURE_ID_MAX_BYTES);
        let failed = vec![long.clone()];
        let marker = mcp_failure_marker(&failed, &long, false).expect("the row failed");

        let after = marker
            .split("failed to connect: ")
            .nth(1)
            .expect("the id follows the reason");
        let kept = after
            .split("… [yoyo:")
            .next()
            .expect("the marker terminates the kept prefix");
        assert!(long.starts_with(kept), "kept text is a real prefix: {kept}");
        assert!(
            kept.chars().all(|c| c == '✓'),
            "no split character survived the cut"
        );
        assert!(
            kept.len() <= crate::agent_builder::EXTERNAL_FAILURE_ID_MAX_BYTES,
            "kept {} bytes exceeds the cap",
            kept.len()
        );
        // The marker's arithmetic must agree with what was actually dropped.
        let dropped = long.len() - kept.len();
        assert!(
            marker.contains(&format!("{dropped} bytes elided from this identifier")),
            "marker must report {dropped} dropped bytes: {marker}"
        );
    }

    #[test]
    fn mcp_failure_marker_marks_nothing_it_cannot_prove() {
        let failed = vec!["fetch (npx)".to_string()];
        assert_eq!(mcp_failure_marker(&[], "fetch (npx)", false), None);
        // A different server sharing a command word, a name that is a prefix,
        // and the OpenAPI shape are all left unmarked.
        assert_eq!(mcp_failure_marker(&failed, "postgres (npx)", false), None);
        assert_eq!(mcp_failure_marker(&failed, "fetch", false), None);
        assert_eq!(mcp_failure_marker(&failed, "npx -y server", false), None);
        assert_eq!(mcp_failure_marker(&failed, "spec.yaml", false), None);
    }
}
