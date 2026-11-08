//! The hard deny list: commands refused with no confirm and no `--yes`
//! override, on both the parent bash (`StreamingBashTool`) and the
//! sub-agent / explore-agent child bash (#977). One predicate, one door:
//! [`hard_deny_refusal`] is the only caller of [`hard_deny_match`].
//!
//! # Why a token matcher and not `command.contains(pattern)`
//!
//! Until Day 217 this list was matched as plain substrings, which failed in
//! both directions at once:
//!
//! - **Overmatch** (what users hit): the pattern for deleting root matched
//!   every recursive delete of an absolute path (`rm -rf /tmp/build`), any
//!   command that merely *mentioned* it (a `grep` for it, an `echo`, a commit
//!   message), `dd if=` blocked an ordinary `dd if=/dev/zero of=./img`, and
//!   the word `mkfs` anywhere (`man mkfs`) was refused.
//! - **Undermatch** (the reason the list exists): `rm -fr /`, `rm -r -f /`,
//!   `rm -Rf /` and `rm -rf --no-preserve-root /` contain no matching
//!   substring, so the catastrophe the list was written for got through.
//!
//! # How it matches
//!
//! The command is split into logical lines on UNQUOTED newlines only (a
//! `\`-newline continuation is joined first; quote state is tracked across
//! lines, #1101), each line into shell words with quote removal (`'…'`,
//! `"…"`, `$'…'`, backslash escapes), and the words into segments on `;`,
//! `&`, `|`, `(`, `)`. A quoted string is therefore one WORD whether or not
//! it spans lines: `echo "rm -rf /"` has an argument whose text is
//! `rm -rf /`, and no word in it equals `rm`; a `git commit -m "…"` whose
//! body has that text on a line of its own is the same. A `#` comment and a
//! heredoc body are not quote-tracked, so an apostrophe there cannot pair up
//! with another across lines and hide the line between.
//!
//! - **rm-root** (`rm -rf /`, or `rm -rf /*` when the target is exactly
//!   `/*`): a word whose basename is `rm`, anywhere in the segment (so
//!   `sudo -u root rm …` and `xargs rm …` are covered), followed by a
//!   recursive flag (`-r`, `-R`, combined like `-fR`, or `--recursive`) and a
//!   target that is root or home: `/`, `/*`, `/.`, `//`, `~`, `~/`, `~/*`,
//!   `$HOME`, `${HOME}` (with or without a trailing `/` or `/*`).
//!   `--no-preserve-root` alone also matches. `-f` is NOT required.
//! - **mkfs** (`mkfs`): a word that is `mkfs` or `mkfs.<fs>` in command
//!   position (after `sudo`/`env`/`nice`/… wrappers, flags and `VAR=x`
//!   assignments), or anywhere when a later word in the segment starts with
//!   `/dev/`. `man mkfs` and `cat notes_about_mkfs.md` pass.
//! - **dd** (`dd if=`, the label kept from the old list): a word `dd`
//!   followed by `of=/dev/…` other than `/dev/null`, `/dev/stdout`,
//!   `/dev/stderr`. Reading a disk or writing an image file passes.
//! - **Fork bomb** (`:(){:|:&};:`): kept as a substring rule, but matched
//!   with whitespace removed, so the classic spaced form `:(){ :|:& };:` is
//!   now caught too. This is strictly tighter than before; a quoted mention
//!   is still refused (fails closed).
//! - **Nested shells:** `bash`/`sh`/`zsh`/`dash`/`ksh` with a `-c` flag, or
//!   `eval`, re-run the matcher on the script string (depth cap
//!   [`MAX_NESTING`]; past the cap, the whitespace-free text is scanned for
//!   the old patterns, which fails closed). Quoting cannot become a bypass.
//!
//! # Known residue (deliberate, not fixed)
//!
//! - A heredoc BODY line that reads like a command (`rm -rf /` on its own
//!   line in a markdown file being written) is tokenized as a command and
//!   refused. That fails closed, which is the safe direction; its
//!   incidence on mentions is counted on #1102. Remedy for
//!   the refused user or model: write the text with `write_file` and pass
//!   the path (`git commit -F msg.txt`, `gh issue create --body-file f`).
//! - A quoted argument that spans lines (`python3 -c "…"`, a
//!   `git commit -m "…"` with a body, `gh issue create --body "…"`) is one
//!   word (#1101). The verdict for a multi-line command equals the verdict
//!   for the same command with each QUOTED newline written as a space. The
//!   cost, decided and stated: a multi-line `python3 -c "…"` whose inner
//!   program runs the pattern passes, exactly as its one-line form always
//!   has, because this guard reads direct commands, not inner programs
//!   (nested `bash -c`/`sh -c`/`eval` ARE re-read, with their own newlines).
//!   Superseded, recorded rather than erased: until #1101 quote removal was
//!   per physical line, every interior line was tokenized as a command, and
//!   this bullet said "Parsing quotes across lines is a separate decision,
//!   not made here."
//! - Command substitution `$(…)` and backticks are not parsed. A command
//!   containing them, or a quote that never closes, is ALSO scanned a
//!   second time (per physical line and per logical line) with quote
//!   characters removed and `$(`, `)` and backticks turned into separators — so `git commit -m "… $(x) rm -rf /"` is
//!   refused. That too fails closed.
//! - Other dangerous deletes (`rm -rf /usr`, `find / -delete`, `> /dev/sda`)
//!   are not on this list and never were.
//!
//! This is a guard against accidental catastrophe, **not a sandbox**: a
//! determined author can still write a command it does not recognise.
//! `safety.rs`'s `analyze_bash_command` and the confirm prompt still run for
//! the parent, and the child inherits the confirm rule (#977).

/// The hard deny rule labels: the authority read by
/// `StreamingBashTool::default()` (its `deny_patterns`, which selects the
/// active rules) and by the sub-agent child's bash (#977). Each label names
/// one rule in [`hard_deny_match`]; they are labels, not substrings.
pub(crate) const HARD_DENY_PATTERNS: &[&str] = &[
    RULE_RM_ROOT,
    RULE_RM_ROOT_GLOB,
    RULE_MKFS,
    RULE_DD,
    RULE_FORK_BOMB,
];

const RULE_RM_ROOT: &str = "rm -rf /";
const RULE_RM_ROOT_GLOB: &str = "rm -rf /*";
const RULE_MKFS: &str = "mkfs";
const RULE_DD: &str = "dd if=";
const RULE_FORK_BOMB: &str = ":(){:|:&};:";

/// How deep `bash -c "…"` / `eval "…"` nesting is followed before the
/// fail-closed substring fallback takes over.
const MAX_NESTING: usize = 3;

/// The hard-deny predicate and refusal text, shared by parent and child bash.
/// `patterns` selects which rule labels are active (the default is all of
/// [`HARD_DENY_PATTERNS`]).
///
/// The text names the rule in words true of every spelling it catches, and
/// quotes the segment that tripped it (#1102). Superseded, recorded rather
/// than erased: until Day 253 it said "contains '<label>'", which was false
/// for every spelling but the label's own (`rm -rf ~` was told it "contains
/// 'rm -rf /'"), so a user searching their command for it found nothing,
/// and a multi-line command gave no hint which line had tripped it.
pub(crate) fn hard_deny_refusal<S: AsRef<str>>(patterns: &[S], command: &str) -> Option<String> {
    let hit = hard_deny_match(command)?;
    patterns.iter().find(|p| p.as_ref() == hit.label)?;
    let shown = quote_for_refusal(&hit.text);
    Some(if hit.unparsed {
        format!(
            "Command blocked by safety policy: `{shown}` contains '{}' inside shell nesting \
             deeper than {MAX_NESTING} levels, which is not parsed. This is denied for safety.",
            hit.label
        )
    } else {
        format!(
            "Command blocked by safety policy: `{shown}` reads as {}. This is denied for safety.",
            rule_description(hit.label)
        )
    })
}

/// Longest quoted segment in a refusal, in bytes, after escaping.
const REFUSAL_QUOTE_MAX_BYTES: usize = 120;

/// Sanitize first, then cut on a char boundary (escaping lengthens the text;
/// the same order as `cli::hook_command_for_display`), marking a cut with `…`.
fn quote_for_refusal(text: &str) -> String {
    let safe = crate::cli::sanitize_for_display(text.trim());
    let head = crate::format::safe_truncate(&safe, REFUSAL_QUOTE_MAX_BYTES);
    if head.len() < safe.len() {
        format!("{head}…")
    } else {
        safe
    }
}

/// Each rule in words true of every spelling [`match_segment`] catches for
/// it (see the module doc's "How it matches").
fn rule_description(label: &str) -> &'static str {
    match label {
        RULE_RM_ROOT => {
            "a recursive delete of the filesystem root or the home directory \
             (/, ~ or $HOME, or any rm given --no-preserve-root)"
        }
        RULE_RM_ROOT_GLOB => "a recursive delete of everything under the filesystem root (/*)",
        RULE_MKFS => "formatting a filesystem (mkfs)",
        RULE_DD => "dd writing onto a device (of=/dev/...)",
        RULE_FORK_BOMB => "a fork bomb",
        _ => "a hard-denied command",
    }
}

/// What [`hard_deny_match`] found: the rule's label, the text that tripped
/// it (the shell segment as written; for `bash -c`/`eval`, the inner
/// script's segment), and whether it came from the unparsed scan past
/// [`MAX_NESTING`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HardDenyHit {
    pub label: &'static str,
    pub text: String,
    pub unparsed: bool,
}

/// Returns the hard-deny rule `command` matches, if any, with the matched text.
pub(crate) fn hard_deny_match(command: &str) -> Option<HardDenyHit> {
    match_at_depth(command, 0)
}

fn match_at_depth(command: &str, depth: usize) -> Option<HardDenyHit> {
    let squeeze = |t: &str| -> String { t.chars().filter(|c| !c.is_whitespace()).collect() };
    if squeeze(command).contains(RULE_FORK_BOMB) {
        // The first physical line that carries it, or the whole command when
        // the bomb itself spans lines.
        let text = command
            .lines()
            .find(|l| squeeze(l).contains(RULE_FORK_BOMB))
            .unwrap_or(command);
        return Some(HardDenyHit {
            label: RULE_FORK_BOMB,
            text: text.to_string(),
            unparsed: false,
        });
    }
    if depth > MAX_NESTING {
        // Fail closed: past the nesting cap, fall back to the old scan.
        let spaced = command.split_whitespace().collect::<Vec<_>>().join(" ");
        return [RULE_RM_ROOT, RULE_MKFS, RULE_DD]
            .into_iter()
            .find(|p| spaced.contains(p))
            .map(|label| HardDenyHit {
                label,
                text: command.to_string(),
                unparsed: true,
            });
    }
    let joined = command.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut needs_desugar = joined.contains("$(") || joined.contains('`');
    let logical = logical_lines(&joined);
    for line in &logical {
        let (segments, balanced) = split_line(line);
        needs_desugar |= !balanced;
        for seg in &segments {
            if let Some(hit) = match_segment(seg, line, depth) {
                return Some(hit);
            }
        }
    }
    if needs_desugar {
        let desugar = |text: &str| -> String {
            text.chars()
                .filter(|c| *c != '\'' && *c != '"')
                .map(|c| if c == '`' { ';' } else { c })
                .collect::<String>()
                .replace("$(", ";")
                .replace(')', ";")
        };
        // Two scans, refuse if either does: every PHYSICAL line (a `$(`
        // inside quotes runs its own newline-separated commands, and an
        // unbalanced quote makes the quote state meaningless), and every
        // LOGICAL line (a quoted newline is then plain whitespace, so the
        // verdict is never looser than the one-line form's).
        let physical = desugar(&joined);
        let scans = physical
            .lines()
            .map(String::from)
            .chain(logical.iter().map(|l| desugar(l)));
        for line in scans {
            for seg in &split_line(&line).0 {
                if let Some(hit) = match_segment(seg, &line, depth) {
                    return Some(hit);
                }
            }
        }
    }
    None
}

/// Splits `text` on UNQUOTED newlines only (#1101): a newline inside `'…'`,
/// `"…"` or `$'…'` stays inside its word, so a multi-line quoted argument
/// (a commit body, a `--body`) is one word, as it is on one line. Outside
/// quotes a backslash escapes the next char. Two things are deliberately
/// NOT quote-tracked, so an apostrophe in them cannot pair up across lines
/// and hide the line between: a `#` comment (to the end of its line), and a
/// heredoc body, each of whose lines is emitted as a logical line of its own
/// (tokenized as a command, which fails closed). An unclosed quote runs to
/// the end; `split_line` then reports it unbalanced and the caller falls
/// back to the per-physical-line desugar scan.
fn logical_lines(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut heredocs: Vec<(String, bool)> = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '\n' => {
                lines.push(std::mem::take(&mut cur));
                i += 1;
                for (delim, strip_tabs) in heredocs.drain(..) {
                    while i < n {
                        let end = chars[i..]
                            .iter()
                            .position(|&c| c == '\n')
                            .map_or(n, |p| i + p);
                        let body: String = chars[i..end].iter().collect();
                        i = (end + 1).min(n);
                        let trimmed = body.trim_end_matches('\r');
                        let trimmed = if strip_tabs {
                            trimmed.trim_start_matches('\t')
                        } else {
                            trimmed
                        };
                        let done = trimmed == delim;
                        lines.push(body);
                        if done {
                            break;
                        }
                    }
                }
            }
            '\'' | '"' => {
                // `$'…'` (ANSI-C) honours backslash escapes like `"…"` does.
                let escapes = c == '"' || cur.ends_with('$');
                cur.push(c);
                i += 1;
                while i < n {
                    let q = chars[i];
                    cur.push(q);
                    i += 1;
                    if q == '\\' && escapes {
                        if let Some(&e) = chars.get(i) {
                            cur.push(e);
                            i += 1;
                        }
                    } else if q == c {
                        break;
                    }
                }
            }
            '\\' => {
                cur.push(c);
                i += 1;
                if let Some(e) = next {
                    cur.push(e);
                    i += 1;
                }
            }
            '#' if cur
                .chars()
                .last()
                .is_none_or(|p| p.is_whitespace() || ";&|()".contains(p)) =>
            {
                while i < n && chars[i] != '\n' {
                    cur.push(chars[i]);
                    i += 1;
                }
            }
            '<' if next == Some('<') && chars.get(i + 2) == Some(&'<') => {
                // A here-string (`<<<`), not a heredoc.
                cur.push_str("<<<");
                i += 3;
            }
            '<' if next == Some('<') => {
                cur.push_str("<<");
                i += 2;
                let strip_tabs = chars.get(i) == Some(&'-');
                if strip_tabs {
                    cur.push('-');
                    i += 1;
                }
                while i < n && (chars[i] == ' ' || chars[i] == '\t') {
                    cur.push(chars[i]);
                    i += 1;
                }
                let mut delim = String::new();
                while i < n && !chars[i].is_whitespace() && !";&|()<>".contains(chars[i]) {
                    cur.push(chars[i]);
                    if !matches!(chars[i], '\'' | '"' | '\\') {
                        delim.push(chars[i]);
                    }
                    i += 1;
                }
                if !delim.is_empty() {
                    heredocs.push((delim, strip_tabs));
                }
            }
            _ => {
                cur.push(c);
                i += 1;
            }
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// One shell segment: its words (quote removal applied) and the byte span
/// of the line they were read from, so a refusal can quote it as written.
struct Segment {
    words: Vec<String>,
    span: std::ops::Range<usize>,
}

/// Splits one line into segments of shell words (quote removal applied).
/// The bool is false when the line ends inside an open quote.
fn split_line(line: &str) -> (Vec<Segment>, bool) {
    let mut segments: Vec<Segment> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.char_indices().peekable();
    let mut balanced = true;
    // Byte span of the current segment: where its first word began, and
    // just past the last char any of its words consumed.
    let mut start: Option<usize> = None;
    let mut end = 0;
    let flush_word = |words: &mut Vec<String>, word: &mut String, in_word: &mut bool| {
        if *in_word {
            words.push(std::mem::take(word));
            *in_word = false;
        }
    };
    while let Some((at, c)) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                let mut closed = false;
                for (_, q) in chars.by_ref() {
                    if q == '\'' {
                        closed = true;
                        break;
                    }
                    word.push(q);
                }
                balanced &= closed;
            }
            '"' => {
                in_word = true;
                let mut closed = false;
                while let Some((_, q)) = chars.next() {
                    match q {
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\\' if matches!(chars.peek(), Some((_, '"' | '\\' | '$' | '`'))) => {
                            word.push(chars.next().map_or('\\', |(_, e)| e));
                        }
                        _ => word.push(q),
                    }
                }
                balanced &= closed;
            }
            '\\' => {
                in_word = true;
                if let Some((_, n)) = chars.next() {
                    word.push(n);
                }
            }
            ';' | '&' | '|' | '(' | ')' => {
                flush_word(&mut words, &mut word, &mut in_word);
                if !words.is_empty() {
                    let span = start.take().unwrap_or(at)..end;
                    segments.push(Segment {
                        words: std::mem::take(&mut words),
                        span,
                    });
                }
                continue;
            }
            c if c.is_whitespace() => {
                flush_word(&mut words, &mut word, &mut in_word);
                continue;
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
        // Every arm that reaches here consumed word text from `at` onwards.
        start.get_or_insert(at);
        end = chars.peek().map_or(line.len(), |(i, _)| *i);
    }
    flush_word(&mut words, &mut word, &mut in_word);
    if !words.is_empty() {
        segments.push(Segment {
            words,
            span: start.unwrap_or(0)..end,
        });
    }
    (segments, balanced)
}

fn basename(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

const WRAPPERS: &[&str] = &[
    "sudo", "doas", "env", "command", "builtin", "exec", "nice", "nohup", "time", "timeout",
    "ionice", "stdbuf", "xargs", "!", "{", "then", "do", "else", "if", "while", "until",
];

const SHELLS: &[&str] = &["bash", "sh", "zsh", "dash", "ksh"];

/// Index of the segment's command word: skips wrappers, their flags, numeric
/// arguments (`nice -n 10`, `timeout 5`) and leading `VAR=x` assignments.
fn command_index(seg: &[String]) -> Option<usize> {
    seg.iter().position(|w| {
        let b = basename(w);
        !(WRAPPERS.contains(&b)
            || w.starts_with('-')
            || w.chars().all(|c| c.is_ascii_digit() || c == '.')
            || (w.contains('=') && !w.starts_with('=')))
    })
}

/// Matches one segment of `line`. A direct match quotes the segment as
/// written (`line[segment.span]`); a nested `bash -c`/`eval` match carries
/// the inner script's own hit.
fn match_segment(segment: &Segment, line: &str, depth: usize) -> Option<HardDenyHit> {
    let seg = &segment.words;
    let direct = |label: &'static str| HardDenyHit {
        label,
        text: line.get(segment.span.clone()).unwrap_or(line).to_string(),
        unparsed: false,
    };
    let cmd_at = command_index(seg);
    for (i, w) in seg.iter().enumerate() {
        let b = basename(w);
        let rest = &seg[i + 1..];
        if b == "rm" {
            if let Some(label) = rm_root(rest) {
                return Some(direct(label));
            }
        }
        if b == "dd" && rest.iter().any(|a| dd_writes_device(a)) {
            return Some(direct(RULE_DD));
        }
        if (b == "mkfs" || b.starts_with("mkfs."))
            && (cmd_at == Some(i) || rest.iter().any(|a| a.starts_with("/dev/")))
        {
            return Some(direct(RULE_MKFS));
        }
        if SHELLS.contains(&b) {
            let script = rest
                .iter()
                .position(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('c'))
                .and_then(|f| rest.get(f + 1));
            if let Some(hit) = script.and_then(|s| match_at_depth(s, depth + 1)) {
                return Some(hit);
            }
        }
        if b == "eval" {
            if let Some(hit) = match_at_depth(&rest.join(" "), depth + 1) {
                return Some(hit);
            }
        }
    }
    None
}

/// The rm-root rule over the words after `rm`.
fn rm_root(args: &[String]) -> Option<&'static str> {
    let mut recursive = false;
    let mut root_target: Option<&'static str> = None;
    let mut options_done = false;
    for a in args {
        if !options_done && a == "--" {
            options_done = true;
        } else if !options_done && a == "--no-preserve-root" {
            return Some(RULE_RM_ROOT);
        } else if !options_done && a.starts_with("--") {
            recursive |= a == "--recursive";
        } else if !options_done && a.starts_with('-') && a.len() > 1 {
            recursive |= a.contains('r') || a.contains('R');
        } else if root_target.is_none() && is_root_or_home(a) {
            root_target = Some(if a == "/*" {
                RULE_RM_ROOT_GLOB
            } else {
                RULE_RM_ROOT
            });
        }
    }
    root_target.filter(|_| recursive)
}

fn is_root_or_home(target: &str) -> bool {
    let anchored = ["/", "~", "$HOME", "${HOME}"]
        .iter()
        .any(|p| target.starts_with(p));
    if !anchored {
        return false;
    }
    let mut t = target.strip_suffix("/*").unwrap_or(target);
    while let Some(s) = t.strip_suffix("/.").or_else(|| t.strip_suffix('/')) {
        t = s;
    }
    matches!(t, "" | "~" | "$HOME" | "${HOME}")
}

fn dd_writes_device(arg: &str) -> bool {
    arg.strip_prefix("of=").is_some_and(|dev| {
        dev.starts_with("/dev/") && !matches!(dev, "/dev/null" | "/dev/stdout" | "/dev/stderr")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule label a command matches: the verdict half of
    /// [`hard_deny_match`], which every verdict table below reads.
    fn label_of(cmd: &str) -> Option<&'static str> {
        hard_deny_match(cmd).map(|hit| hit.label)
    }

    /// (command, expected label). Every row must refuse with exactly this label.
    const MUST_REFUSE: &[(&str, &str)] = &[
        ("rm -rf /", RULE_RM_ROOT),
        ("rm -rf /*", RULE_RM_ROOT_GLOB),
        ("rm -fr /", RULE_RM_ROOT),
        ("rm -r -f /", RULE_RM_ROOT),
        ("rm -Rf /", RULE_RM_ROOT),
        ("rm -r /", RULE_RM_ROOT),
        ("rm --recursive --force /", RULE_RM_ROOT),
        ("rm -rf --no-preserve-root /", RULE_RM_ROOT),
        ("rm --no-preserve-root -r /tmp/../", RULE_RM_ROOT),
        ("rm -rf ~", RULE_RM_ROOT),
        ("rm -rf ~/", RULE_RM_ROOT),
        ("rm -rf $HOME", RULE_RM_ROOT),
        ("rm -rf \"$HOME\"/*", RULE_RM_ROOT),
        ("rm -rf /.", RULE_RM_ROOT),
        ("rm -rf //", RULE_RM_ROOT),
        ("rm / -rf", RULE_RM_ROOT),
        ("/bin/rm -rf /", RULE_RM_ROOT),
        ("\\rm -rf /", RULE_RM_ROOT),
        ("\"rm\" -rf \"/\"", RULE_RM_ROOT),
        ("sudo rm -rf /", RULE_RM_ROOT),
        ("sudo -u root rm -rf /", RULE_RM_ROOT),
        ("cd x && rm -rf /", RULE_RM_ROOT),
        ("true;rm -rf /", RULE_RM_ROOT),
        ("rm -rf / &", RULE_RM_ROOT),
        ("rm -rf \\\n  /", RULE_RM_ROOT),
        ("bash -c \"rm -rf /\"", RULE_RM_ROOT),
        ("sh -c 'rm -fr /*'", RULE_RM_ROOT_GLOB),
        ("bash -lc 'rm -r /'", RULE_RM_ROOT),
        ("eval 'rm -rf /'", RULE_RM_ROOT),
        ("bash -c \"sh -c 'rm -rf /'\"", RULE_RM_ROOT),
        ("echo \"$(rm -rf /)\"", RULE_RM_ROOT),
        ("echo `rm -rf /`", RULE_RM_ROOT),
        ("echo 'a\n'; rm -rf /", RULE_RM_ROOT),
        ("mkfs.ext4 /dev/sda1", RULE_MKFS),
        ("mkfs -t ext4 /dev/sdb", RULE_MKFS),
        ("sudo mkfs.ext4 disk.img", RULE_MKFS),
        ("sudo -u root mkfs.xfs /dev/nvme0n1", RULE_MKFS),
        ("dd if=/dev/zero of=/dev/sda", RULE_DD),
        ("sudo dd if=image.iso of=/dev/sdb bs=4M", RULE_DD),
        (":(){:|:&};:", RULE_FORK_BOMB),
        (":(){ :|:& };:", RULE_FORK_BOMB),
    ];

    /// Near misses: each must pass the hard deny list untouched.
    const MUST_PASS: &[&str] = &[
        "rm -rf /tmp/build",
        "rm -rf /home/u/proj/target",
        "rm -rf ./target",
        "rm -rf target/",
        "rm -rf *",
        "rm -rf \"\"",
        "rm -f /",
        "rm -rf ~/proj/target",
        "rm -rf $HOME/.cache/yoyo",
        "grep -rn 'rm -rf /' src/",
        "echo \"rm -rf /\"",
        "git commit -m \"guard against rm -rf /\"",
        "printf '%s\\n' 'rm -fr /*'",
        "dd if=/dev/zero of=./img bs=1M count=1",
        "dd if=/dev/sda of=disk.img",
        "dd if=/dev/urandom of=/dev/null count=1",
        "man mkfs",
        "which mkfs.ext4",
        "cat notes_about_mkfs.md",
        "grep -rn mkfs src/",
        "ls",
        "cargo test",
        "git status",
    ];

    #[test]
    fn must_refuse_rows_match_their_exact_label() {
        for (cmd, label) in MUST_REFUSE {
            assert_eq!(label_of(cmd), Some(*label), "should refuse: {cmd:?}");
        }
    }

    #[test]
    fn near_miss_rows_pass() {
        for cmd in MUST_PASS {
            assert_eq!(label_of(cmd), None, "should pass: {cmd:?}");
        }
    }

    #[test]
    fn every_listed_label_is_reachable_from_its_canonical_form() {
        // The canonical form of each list entry (the old substring itself,
        // completed into a real command where the label is a fragment).
        let canonical = [
            (RULE_RM_ROOT, "rm -rf /"),
            (RULE_RM_ROOT_GLOB, "rm -rf /*"),
            (RULE_MKFS, "mkfs /dev/sda"),
            (RULE_DD, "dd if=/dev/zero of=/dev/sda"),
            (RULE_FORK_BOMB, ":(){:|:&};:"),
        ];
        assert_eq!(canonical.len(), HARD_DENY_PATTERNS.len());
        for (label, cmd) in canonical {
            assert!(HARD_DENY_PATTERNS.contains(&label), "{label}");
            assert_eq!(label_of(cmd), Some(label), "{cmd}");
        }
    }

    /// #1102: the refusal quotes the segment that tripped it, as written, and
    /// names the rule in words true of that spelling. Superseded, recorded
    /// rather than erased: this was `refusal_text_is_byte_identical_to_the_old_format`,
    /// pinning "contains 'rm -rf /'" for every rm-root spelling.
    #[test]
    fn refusal_quotes_the_tilde_segment_not_the_rule_label() {
        let got = hard_deny_refusal(HARD_DENY_PATTERNS, "rm -rf ~").expect("refuses");
        assert_eq!(
            got,
            "Command blocked by safety policy: `rm -rf ~` reads as a recursive delete of the \
             filesystem root or the home directory (/, ~ or $HOME, or any rm given \
             --no-preserve-root). This is denied for safety."
        );
        assert!(!got.contains("rm -rf /"), "{got}");
        assert!(!got.contains("contains '"), "{got}");
        assert_eq!(hard_deny_refusal(HARD_DENY_PATTERNS, "rm -rf /tmp/x"), None);
    }

    #[test]
    fn refusal_quotes_the_root_segment_as_written() {
        let got = hard_deny_refusal(HARD_DENY_PATTERNS, "rm -fr /").expect("refuses");
        assert!(got.starts_with("Command blocked by safety policy: `rm -fr /` reads as "));
        // Only the segment that tripped it, with its wrapper, not the chain.
        let got = hard_deny_refusal(HARD_DENY_PATTERNS, "cd x && sudo rm -rf \"$HOME\"/*")
            .expect("refuses");
        assert!(got.contains("`sudo rm -rf \"$HOME\"/*` reads as"), "{got}");
        // The glob rule and the other rules are named by what they do.
        let cases = [
            (
                "rm -rf /*",
                "`rm -rf /*` reads as a recursive delete of everything under",
            ),
            (
                "mkfs.ext4 /dev/sda1",
                "`mkfs.ext4 /dev/sda1` reads as formatting a filesystem",
            ),
            (
                "dd if=/dev/zero of=/dev/sda",
                "`dd if=/dev/zero of=/dev/sda` reads as dd writing onto a device",
            ),
            (":(){ :|:& };:", "`:(){ :|:& };:` reads as a fork bomb"),
        ];
        for (cmd, want) in cases {
            let got = hard_deny_refusal(HARD_DENY_PATTERNS, cmd).expect("refuses");
            assert!(got.contains(want), "{cmd:?}: {got}");
        }
    }

    #[test]
    fn refusal_quotes_only_the_matching_line_of_a_multi_line_command() {
        let cmd = "cat <<EOF > notes.md\nfirst line\nrm -rf ~/\nlast line\nEOF";
        let got = hard_deny_refusal(HARD_DENY_PATTERNS, cmd).expect("refuses");
        assert!(got.contains("`rm -rf ~/` reads as"), "{got}");
        for other in ["first line", "last line", "notes.md", "EOF"] {
            assert!(!got.contains(other), "quoted more than the line: {got}");
        }
        // A nested shell quotes the inner script's segment.
        let got = hard_deny_refusal(HARD_DENY_PATTERNS, "bash -c \"echo hi; rm -r $HOME\"")
            .expect("refuses");
        assert!(got.contains("`rm -r $HOME` reads as"), "{got}");
        // The desugar path (`$(`) quotes the segment, not the whole line.
        let got =
            hard_deny_refusal(HARD_DENY_PATTERNS, "echo \"$(rm -rf ~)\" done").expect("refuses");
        assert!(got.contains("`rm -rf ~` reads as"), "{got}");
    }

    #[test]
    fn refusal_quote_is_capped_char_safely_and_escaped() {
        // A multi-byte char straddles the 120-byte cap: no panic, cut marked.
        let target = format!("rm -rf / {}", "é".repeat(80));
        let got = hard_deny_refusal(HARD_DENY_PATTERNS, &target).expect("refuses");
        assert!(got.contains("…` reads as"), "{got}");
        let quoted = got
            .split('`')
            .nth(1)
            .expect("quoted segment")
            .trim_end_matches('…');
        assert!(quoted.len() <= REFUSAL_QUOTE_MAX_BYTES, "{}", quoted.len());
        for cap_shift in 0..4 {
            let cmd = format!("rm -rf / {}{}", "a".repeat(cap_shift), "é".repeat(80));
            assert!(hard_deny_refusal(HARD_DENY_PATTERNS, &cmd).is_some());
        }
        // Control bytes cannot reach the terminal.
        let got = hard_deny_refusal(HARD_DENY_PATTERNS, "rm -rf / \u{1b}[2J").expect("refuses");
        assert!(!got.contains('\u{1b}'), "{got:?}");
        assert!(got.contains("\\x1b"), "{got:?}");
    }

    /// The message changed, the verdict did not: every refuse row still
    /// refuses through the real door, and every pass row still passes.
    #[test]
    fn refusal_verdicts_are_unchanged_by_the_new_message() {
        for (cmd, _) in MUST_REFUSE {
            let got = hard_deny_refusal(HARD_DENY_PATTERNS, cmd);
            assert!(
                got.as_deref()
                    .is_some_and(|m| m.starts_with("Command blocked by safety policy: `")),
                "should refuse: {cmd:?} -> {got:?}"
            );
        }
        for cmd in MUST_PASS {
            assert_eq!(hard_deny_refusal(HARD_DENY_PATTERNS, cmd), None, "{cmd:?}");
        }
    }

    #[test]
    fn nesting_past_the_cap_says_it_was_not_parsed() {
        let mut cmd = String::from("rm -rf /");
        for _ in 0..6 {
            cmd = format!("eval {cmd:?}");
        }
        let got = hard_deny_refusal(HARD_DENY_PATTERNS, &cmd).expect("refuses");
        assert!(
            got.contains("inside shell nesting deeper than 3 levels"),
            "{got}"
        );
    }

    #[test]
    fn disabled_rule_is_not_refused() {
        // `deny_patterns` selects the active rules: a list without the
        // mkfs label does not refuse mkfs, and still refuses rm-root.
        let only_rm = [RULE_RM_ROOT];
        assert_eq!(hard_deny_refusal(&only_rm, "mkfs /dev/sda"), None);
        assert!(hard_deny_refusal(&only_rm, "rm -rf /").is_some());
    }

    /// #1068 defect 2, decided by #1101: quote state is tracked across lines,
    /// so a double-quoted argument that spans lines is ONE word, as it is on
    /// one line. Superseded: until #1101 this test was
    /// `multi_line_quoted_argument_fails_closed` and pinned that the interior
    /// line was refused. The stated cost, pinned here: an inner program that
    /// runs the pattern passes, exactly as its one-line form passes.
    #[test]
    fn multi_line_quoted_argument_is_one_word() {
        let cmd = "python3 -c \"print(1)\nrm -rf /\nprint(2)\"";
        // Anti-vacuous: the fixture really spans lines and really carries the
        // pattern on an interior line of its own.
        assert!(cmd.contains('\n'), "fixture must span lines: {cmd:?}");
        assert_eq!(cmd.lines().nth(1), Some("rm -rf /"), "{cmd:?}");
        assert_eq!(label_of(cmd), None, "{cmd:?}");
        // The same text through the real refusal door.
        assert_eq!(hard_deny_refusal(HARD_DENY_PATTERNS, cmd), None);
        // Its one-line form gets the same verdict.
        let one_line = "python3 -c \"print(1); rm -rf /; print(2)\"";
        assert_eq!(label_of(one_line), None, "{one_line:?}");

        // A commit body is the same shape and now passes.
        let commit = "git commit -m \"subject\n\nrm -rf / is now guarded\"";
        assert_eq!(label_of(commit), None, "{commit:?}");
    }

    /// Near miss for the above: on ONE line the quoted string is one word,
    /// which is the module doc's own example, so it passes.
    #[test]
    fn single_line_quoted_mention_still_passes() {
        let cmd = "echo \"rm -rf /\"";
        assert!(!cmd.contains('\n'));
        assert_eq!(label_of(cmd), None, "{cmd:?}");
        assert_eq!(hard_deny_refusal(HARD_DENY_PATTERNS, cmd), None);
        // A multi-line quoted argument that never mentions the pattern
        // passes: the fail-closed reach is the mention, not the shape.
        let benign = "python3 -c \"print(1)\nprint(2)\"";
        assert_eq!(label_of(benign), None, "{benign:?}");
    }

    /// #1101: (multi-line command, the same command with each QUOTED newline
    /// written as a space by hand, expected label). The invariant is that the
    /// two forms get the same verdict; the explicit expected column stops the
    /// table passing by agreeing with itself. The single-line column is typed,
    /// not computed, so the quote tracker under test cannot produce it.
    const QUOTED_NEWLINE_ROWS: &[(&str, &str, Option<&str>)] = &[
        // --- must still refuse ---
        // An unquoted newline separates commands.
        ("echo hi\nrm -rf /", "echo hi\nrm -rf /", Some(RULE_RM_ROOT)),
        // A quote that opens and closes across a newline, then the pattern.
        (
            "echo \"a\nb\"; rm -rf /",
            "echo \"a b\"; rm -rf /",
            Some(RULE_RM_ROOT),
        ),
        (
            "echo \"a\nb\"\nrm -rf /",
            "echo \"a b\"\nrm -rf /",
            Some(RULE_RM_ROOT),
        ),
        // Backslash-newline continuation splitting the pattern (a normaliser
        // the old line split merged, so it gets its own row).
        ("rm -rf \\\n/", "rm -rf /", Some(RULE_RM_ROOT)),
        ("rm -rf \\\r\n/", "rm -rf /", Some(RULE_RM_ROOT)),
        // Heredoc fed to a shell: body lines are tokenized as commands.
        (
            "bash <<EOF\nrm -rf /\nEOF",
            "bash <<EOF\nrm -rf /\nEOF",
            Some(RULE_RM_ROOT),
        ),
        // A heredoc body is not quote-tracked: apostrophes in it must not
        // pair up across lines and hide the line between them.
        (
            "bash <<EOF\necho it's\nrm -rf /\necho don't\nEOF",
            "bash <<EOF\necho it's\nrm -rf /\necho don't\nEOF",
            Some(RULE_RM_ROOT),
        ),
        // A comment's apostrophe is not a quote either (bash runs the rm).
        (
            "# don't\nrm -rf /\n# isn't",
            "# don't\nrm -rf /\n# isn't",
            Some(RULE_RM_ROOT),
        ),
        // ANSI-C quoting: `\'` inside `$'…'` is an escaped quote, not a close.
        (
            "echo $'\\''\nrm -rf /\necho '",
            "echo $'\\''\nrm -rf /\necho '",
            Some(RULE_RM_ROOT),
        ),
        // A quote that never closes: the fail-closed desugar path.
        (
            "echo \"oops\nrm -rf /",
            "echo \"oops rm -rf /",
            Some(RULE_RM_ROOT),
        ),
        // `$(` inside double quotes executes, across lines too.
        (
            "echo \"$(\nrm -rf /\n)\"",
            "echo \"$( rm -rf / )\"",
            Some(RULE_RM_ROOT),
        ),
        // A nested shell's script string is re-matched with ITS newlines.
        (
            "bash -c \"x\nrm -rf /\"",
            "bash -c \"x rm -rf /\"",
            Some(RULE_RM_ROOT),
        ),
        (
            "sh -c 'x\nrm -rf /'",
            "sh -c 'x rm -rf /'",
            Some(RULE_RM_ROOT),
        ),
        // CRLF, refusing.
        (
            "echo hi\r\nrm -rf /",
            "echo hi\r\nrm -rf /",
            Some(RULE_RM_ROOT),
        ),
        // --- must now pass (the #1101 fix) ---
        (
            "git commit -m \"subject\n\nbody mentioning rm -rf /\"",
            "git commit -m \"subject  body mentioning rm -rf /\"",
            None,
        ),
        (
            "gh issue comment 1 --body \"line\nrm -rf / is refused\"",
            "gh issue comment 1 --body \"line rm -rf / is refused\"",
            None,
        ),
        ("echo 'a\nrm -rf /'", "echo 'a rm -rf /'", None),
        (
            "git commit -m \"x\n\nmkfs.ext4 /dev/sda is refused\"",
            "git commit -m \"x  mkfs.ext4 /dev/sda is refused\"",
            None,
        ),
        // A mention quoted again inside the argument (Day 243's refusal).
        (
            "python3 -c \"print(1)\nprint('rm -rf /')\"",
            "python3 -c \"print(1) print('rm -rf /')\"",
            None,
        ),
        // The stated cost: an inner program passes, as its one-line form does.
        (
            "python3 -c \"print(1)\nrm -rf /\nprint(2)\"",
            "python3 -c \"print(1) rm -rf / print(2)\"",
            None,
        ),
        // CRLF, passing.
        (
            "git commit -m \"subject\r\n\r\nrm -rf / is guarded\"",
            "git commit -m \"subject\r \r rm -rf / is guarded\"",
            None,
        ),
    ];

    #[test]
    fn quoted_newline_verdict_equals_its_single_line_form() {
        let mut wrong = Vec::new();
        for (multi, single, expected) in QUOTED_NEWLINE_ROWS {
            // Anti-vacuous: every multi-line row really spans lines.
            assert!(multi.contains('\n'), "fixture must span lines: {multi:?}");
            let (m, s) = (label_of(multi), label_of(single));
            if m != *expected || s != *expected {
                wrong.push(format!(
                    "{multi:?}: multi={m:?} single={s:?} expected={expected:?}"
                ));
            }
        }
        assert!(wrong.is_empty(), "rows off:\n{}", wrong.join("\n"));
    }

    /// Inverse probe (d217): each passing row's pattern-EXECUTING twin, with
    /// the quote closed before an unquoted newline, still refuses.
    #[test]
    fn passing_rows_executing_twins_refuse() {
        let twins = [
            "git commit -m \"subject\"\nrm -rf /",
            "gh issue comment 1 --body \"line\"\nrm -rf /",
            "echo 'a'\nrm -rf /",
            "git commit -m \"x\"\nmkfs.ext4 /dev/sda",
            "python3 -c \"print(1)\"\nrm -rf /",
            "git commit -m \"subject\"\r\nrm -rf /",
        ];
        for cmd in twins {
            assert!(label_of(cmd).is_some(), "should refuse: {cmd:?}");
        }
    }

    #[test]
    fn nesting_past_the_cap_still_fails_closed() {
        let mut cmd = String::from("rm -rf /");
        for _ in 0..6 {
            cmd = format!("eval {cmd:?}");
        }
        assert_eq!(label_of(&cmd), Some(RULE_RM_ROOT), "{cmd}");
    }
}
