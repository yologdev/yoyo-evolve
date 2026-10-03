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
//! The command is split into lines (a `\`-newline continuation is joined
//! first), each line into shell words with quote removal (`'…'`, `"…"`,
//! backslash escapes), and the words into segments on `;`, `&`, `|`, `(`,
//! `)`. A quoted string is therefore one WORD: `echo "rm -rf /"` has an
//! argument whose text is `rm -rf /`, and no word in it equals `rm`.
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
//!   refused. That fails closed, which is the safe direction.
//! - Command substitution `$(…)` and backticks are not parsed. A command
//!   containing them, or a line with an unbalanced quote, is ALSO scanned a
//!   second time with quote characters removed and `$(`, `)` and backticks
//!   turned into separators — so `git commit -m "… $(x) rm -rf /"` is
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
pub(crate) fn hard_deny_refusal<S: AsRef<str>>(patterns: &[S], command: &str) -> Option<String> {
    let label = hard_deny_match(command)?;
    let pattern = patterns.iter().find(|p| p.as_ref() == label)?;
    Some(format!(
        "Command blocked by safety policy: contains '{}'. This pattern is denied for safety.",
        pattern.as_ref()
    ))
}

/// Returns the label of the hard-deny rule `command` matches, if any.
pub(crate) fn hard_deny_match(command: &str) -> Option<&'static str> {
    match_at_depth(command, 0)
}

fn match_at_depth(command: &str, depth: usize) -> Option<&'static str> {
    let squeezed: String = command.chars().filter(|c| !c.is_whitespace()).collect();
    if squeezed.contains(RULE_FORK_BOMB) {
        return Some(RULE_FORK_BOMB);
    }
    if depth > MAX_NESTING {
        // Fail closed: past the nesting cap, fall back to the old scan.
        let spaced = command.split_whitespace().collect::<Vec<_>>().join(" ");
        return [RULE_RM_ROOT, RULE_MKFS, RULE_DD]
            .into_iter()
            .find(|p| spaced.contains(p));
    }
    let joined = command.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut needs_desugar = joined.contains("$(") || joined.contains('`');
    for line in joined.lines() {
        let (segments, balanced) = split_line(line);
        needs_desugar |= !balanced;
        for seg in &segments {
            if let Some(label) = match_segment(seg, depth) {
                return Some(label);
            }
        }
    }
    if needs_desugar {
        let desugared: String = joined
            .chars()
            .filter(|c| *c != '\'' && *c != '"')
            .map(|c| if c == '`' { ';' } else { c })
            .collect::<String>()
            .replace("$(", ";")
            .replace(')', ";");
        for line in desugared.lines() {
            for seg in &split_line(line).0 {
                if let Some(label) = match_segment(seg, depth) {
                    return Some(label);
                }
            }
        }
    }
    None
}

/// Splits one line into segments of shell words (quote removal applied).
/// The bool is false when the line ends inside an open quote.
fn split_line(line: &str) -> (Vec<Vec<String>>, bool) {
    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.chars().peekable();
    let mut balanced = true;
    let flush_word = |words: &mut Vec<String>, word: &mut String, in_word: &mut bool| {
        if *in_word {
            words.push(std::mem::take(word));
            *in_word = false;
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                let mut closed = false;
                for q in chars.by_ref() {
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
                while let Some(q) = chars.next() {
                    match q {
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\\' if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.push(chars.next().unwrap_or('\\'));
                        }
                        _ => word.push(q),
                    }
                }
                balanced &= closed;
            }
            '\\' => {
                in_word = true;
                if let Some(n) = chars.next() {
                    word.push(n);
                }
            }
            ';' | '&' | '|' | '(' | ')' => {
                flush_word(&mut words, &mut word, &mut in_word);
                if !words.is_empty() {
                    segments.push(std::mem::take(&mut words));
                }
            }
            c if c.is_whitespace() => flush_word(&mut words, &mut word, &mut in_word),
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    flush_word(&mut words, &mut word, &mut in_word);
    if !words.is_empty() {
        segments.push(words);
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

fn match_segment(seg: &[String], depth: usize) -> Option<&'static str> {
    let cmd_at = command_index(seg);
    for (i, w) in seg.iter().enumerate() {
        let b = basename(w);
        let rest = &seg[i + 1..];
        if b == "rm" {
            if let Some(label) = rm_root(rest) {
                return Some(label);
            }
        }
        if b == "dd" && rest.iter().any(|a| dd_writes_device(a)) {
            return Some(RULE_DD);
        }
        if (b == "mkfs" || b.starts_with("mkfs."))
            && (cmd_at == Some(i) || rest.iter().any(|a| a.starts_with("/dev/")))
        {
            return Some(RULE_MKFS);
        }
        if SHELLS.contains(&b) {
            let script = rest
                .iter()
                .position(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('c'))
                .and_then(|f| rest.get(f + 1));
            if let Some(label) = script.and_then(|s| match_at_depth(s, depth + 1)) {
                return Some(label);
            }
        }
        if b == "eval" {
            if let Some(label) = match_at_depth(&rest.join(" "), depth + 1) {
                return Some(label);
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
            assert_eq!(hard_deny_match(cmd), Some(*label), "should refuse: {cmd:?}");
        }
    }

    #[test]
    fn near_miss_rows_pass() {
        for cmd in MUST_PASS {
            assert_eq!(hard_deny_match(cmd), None, "should pass: {cmd:?}");
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
            assert_eq!(hard_deny_match(cmd), Some(label), "{cmd}");
        }
    }

    #[test]
    fn refusal_text_is_byte_identical_to_the_old_format() {
        assert_eq!(
            hard_deny_refusal(HARD_DENY_PATTERNS, "rm -fr /").as_deref(),
            Some(
                "Command blocked by safety policy: contains 'rm -rf /'. \
                 This pattern is denied for safety."
            )
        );
        assert_eq!(hard_deny_refusal(HARD_DENY_PATTERNS, "rm -rf /tmp/x"), None);
    }

    #[test]
    fn disabled_rule_is_not_refused() {
        // `deny_patterns` selects the active rules: a list without the
        // mkfs label does not refuse mkfs, and still refuses rm-root.
        let only_rm = [RULE_RM_ROOT];
        assert_eq!(hard_deny_refusal(&only_rm, "mkfs /dev/sda"), None);
        assert!(hard_deny_refusal(&only_rm, "rm -rf /").is_some());
    }

    #[test]
    fn nesting_past_the_cap_still_fails_closed() {
        let mut cmd = String::from("rm -rf /");
        for _ in 0..6 {
            cmd = format!("eval {cmd:?}");
        }
        assert_eq!(hard_deny_match(&cmd), Some(RULE_RM_ROOT), "{cmd}");
    }
}
