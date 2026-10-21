//! Day 225: command words the analyser cannot read.
//!
//! The inverse probe of every name-keyed check in `safety.rs`: each one
//! recognises a dangerous program by its SPELLING, so a program chosen by an
//! expansion (`X=rm; $X …`, `BASH_ARGV0=rm; $BASH_ARGV0 …`, `$(echo rm) …`)
//! slipped past all of them. Split out of `safety.rs` for the module-size gate.

use super::RM_COMMAND_WRAPPERS;

/// Shells whose `-c <string>` argument is itself a command to analyse.
const INLINE_SHELLS: &[&str] = &["bash", "sh", "zsh", "dash", "ksh"];

/// Words that keep the next word in command position (`if $X; then $Y; fi`).
const COMMAND_POSITION_KEYWORDS: &[&str] = &[
    "!", "{", "if", "then", "else", "elif", "do", "while", "until",
];

/// One lexed shell token: a word, or a command boundary.
enum ShellToken {
    Word(String),
    Boundary,
}

/// Split a command into shell words and command boundaries, honouring quotes,
/// backticks, `$(...)` and `${...}` well enough to keep `"$CARGO"`,
/// `$(echo rm)` and `${X:-a b}` each as ONE word. Boundaries are `;`, `&`,
/// `|`, newline and a bare `(`/`)` (subshell); `&` glued to a redirection
/// (`2>&1`, `&>f`) stays inside its word. This is not a shell parser — it
/// only has to find command words for [`check_indirect_command_word`].
fn lex_shell_words(cmd: &str) -> Vec<ShellToken> {
    let chars: Vec<char> = cmd.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut sq, mut dq, mut bt) = (false, false, false);
    let (mut paren, mut brace) = (0usize, 0usize);
    let flush = |cur: &mut String, out: &mut Vec<ShellToken>| {
        if !cur.is_empty() {
            out.push(ShellToken::Word(std::mem::take(cur)));
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if sq {
            cur.push(c);
            sq = c != '\'';
        } else if dq {
            cur.push(c);
            if c == '\\' {
                if let Some(n) = next {
                    cur.push(n);
                    i += 1;
                }
            } else if c == '"' {
                dq = false;
            }
        } else if bt {
            cur.push(c);
            bt = c != '`';
        } else {
            match c {
                '\\' => {
                    cur.push(c);
                    if let Some(n) = next {
                        cur.push(n);
                        i += 1;
                    }
                }
                '\'' => {
                    cur.push(c);
                    sq = true;
                }
                '"' => {
                    cur.push(c);
                    dq = true;
                }
                '`' => {
                    cur.push(c);
                    bt = true;
                }
                '$' if matches!(next, Some('(' | '{')) => {
                    cur.push(c);
                    if next == Some('(') {
                        cur.push('(');
                        paren += 1;
                    } else {
                        cur.push('{');
                        brace += 1;
                    }
                    i += 1;
                }
                '(' if paren > 0 => {
                    paren += 1;
                    cur.push(c);
                }
                ')' if paren > 0 => {
                    paren -= 1;
                    cur.push(c);
                }
                '}' if brace > 0 => {
                    brace -= 1;
                    cur.push(c);
                }
                _ if paren > 0 || brace > 0 => cur.push(c),
                ' ' | '\t' => flush(&mut cur, &mut out),
                '&' if cur.ends_with(['>', '<']) || next == Some('>') => cur.push(c),
                '\n' | ';' | '&' | '|' | '(' | ')' => {
                    flush(&mut cur, &mut out);
                    out.push(ShellToken::Boundary);
                }
                _ => cur.push(c),
            }
        }
        i += 1;
    }
    flush(&mut cur, &mut out);
    out
}

/// Strip one layer of matching outer quotes (`"…"` or `'…'`).
fn strip_outer_quotes(w: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = w.strip_prefix(q).and_then(|s| s.strip_suffix(q)) {
            return inner;
        }
    }
    w
}

/// `NAME=value` / `NAME+=value` — a leading assignment, after which the next
/// word is still in command position.
fn is_shell_assignment(w: &str) -> bool {
    let Some((name, _)) = w.split_once('=') else {
        return false;
    };
    let name = name.strip_suffix('+').unwrap_or(name);
    let mut cs = name.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Is this command word one the analyser cannot read — i.e. the program that
/// runs is chosen by an expansion? `$X`, `${X}`, `"$X"`, `$(…)`, `` `…` ``,
/// `$'…'`, `$@`, `$_` all are. A path whose LAST component is literal
/// (`$HOME/.cargo/bin/cargo`) is not: the program's name is readable, and
/// the name-keyed checks still see it (`/` is a word boundary for the rm
/// check, so `$D/rm` with a dangerous target is caught there).
fn unreadable_command_word(word: &str) -> bool {
    let w = word.strip_prefix('"').unwrap_or(word);
    // Where does the leading expansion end?
    let after = if let Some(inner) = w.strip_prefix('`') {
        match inner.find('`') {
            Some(end) => Some(&inner[end + 1..]),
            None => return true,
        }
    } else if let Some(inner) = w.strip_prefix("$(") {
        let mut depth = 1usize;
        let end = inner.char_indices().find_map(|(i, c)| {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(i)
        });
        match end {
            Some(end) => Some(&inner[end + 1..]),
            None => return true,
        }
    } else if let Some(rest) = w.strip_prefix('$').filter(|r| !r.is_empty()) {
        variable_expansion_tail(rest)
    } else {
        return false;
    };
    let Some(after) = after else {
        return true;
    };
    let after = after.trim_end_matches('"');
    match after.strip_prefix('/') {
        Some(path) => {
            let last = path.rsplit('/').next().unwrap_or("");
            last.is_empty() || last.contains(['$', '`'])
        }
        None => true,
    }
}

/// The text after a leading `$NAME` / `${…}` expansion, or `None` for a
/// special form (`$'…'`, `$@`, `$_`, an unclosed `${`) that is never a
/// readable name.
fn variable_expansion_tail(rest: &str) -> Option<&str> {
    if let Some(inner) = rest.strip_prefix('{') {
        return inner.find('}').map(|end| &inner[end + 1..]);
    }
    let n = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (n > 0).then(|| &rest[n..])
}

/// Drop here-document bodies (they are data, not commands) unless the
/// here-doc feeds a shell (`bash <<EOF`), whose body IS commands.
fn strip_heredoc_bodies(cmd: &str) -> String {
    let mut out = String::new();
    let mut pending: Vec<String> = Vec::new();
    let mut skipping: Option<String> = None;
    for line in cmd.lines() {
        if let Some(delim) = &skipping {
            if line.trim() == delim {
                skipping = pending.pop();
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
        let first = line.split_whitespace().next().unwrap_or("");
        let feeds_shell = INLINE_SHELLS.contains(&first.rsplit('/').next().unwrap_or(""));
        let mut rest = line;
        while let Some(pos) = rest.find("<<") {
            let after = &rest[pos + 2..];
            rest = after;
            if let Some(here_string) = after.strip_prefix('<') {
                rest = here_string;
                continue;
            }
            let delim: String = after
                .trim_start_matches('-')
                .trim_start()
                .trim_start_matches(['\'', '"'])
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !delim.is_empty() && !feeds_shell {
                pending.insert(0, delim);
            }
        }
        if skipping.is_none() {
            skipping = pending.pop();
        }
    }
    out
}

/// Day 225: the inverse probe of every name-keyed check above. Each of them
/// recognises a dangerous command by the SPELLING of its program name, so a
/// command word chosen by an expansion — `X=rm; $X -rf ~`,
/// `BASH_ARGV0=rm; $BASH_ARGV0 -rf ~`, `$(echo rm) -rf ~` — slipped past all
/// of them while the plain spelling was flagged. The analyser cannot know
/// what such a word runs, so it says so: the command goes to the same
/// confirm path as any other flagged command (never the hard-deny tier — an
/// expansion it cannot read is not one it knows to be catastrophic).
/// `eval …` and `bash/sh -c '…'` strings are re-checked as commands, since an
/// expansion inside them is in command position when they run.
///
/// The trade, stated: `"$CARGO" build` / `$EDITOR file` are flagged too —
/// the analyser cannot tell a user's `$CARGO` from a model's `$X`. That only
/// changes behaviour where a flag does: an interactive prompt gains a
/// warning line (it already asked), a sub-agent child under a confirming
/// parent refuses, and plan/read mode blocks it. Piped `-p` runs — the
/// evolve loop's — have no confirm callback, so their bash is unaffected.
pub(super) fn check_indirect_command_word(cmd: &str, _cmd_lower: &str) -> Option<String> {
    let cmd = strip_heredoc_bodies(cmd);
    let tokens = lex_shell_words(&cmd);
    let mut at_cmd = true;
    let mut after_wrapper = false;
    let mut i = 0;
    while i < tokens.len() {
        let w = match &tokens[i] {
            ShellToken::Boundary => {
                at_cmd = true;
                after_wrapper = false;
                i += 1;
                continue;
            }
            ShellToken::Word(w) => w.as_str(),
        };
        i += 1;
        if !at_cmd {
            continue;
        }
        if COMMAND_POSITION_KEYWORDS.contains(&w)
            || is_shell_assignment(w)
            || (after_wrapper && w.starts_with('-'))
        {
            continue;
        }
        if unreadable_command_word(w) {
            let shown: String = w.chars().take(60).collect();
            return Some(format!(
                "Unanalyzable command: the program to run comes from the expansion `{shown}`, \
                 so the safety check cannot see what it is"
            ));
        }
        let base = strip_outer_quotes(w).rsplit('/').next().unwrap_or("");
        // The rest of this command segment, as words.
        let segment: Vec<&str> = tokens[i..]
            .iter()
            .map_while(|t| match t {
                ShellToken::Word(s) => Some(s.as_str()),
                ShellToken::Boundary => None,
            })
            .collect();
        let inner = if base == "eval" {
            // Day 225 exemption, kept rather than overridden: `eval "$(tool)"`
            // is the load-what-a-tool-prints idiom (ssh-agent, direnv, brew
            // shellenv), the program printing it is readable, and
            // `test_analyze_eval_fetch` already pins `eval $(cat local_script.sh)`
            // as unflagged. Residue, stated: the substitution's OUTPUT is
            // still unread, so `eval "$(printf …)"` assembling a command
            // passes this analyser. With a user deny list set it is refused
            // instead, by `eval_of_substitution` (#1011).
            let whole_substitution = |s: &&str| {
                let s = strip_outer_quotes(s);
                (s.starts_with("$(") && s.ends_with(')'))
                    || (s.len() > 1 && s.starts_with('`') && s.ends_with('`'))
            };
            (!segment.is_empty() && !segment.iter().all(whole_substitution)).then(|| {
                segment
                    .iter()
                    .map(|s| strip_outer_quotes(s))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        } else if INLINE_SHELLS.contains(&base) {
            segment
                .iter()
                .position(|s| s.starts_with('-') && !s.starts_with("--") && s.contains('c'))
                .and_then(|p| segment.get(p + 1))
                .map(|s| strip_outer_quotes(s).to_string())
        } else {
            None
        };
        if let Some(reason) = inner.and_then(|s| check_indirect_command_word(&s, "")) {
            return Some(reason);
        }
        if RM_COMMAND_WRAPPERS.contains(&base) {
            after_wrapper = true;
        } else {
            at_cmd = false;
            after_wrapper = false;
        }
    }
    None
}

/// Declaration builtins whose arguments are assignments, not a command
/// (`export A=1 && rm …` — the `export` segment runs nothing).
const DECLARATION_KEYWORDS: &[&str] = &["export", "declare", "typeset", "readonly", "local"];

/// #1010: the command segments a user deny pattern is matched against, as the
/// shell would run them. Quote-aware (the same lexer as the analyser, so
/// `echo "a; rm …"` is ONE segment) and heredoc bodies are dropped unless they
/// feed a shell. Per segment, leading `NAME=value` words, a leading
/// declaration keyword with its flags/assignments, and command-position
/// keywords (`if`, `then`, `{` …) are stripped; a segment left empty runs
/// nothing and is omitted. Spelling normalisation, not interpretation.
fn deny_segments(cmd: &str) -> Vec<String> {
    let cmd = strip_heredoc_bodies(cmd);
    let mut out = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut in_decl = false;
    let push = |cur: &mut Vec<String>, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur).join(" "));
        }
    };
    for tok in lex_shell_words(&cmd) {
        match tok {
            ShellToken::Boundary => {
                push(&mut cur, &mut out);
                in_decl = false;
            }
            ShellToken::Word(w) => {
                if cur.is_empty() {
                    if in_decl || COMMAND_POSITION_KEYWORDS.contains(&w.as_str()) {
                        // `export -n A=1 B`: the whole segment declares.
                        continue;
                    }
                    if is_shell_assignment(&w) {
                        continue;
                    }
                    if DECLARATION_KEYWORDS.contains(&w.as_str()) {
                        in_decl = true;
                        continue;
                    }
                }
                cur.push(w);
            }
        }
    }
    push(&mut cur, &mut out);
    out
}

/// #1011: the first deny segment that runs `eval` with a command substitution
/// (`$(...)` or backticks) anywhere in its arguments, or `None`. Segments come
/// from [`deny_segments`], so `VAR=` / `export` prefixes are already stripped;
/// `builtin` / `command` in front of `eval` are skipped, and `bash -c '…'`
/// strings are searched too. Unlike the analyser's `whole_substitution`
/// exemption, this is only consulted when the user has a deny list.
fn eval_of_substitution(cmd: &str) -> Option<String> {
    for seg in deny_segments(cmd) {
        let words: Vec<String> = lex_shell_words(&seg)
            .into_iter()
            .filter_map(|t| match t {
                ShellToken::Word(w) => Some(w),
                ShellToken::Boundary => None,
            })
            .collect();
        let start = words
            .iter()
            .position(|w| w != "builtin" && w != "command")
            .unwrap_or(words.len());
        let Some(program) = words.get(start) else {
            continue;
        };
        let base = strip_outer_quotes(program).rsplit('/').next().unwrap_or("");
        let args = &words[start + 1..];
        if base == "eval" && args.iter().any(|a| a.contains("$(") || a.contains('`')) {
            return Some(seg);
        }
        if INLINE_SHELLS.contains(&base) {
            let inner = args
                .iter()
                .position(|s| s.starts_with('-') && !s.starts_with("--") && s.contains('c'))
                .and_then(|p| args.get(p + 1));
            if let Some(found) = inner.and_then(|s| eval_of_substitution(strip_outer_quotes(s))) {
                return Some(found);
            }
        }
    }
    None
}

/// #1010: the part of the user deny check that the whole-string glob cannot
/// do. Called only after that glob found nothing and only with a non-empty
/// deny list. (1) Each segment from [`deny_segments`] is matched against each
/// pattern — catches `cd x && rm …`, `TZ=… rm …`. (2) Otherwise, a command
/// word the analyser cannot read (`$X`, `$(…)`, backticks) is refused: a deny
/// list cannot check a program name it cannot see, and a fence any `$VAR`
/// walks through is not a fence. Not seen: `eval "$(printf …)"` assembling a
/// command string (the analyser's `eval` exemption is kept). Model-supplied
/// text in the message is sanitized for the terminal.
pub(crate) fn user_deny_spelling_refusal(
    user_deny: &[String],
    command: &str,
    glob_match: impl Fn(&str, &str) -> bool,
) -> Option<String> {
    if user_deny.is_empty() {
        return None;
    }
    for seg in deny_segments(command) {
        if let Some(p) = user_deny.iter().find(|p| glob_match(p, &seg)) {
            let shown = crate::cli::sanitize_for_display(&seg);
            return Some(format!(
                "Command blocked by your permission rules: the part `{shown}` matches deny pattern '{p}'."
            ));
        }
    }
    if let Some(seg) = eval_of_substitution(command) {
        let shown: String = crate::cli::sanitize_for_display(&seg)
            .chars()
            .take(80)
            .collect();
        return Some(format!(
            "Command blocked by your permission rules: `{shown}` runs eval on a command \
             substitution, so the text that actually runs only exists at runtime and a deny \
             list cannot check it. Write the command plainly; for setup like \
             eval \"$(ssh-agent)\", run it outside yoyo, or start this session without a deny list."
        ));
    }
    let reason = check_indirect_command_word(command, "")?;
    Some(format!(
        "Command blocked by your permission rules: {}. A deny list cannot check a program \
         name that comes from a shell expansion ($X, $(...), backticks); write the command plainly.",
        crate::cli::sanitize_for_display(&reason)
    ))
}

#[cfg(test)]
mod tests {
    use super::super::analyze_bash_command;

    /// Day 225 inverse probe: does variable indirection slip past what the
    /// plain spelling trips? Measured BEFORE the fix (raw verdicts, recorded
    /// in the commit): both plain rows flagged; `eval "…"`, `bash -c "…"`
    /// and `sh -c '…'` already flagged by the rm check (its quote boundary);
    /// `X=rm; $X`, `X=rm; ${X}`, `BASH_ARGV0=rm; $BASH_ARGV0`, `$(echo rm)`
    /// and `` `echo rm` `` all returned None — a bypass. They now route to
    /// the ordinary confirm tier via `check_indirect_command_word`.
    #[test]
    fn indirection_probe_plain_vs_indirect_twins() {
        let unreadable = |shown: &str| {
            Some(format!(
                "Unanalyzable command: the program to run comes from the expansion `{shown}`, \
                 so the safety check cannot see what it is"
            ))
        };
        let home = Some("Destructive command: force-recursive delete targeting '~'".to_string());
        let root = Some("Destructive command: force-recursive delete targeting '/'".to_string());
        let rows: &[(&str, Option<String>)] = &[
            // plain
            ("rm -rf ~", home.clone()),
            ("rm -rf /", root.clone()),
            // indirect twins
            ("X=rm; $X -rf ~", unreadable("$X")),
            ("X=rm; ${X} -rf ~", unreadable("${X}")),
            (
                "BASH_ARGV0=rm; $BASH_ARGV0 -rf ~",
                unreadable("$BASH_ARGV0"),
            ),
            ("eval \"rm -rf ~\"", home.clone()),
            ("$(echo rm) -rf ~", unreadable("$(echo rm)")),
            ("`echo rm` -rf ~", unreadable("`echo rm`")),
            ("bash -c \"rm -rf ~\"", home.clone()),
            ("sh -c 'rm -rf /'", root.clone()),
            // further twins found while probing (None before the fix)
            ("X=rm $X -rf ~", unreadable("$X")),
            ("X=rm; \"$X\" -rf /", unreadable("\"$X\"")),
            ("eval $CMD", unreadable("$CMD")),
            ("eval \"$X -rf ~\"", unreadable("$X")),
            ("bash -c '$X -rf ~'", unreadable("$X")),
            ("sudo $X -rf ~", unreadable("$X")),
            ("true && $X -rf ~", unreadable("$X")),
            ("if true; then $X -rf ~; fi", unreadable("$X")),
            ("$'\\x72m' -rf ~", unreadable("$'\\x72m'")),
            ("set -- rm -rf ~; \"$@\"", unreadable("\"$@\"")),
            ("bash <<EOF\n$X -rf ~\nEOF", unreadable("$X")),
        ];
        for (cmd, want) in rows {
            assert_eq!(&analyze_bash_command(cmd), want, "verdict for {cmd:?}");
        }
    }

    /// Near-misses for the Day 225 probe: expansions that are ARGUMENTS, data
    /// or a readable program name must stay unflagged, byte-identically.
    #[test]
    fn indirection_probe_near_misses_stay_unflagged() {
        let rows = [
            "echo $HOME",
            "X=1; echo $X",
            "cd \"$DIR\" && ls",
            "cargo test",
            "cargo build 2>&1 | tail -5",
            "for f in $FILES; do echo $f; done",
            "echo \"$(date)\" > /tmp/stamp",
            "x=$(pwd); echo $x",
            "[ -n \"$X\" ] && echo yes",
            "'$X' literal-name",
            "$HOME/.cargo/bin/cargo build",
            "\"$(dirname \"$0\")/run.sh\" --fast",
            "eval echo hi",
            "eval \"$(ssh-agent -s)\"",
            "eval $(cat local_script.sh)",
            "bash -c 'echo $X'",
            "cat > s.sh <<'EOF'\n$X -rf ~\nEOF\necho done",
            "grep -c x <<< \"$Y\"",
        ];
        for cmd in rows {
            assert_eq!(analyze_bash_command(cmd), None, "verdict for {cmd:?}");
        }
    }

    /// The stated trade (see `check_indirect_command_word`): a user-env
    /// variable as the program is indistinguishable from a model-chosen one,
    /// so `"$CARGO" build` was None before Day 225 and is flagged now. In
    /// piped `-p` runs (the evolve loop's) a flag does not block anything; it
    /// matters in an interactive prompt, a confirming parent's sub-agent
    /// child, and plan/read mode.
    #[test]
    fn indirection_probe_user_env_program_is_the_stated_trade() {
        assert_eq!(
            analyze_bash_command("\"$CARGO\" build"),
            Some(
                "Unanalyzable command: the program to run comes from the expansion \
                 `\"$CARGO\"`, so the safety check cannot see what it is"
                    .to_string()
            )
        );
    }
}
