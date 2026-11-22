//! #1102: a heredoc body fed to `cat`/`tee` that writes a file is data, not
//! commands. bash writes that body to the file and runs none of it, so a
//! mention of a pattern there (a plan, a note, a commit message drafted to a
//! file) must not be refused. Every other heredoc body is still read as
//! commands, as it was before.
//!
//! The pattern text is built from pieces so this file stays readable and no
//! line of it is itself the pattern.

use super::*;

/// The root-delete pattern, assembled.
fn root() -> String {
    ["rm", "-rf", "/"].join(" ")
}

/// A home-directory delete, the shape of the Day-267 planning refusal.
fn home() -> String {
    ["rm", "-rf", "~/"].join(" ")
}

/// Rows whose heredoc body is data: they must now pass.
fn must_pass() -> Vec<(&'static str, String)> {
    let (r, h) = (root(), home());
    vec![
        (
            "cat > f <<'EOF' (plan text, apostrophes, bare pattern line)",
            format!(
                "cat > notes.md <<'EOF'\n# Plan\nNever run `{r}` here, don't.\n{r}\nit's fine\nEOF"
            ),
        ),
        ("cat >> f <<EOF", format!("cat >> f <<EOF\n{r}\nEOF")),
        ("tee f <<'EOF'", format!("tee out.md <<'EOF'\n{r}\nEOF")),
        ("tee -a f <<EOF", format!("tee -a out.md <<EOF\n{r}\nEOF")),
        (
            "/bin/cat > f <<'EOF'",
            format!("/bin/cat > f <<'EOF'\n{r}\nEOF"),
        ),
        (
            "cat <<'EOF' > f (redirect after the heredoc)",
            format!("cat <<'EOF' > f\n{r}\nEOF"),
        ),
        (
            "cat > plan <<'EOF' (home-delete mention, Day 267)",
            format!("cat > session_plan/task_01.md <<'EOF'\nTitle: x\n{h}\nEOF"),
        ),
        (
            "cat <<-EOF (tab-stripped terminator)",
            format!("cat > f <<-EOF\n\t{r}\n\tEOF"),
        ),
        (
            "cat > f <<\"EOF\" then a harmless command",
            format!("cat > f <<\"EOF\"\n{r}\nEOF\necho done"),
        ),
    ]
}

/// Rows that must still refuse, each with the reason it is not data.
fn must_refuse() -> Vec<(&'static str, String)> {
    let r = root();
    vec![
        (
            "cat <<'EOF' | bash",
            format!("cat <<'EOF' | bash\n{r}\nEOF"),
        ),
        ("cat <<EOF | sh", format!("cat <<EOF | sh\n{r}\nEOF")),
        ("tee f <<EOF | sh", format!("tee f <<EOF | sh\n{r}\nEOF")),
        ("bash <<'EOF'", format!("bash <<'EOF'\n{r}\nEOF")),
        ("sh -s <<EOF", format!("sh -s <<EOF\n{r}\nEOF")),
        (
            "python3 - <<'EOF' (interpreter, #1222)",
            format!("python3 - <<'EOF'\n{r}\nEOF"),
        ),
        (
            "cat > f <<'EOF' && <pattern> on the command line",
            format!("cat > f <<'EOF' && {r}\nbody\nEOF"),
        ),
        (
            "cat to stdout only (no file)",
            format!("cat <<'EOF'\n{r}\nEOF"),
        ),
        (
            "unquoted body runs $(…)",
            format!("cat > f <<EOF\nnow: $({r})\nEOF"),
        ),
        (
            "unquoted body runs backticks",
            format!("cat > f <<EOF\nnow: `{r}`\nEOF"),
        ),
        (
            "pattern after the terminator",
            format!("cat > f <<'EOF'\nok\nEOF\n{r}"),
        ),
        ("unterminated heredoc", format!("cat > f <<'EOF'\nok\n{r}")),
        (
            "backslash-newline hides the quoted terminator",
            format!("cat > f <<'EOF'\nx \\\nEOF\n{r}\nEOF"),
        ),
        (
            "an apostrophe in the body must not hide a later $(…)",
            format!("cat > f <<'EOF'\nit's\nEOF\necho \"$({r})\""),
        ),
        (
            "process substitution runs the output",
            format!("cat <<'EOF' > >(bash)\n{r}\nEOF"),
        ),
        (
            "a second heredoc on the line feeds a shell",
            format!("cat > a <<'A' && bash <<'B'\nok\nA\n{r}\nB"),
        ),
        (
            "a delimiter with a space is not read as written",
            format!("cat > f <<'E OF'\nE OF\n{r}\nE"),
        ),
        (
            "an escaped space: bash ends at `E F`, a naive read at `E`",
            format!("cat > f <<E\\ F\nE F\n{r}\nE"),
        ),
    ]
}

fn verdict(cmd: &str) -> &'static str {
    match hard_deny_match(cmd) {
        Some(hit) => hit.label,
        None => "pass",
    }
}

#[test]
fn heredoc_verdict_table() {
    let mut wrong = Vec::new();
    println!("| row | want | got |");
    for (name, cmd) in must_pass() {
        let got = verdict(&cmd);
        println!("| {name} | pass | {got} |");
        if got != "pass" {
            wrong.push(format!("must pass: {name}: refused as {got}"));
        }
    }
    for (name, cmd) in must_refuse() {
        let got = verdict(&cmd);
        println!("| {name} | refuse | {got} |");
        if got == "pass" {
            wrong.push(format!("must refuse: {name}: passed"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn cat_and_tee_to_a_file_bodies_are_data() {
    for (name, cmd) in must_pass() {
        assert_eq!(verdict(&cmd), "pass", "{name}: {cmd:?}");
    }
}

#[test]
fn every_other_heredoc_body_is_still_read_as_commands() {
    for (name, cmd) in must_refuse() {
        assert_ne!(verdict(&cmd), "pass", "{name}: {cmd:?}");
    }
}
