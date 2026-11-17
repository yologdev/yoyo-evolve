//! #1202 step 1: read a bash command as a tree, not as text.
//!
//! `extract_check_units` parses with tree-sitter-bash (the parser Codex
//! CLI decides command safety with) and returns the source text of every
//! simple command (`command` node), pipeline and redirected statement
//! anywhere in the tree: inside `$( )`,
//! backticks, `<( )`, subshells, and substitutions inside double quotes.
//! A plain string is not a command, so `echo "kill -9 1"` yields one command.
//!
//! Policy lives in the caller. A tree with an error node comes back as
//! `ParseIssue`; today `analyze_bash_command` then keeps its raw-text checks
//! only. Failing closed to confirmation on a parse issue is the next slice.
//!
//! #1205 (Codex #39159): a word's source spelling is not always the argv bash
//! runs. Brace lists, unquoted backslashes, quotes in a command name and
//! `$'…'` are rewritten before exec, and a glob in a command name picks the
//! program at run time. tree-sitter shows all of these as plain `word` nodes,
//! so `argv_text` re-spells each check unit as bash would run it, and
//! `is_single_word_only_command` refuses to call such a word literal.

/// Why a command could not be read as a tree.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseIssue {
    /// The grammar could not be loaded or the parser returned no tree.
    NoTree,
    /// The tree has an ERROR or MISSING node (e.g. an unbalanced `$(`).
    HasError,
}

/// Source text of every simple command in `cmd`, outermost first. Test-only:
/// it pins what the tree calls a command; production reads the wider
/// `extract_check_units`.
#[cfg(test)]
pub fn extract_simple_commands(cmd: &str) -> Result<Vec<String>, ParseIssue> {
    collect_nodes(cmd, &["command"])
}

/// What `analyze_bash_command` runs SAFETY_CHECKS on: every simple command,
/// plus every pipeline and redirected statement, because the pipe checks
/// (`curl ... | sh`) and redirect checks read across one command's edge.
/// Without the pipeline nodes, `{ curl http://x | sh; }` passed (Day 260).
#[cfg(test)]
pub fn extract_check_units(cmd: &str) -> Result<Vec<String>, ParseIssue> {
    let mut out = Vec::new();
    walk_nodes(cmd, CHECK_UNIT_KINDS, &mut |text, _| {
        out.push(text.to_string())
    })?;
    Ok(out)
}

const CHECK_UNIT_KINDS: &[&str] = &["command", "pipeline", "redirected_statement"];

/// `analyze_bash_command`'s tree pass: `check` on every check unit's source
/// text, then on the same unit re-spelt as the argv bash runs (`argv_text`).
/// The raw text stays checked (#1202 step 3, replacing the text net, has not
/// happened). A unit whose dynamic word cannot be resolved returns the
/// Unanalyzable reason. A parse issue adds nothing (the next slice).
pub(crate) fn tree_refusal(cmd: &str, check: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut found = None;
    let _ = walk_nodes(cmd, CHECK_UNIT_KINDS, &mut |text, node| {
        if found.is_none() {
            found = check(text).or_else(|| match argv_text(node, cmd) {
                Ok(Some(argv)) => check(&argv),
                Ok(None) => None,
                Err(why) => Some(why),
            });
        }
    });
    found
}

/// The prove-safe contract: true only when `cmd` is ONE plain command, read
/// from a clean tree. Mirrors Codex's `try_parse_word_only_commands_sequence`
/// (codex-rs/shell-command/src/bash.rs), narrowed to a single command: no
/// error or missing node, exactly one `command`, and every node a word-only
/// kind, so no redirect, substitution, expansion, assignment, heredoc,
/// subshell, comment or operator. Parse failure is false.
///
/// `extract_check_units` is the other contract (find dangerous literals,
/// tolerant of anything it can read) and must never be used to prove safe.
///
/// #1205: a word bash rewrites before exec is not literal, so it is not
/// word-only either: an unquoted backslash, brace list or sequence, glob
/// character, or tilde at a word start or after `=`/`:` (`word_is_dynamic`).
/// `$'…'` is an `ansi_c_string` node, already outside `WORD_ONLY`.
pub(crate) fn is_single_word_only_command(cmd: &str) -> bool {
    word_only(cmd, true)
}

/// `literal_words: false` is the pattern side of the wildcard rule: an allow
/// pattern is full of `*` by design, so only its structure is read.
fn word_only(cmd: &str, literal_words: bool) -> bool {
    const WORD_ONLY: &[&str] = &[
        "program",
        "command",
        "command_name",
        "word",
        "string",
        "string_content",
        "raw_string",
        "concatenation",
        "number",
    ];
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
    if parser.set_language(&language).is_err() {
        return false;
    }
    let Some(tree) = parser.parse(cmd, None) else {
        return false;
    };
    let root = tree.root_node();
    if root.has_error() || root.named_child_count() != 1 {
        return false;
    }
    let mut commands = 0;
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.is_missing() || node.is_error() {
            return false;
        }
        if node.is_named() {
            if !WORD_ONLY.contains(&node.kind()) {
                return false;
            }
            commands += usize::from(node.kind() == "command");
            let is_word = node.parent().is_some_and(|p| p.kind() == "command");
            if literal_words && is_word && word_is_dynamic(node.utf8_text(cmd.as_bytes())) {
                return false;
            }
        } else if node.kind() != "\"" {
            // The only anonymous token a plain command has is a string's quote.
            return false;
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return commands == 1;
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only: switches the tree rule off on this thread so the
    /// differential in bash_parse_tests.rs can read the text-only verdict.
    pub(crate) static TREE_RULE_OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A wildcard `allow` pattern auto-approves only one plain command (#1202
/// step 2): true (falls through to the prompt) when the pattern has a
/// wildcard, is itself one plain command, and `command` is not. A pattern
/// that spells out a chain, redirect or substitution (`cargo * 2>&1`) is the
/// user naming that shape, the same hatch the text predicate gives; it is
/// left to the text predicates, so nothing approves that did not before.
pub(crate) fn allow_wildcard_not_one_plain_command(pattern: &str, command: &str) -> bool {
    #[cfg(test)]
    if TREE_RULE_OFF.with(|c| c.get()) {
        return false;
    }
    (pattern.contains('*') || pattern.contains('?'))
        && word_only(pattern, false)
        && !is_single_word_only_command(command)
}

/// #1202 step 1's bolded clause: why yoyo cannot read `cmd` well enough for
/// a session "always" or an allow rule to approve it unasked, or `None`.
/// Three shapes: a parse error (ERROR/MISSING node, e.g. `{echo,hi}`,
/// `l[s]`, `ls &&`); `eval` in command position, also behind a wrapper
/// (`command eval …`); and a word bash resolves at run time, from the two
/// existing producers: `argv_text`'s Unanalyzable (glob or `$'…'` program
/// name, an unexpandable brace) and `check_indirect_command_word` (`$cmd`).
pub(crate) fn unanalyzable_reason(cmd: &str) -> Option<String> {
    let mut found = None;
    let walked = walk_nodes(cmd, &["command"], &mut |_, node| {
        if found.is_none() {
            found = eval_in_command_position(node, cmd).or_else(|| argv_text(node, cmd).err());
        }
    });
    if walked.is_err() {
        return Some(
            "Unanalyzable command: bash's grammar cannot parse it, so the safety \
             check cannot see what runs"
                .to_string(),
        );
    }
    found.or_else(|| crate::safety::check_indirect_command_word(cmd, ""))
}

/// `Some(reason)` when this simple command runs `eval`: its name, or the first
/// word after a wrapper (`sudo`, `command`, `builtin`, `env X=1`, …) and its
/// `-flags`. Quotes are removed first, so `e'v'al` is `eval`.
fn eval_in_command_position(node: tree_sitter::Node, src: &str) -> Option<String> {
    let mut cursor = node.walk();
    let mut seen_name = false;
    for word in node.named_children(&mut cursor) {
        if !seen_name && word.kind() != "command_name" {
            continue;
        }
        seen_name = true;
        let text = word.utf8_text(src.as_bytes()).ok()?;
        let lexed: String = lex_word(text)?.chars.iter().map(|ch| ch.c).collect();
        let base = lexed.rsplit('/').next().unwrap_or("");
        if base == "eval" {
            return Some(
                "Unanalyzable command: `eval` runs a string as a new command, so the \
                 safety check cannot see what runs"
                    .to_string(),
            );
        }
        let skip = crate::safety::RM_COMMAND_WRAPPERS.contains(&base)
            || (word.kind() != "command_name" && (lexed.starts_with('-') || lexed.contains('=')));
        if !skip {
            return None;
        }
    }
    None
}

/// What the bash confirm callback does before it would prompt (#1202 step 1).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PrePrompt {
    /// Run without asking.
    Approve,
    /// Refuse without asking (an allow-closure deny verdict).
    Refuse,
    /// Show the prompt.
    Ask,
}

/// `always`: the session "always" flag. `allow`: the allow list's verdict on
/// the command. `unanalyzable`: [`unanalyzable_reason`] is `Some`. A command
/// yoyo cannot read is never auto-approved; a deny verdict still wins.
pub(crate) fn pre_prompt_route(always: bool, allow: Option<bool>, unanalyzable: bool) -> PrePrompt {
    match (allow, always) {
        (Some(false), _) => PrePrompt::Refuse,
        _ if unanalyzable => PrePrompt::Ask,
        (_, true) | (Some(true), _) => PrePrompt::Approve,
        (None, false) => PrePrompt::Ask,
    }
}

#[cfg(test)]
fn collect_nodes(cmd: &str, kinds: &[&str]) -> Result<Vec<String>, ParseIssue> {
    let mut out = Vec::new();
    walk_nodes(cmd, kinds, &mut |text, _| out.push(text.to_string()))?;
    Ok(out)
}

/// Pre-order walk over every node of a clean tree, calling `f` with the
/// source text of each node whose kind is in `kinds`, so nested commands
/// come after the command that contains them.
fn walk_nodes(
    cmd: &str,
    kinds: &[&str],
    f: &mut dyn FnMut(&str, tree_sitter::Node),
) -> Result<(), ParseIssue> {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|_| ParseIssue::NoTree)?;
    let tree = parser.parse(cmd, None).ok_or(ParseIssue::NoTree)?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(ParseIssue::HasError);
    }
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if kinds.contains(&node.kind()) {
            if let Ok(text) = node.utf8_text(cmd.as_bytes()) {
                f(text, node);
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(());
            }
        }
    }
}

// ---- #1205: what bash does to a word before exec ----------------------------

/// How one character of a word reaches bash's argv.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Q {
    /// Unquoted: braces, globs and tildes act on it.
    Bare,
    /// Unquoted but backslash-escaped: literal, and the backslash is removed.
    Escaped,
    /// Inside quotes: literal.
    Quoted,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Ch {
    c: char,
    q: Q,
}

/// A word after quote removal. `ansi_escape` is a `$'…'` with a backslash in
/// it, which is NOT decoded (its chars are the raw escape text).
struct Lexed {
    chars: Vec<Ch>,
    ansi_escape: bool,
    ansi: bool,
}

/// Quote removal over one word's source. `None` for what this reader does
/// not re-spell: a substitution, a `${…}` or a `$x` inside double quotes, or
/// an unterminated quote. Such a word keeps its source text (still checked).
fn lex_word(src: &str) -> Option<Lexed> {
    let mut out = Lexed {
        chars: Vec::new(),
        ansi_escape: false,
        ansi: false,
    };
    let mut it = src.chars().peekable();
    let push = |out: &mut Lexed, c, q| out.chars.push(Ch { c, q });
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next() {
                Some('\n') => {}
                Some(x) => push(&mut out, x, Q::Escaped),
                None => push(&mut out, '\\', Q::Escaped),
            },
            '\'' => loop {
                match it.next()? {
                    '\'' => break,
                    x => push(&mut out, x, Q::Quoted),
                }
            },
            '"' => loop {
                match it.next()? {
                    '"' => break,
                    '`' => return None,
                    '$' if it.peek().is_some_and(|n| {
                        *n == '(' || *n == '{' || *n == '_' || n.is_ascii_alphanumeric()
                    }) =>
                    {
                        return None
                    }
                    '\\' => match it.next()? {
                        '\n' => {}
                        x @ ('$' | '`' | '"' | '\\') => push(&mut out, x, Q::Quoted),
                        x => {
                            push(&mut out, '\\', Q::Quoted);
                            push(&mut out, x, Q::Quoted);
                        }
                    },
                    x => push(&mut out, x, Q::Quoted),
                }
            },
            '`' => return None,
            '$' if it.peek() == Some(&'\'') => {
                it.next();
                out.ansi = true;
                loop {
                    match it.next()? {
                        '\'' => break,
                        '\\' => {
                            out.ansi_escape = true;
                            push(&mut out, '\\', Q::Quoted);
                            push(&mut out, it.next()?, Q::Quoted);
                        }
                        x => push(&mut out, x, Q::Quoted),
                    }
                }
            }
            '$' if matches!(it.peek(), Some('(' | '{')) => return None,
            x => push(&mut out, x, Q::Bare),
        }
    }
    Some(out)
}

/// At most this many words from one word's brace expansion; more is
/// Unanalyzable rather than a long check.
const MAX_BRACE_WORDS: usize = 16;

/// Bash's brace expansion over a lexed word: comma lists with prefix and
/// suffix, products of several lists, and single-letter sequences (`{c..c}`
/// spells a letter). Digit sequences (`{1..3}`) stay unexpanded: digits
/// cannot spell a command word or a flag name. A list with no comma (`{}`,
/// `@{u}`) is literal, as in bash. Nested lists and more than
/// `MAX_BRACE_WORDS` words are `Err`.
fn expand_braces(w: &[Ch]) -> Result<Vec<Vec<Ch>>, &'static str> {
    let bare = |i: usize, c: char| w[i].q == Q::Bare && w[i].c == c;
    let mut i = 0;
    while i < w.len() {
        if !bare(i, '{') {
            i += 1;
            continue;
        }
        let Some(j) = (i + 1..w.len()).find(|&j| bare(j, '{') || bare(j, '}')) else {
            break;
        };
        if bare(j, '{') {
            if (j..w.len()).any(|k| bare(k, ',')) {
                return Err("a nested brace list");
            }
            i = j;
            continue;
        }
        let body = &w[i + 1..j];
        let mut alts: Vec<Vec<Ch>> = body
            .split(|ch| ch.q == Q::Bare && ch.c == ',')
            .map(<[Ch]>::to_vec)
            .collect();
        if alts.len() == 1 {
            match letter_sequence(body) {
                Some(seq) => alts = seq,
                None => {
                    i = j + 1;
                    continue;
                }
            }
        }
        let mut out = Vec::new();
        for alt in alts {
            let word: Vec<Ch> = [&w[..i], &alt[..], &w[j + 1..]].concat();
            out.extend(expand_braces(&word)?);
            if out.len() > MAX_BRACE_WORDS {
                return Err("a brace expansion of more than 16 words");
            }
        }
        out.retain(|w| !w.is_empty());
        return Ok(out);
    }
    Ok(vec![w.to_vec()])
}

/// `{a..e}` as its letters, or `None` (digits, steps, anything else).
fn letter_sequence(body: &[Ch]) -> Option<Vec<Vec<Ch>>> {
    let [a, d1, d2, b] = body else {
        return None;
    };
    let (a, b) = (a.c, b.c);
    if d1.c != '.' || d2.c != '.' || !a.is_ascii_alphabetic() || !b.is_ascii_alphabetic() {
        return None;
    }
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    Some((lo..=hi).map(|c| vec![Ch { c, q: Q::Bare }]).collect())
}

fn is_glob(w: &[Ch]) -> bool {
    let bare = |ch: &Ch, c: char| ch.q == Q::Bare && ch.c == c;
    w.iter().any(|ch| bare(ch, '*') || bare(ch, '?'))
        || w.iter()
            .position(|ch| bare(ch, '['))
            .is_some_and(|i| w[i..].iter().any(|ch| bare(ch, ']')))
}

/// The prove-safe side (`is_single_word_only_command`): does bash rewrite
/// this word before exec? Errs toward "yes" (a prompt) on anything it cannot
/// lex.
fn word_is_dynamic(src: Result<&str, std::str::Utf8Error>) -> bool {
    let Some(lexed) = src.ok().and_then(lex_word) else {
        return true;
    };
    let w = &lexed.chars;
    let tilde = w.iter().enumerate().any(|(i, ch)| {
        ch.q == Q::Bare
            && ch.c == '~'
            && (i == 0 || (w[i - 1].q == Q::Bare && matches!(w[i - 1].c, '=' | ':')))
    });
    lexed.ansi
        || tilde
        || is_glob(w)
        || w.iter().any(|ch| ch.q == Q::Escaped)
        || !matches!(expand_braces(w).as_deref(), Ok([one]) if one == w)
}

/// One word rendered as text for the safety checks. A command name that is
/// all plain characters is written bare (so `"cu"rl` reads as `curl`); any
/// quoted run elsewhere stays single-quoted, so a quoted mention in an
/// argument is still a mention.
fn render(w: &[Ch], command_name: bool) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "-_./+@%:=,~^".contains(c);
    if command_name && w.iter().all(|ch| plain(ch.c)) {
        return w.iter().map(|ch| ch.c).collect();
    }
    let mut out = String::new();
    let mut open = false;
    for ch in w {
        let quote = ch.q == Q::Quoted || (ch.q == Q::Escaped && !plain(ch.c));
        if quote != open {
            out.push('\'');
            open = quote;
        }
        if ch.c == '\'' && open {
            out.push_str("'\\''");
        } else {
            out.push(ch.c);
        }
    }
    if open {
        out.push('\'');
    }
    out
}

/// One word of a command as bash will pass it: `None` when the spelling is
/// already the argv, `Some(words)` when bash rewrites it.
fn respell_word(src: &str, command_name: bool) -> Result<Option<Vec<String>>, String> {
    let unanalyzable = |what: &str| {
        let shown: String = src.chars().take(60).collect();
        format!(
            "Unanalyzable command: `{shown}` is {what}, which bash expands at run time, \
             so the safety check cannot see what runs"
        )
    };
    let Some(mut lexed) = lex_word(src) else {
        return Ok(None);
    };
    // An argument that STARTS with an escape (`--grep \rm`) stays a mention,
    // as the dequoting pass treats a quoted argument: spelt bare, it would
    // read as a program to the boundary-anchored checks. Inside a word
    // (`-\delete`) the escape is removed.
    if !command_name && lexed.chars.first().is_some_and(|ch| ch.q == Q::Escaped) {
        for ch in lexed.chars.iter_mut().filter(|ch| ch.q == Q::Escaped) {
            ch.q = Q::Quoted;
        }
    }
    let words = expand_braces(&lexed.chars).map_err(unanalyzable)?;
    let expanded = !matches!(words.as_slice(), [one] if *one == lexed.chars);
    if command_name {
        if lexed.ansi_escape {
            return Err(unanalyzable("a `$'…'` program name"));
        }
        if words.first().is_some_and(|w| is_glob(w)) {
            return Err(unanalyzable("a glob in the program name"));
        }
    }
    let respelt = expanded
        || lexed.chars.iter().any(|ch| ch.q == Q::Escaped)
        || (lexed.ansi && !lexed.ansi_escape)
        || (command_name && lexed.chars.iter().any(|ch| ch.q == Q::Quoted));
    if !respelt {
        return Ok(None);
    }
    Ok(Some(
        words
            .iter()
            .enumerate()
            .map(|(i, w)| render(w, command_name && i == 0))
            .collect(),
    ))
}

/// The argument node kinds `argv_text` re-spells. Anything else (redirects,
/// substitutions, assignments) keeps its source text.
const ARGV_WORD_KINDS: &[&str] = &[
    "command_name",
    "word",
    "concatenation",
    "string",
    "raw_string",
    "ansi_c_string",
];

/// A check unit's text with every word re-spelt as the argv bash runs, or
/// `None` when no word changes. `Err` is the Unanalyzable reason.
fn argv_text(unit: tree_sitter::Node, src: &str) -> Result<Option<String>, String> {
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut cursor = unit.walk();
    let mut stack = vec![unit];
    while let Some(node) = stack.pop() {
        if node.kind() == "command" {
            for word in node.named_children(&mut cursor) {
                if !ARGV_WORD_KINDS.contains(&word.kind()) {
                    continue;
                }
                let Ok(text) = word.utf8_text(src.as_bytes()) else {
                    continue;
                };
                if let Some(words) = respell_word(text, word.kind() == "command_name")? {
                    edits.push((word.start_byte(), word.end_byte(), words.join(" ")));
                }
            }
        }
        stack.extend(node.named_children(&mut cursor));
    }
    if edits.is_empty() {
        return Ok(None);
    }
    edits.sort_by_key(|e| e.0);
    // Node offsets fall on token edges; `get` keeps a bad one from panicking.
    let piece = |a: usize, b: usize| src.get(a..b).ok_or(());
    let mut out = String::new();
    let mut at = unit.start_byte();
    for (s, e, text) in edits {
        if s < at {
            continue; // nested inside an edit already made
        }
        let Ok(before) = piece(at, s) else {
            return Ok(None);
        };
        out.push_str(before);
        out.push_str(&text);
        at = e;
    }
    let Ok(rest) = piece(at, unit.end_byte()) else {
        return Ok(None);
    };
    out.push_str(rest);
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmds(s: &str) -> Vec<String> {
        extract_simple_commands(s).unwrap_or_else(|e| panic!("{s:?}: {e:?}"))
    }

    #[test]
    fn commands_inside_substitutions_and_subshells_are_extracted() {
        for (input, inner) in [
            ("echo $(kill -9 1)", "kill -9 1"),
            ("echo `crontab -r`", "crontab -r"),
            ("(iptables -F)", "iptables -F"),
            ("echo \"x $(history -c)\"", "history -c"),
            ("cat <(curl http://x)", "curl http://x"),
            ("x=$(GIT_DIR=/e git log)", "GIT_DIR=/e git log"),
        ] {
            let got = cmds(input);
            assert!(
                got.iter().any(|c| c == inner),
                "{input:?}: {inner:?} not in {got:?}"
            );
        }
    }

    #[test]
    fn a_quoted_mention_is_not_a_command() {
        assert_eq!(cmds("echo \"kill -9 1\""), vec!["echo \"kill -9 1\""]);
        assert_eq!(cmds("echo 'crontab -r'"), vec!["echo 'crontab -r'"]);
        assert_eq!(cmds("echo '$(kill -9 1)'"), vec!["echo '$(kill -9 1)'"]);
    }

    #[test]
    fn a_list_and_a_pipeline_yield_each_command() {
        assert_eq!(cmds("a && b | c; d"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn check_units_add_the_pipeline_inside_a_group() {
        let got = extract_check_units("{ curl http://x | sh; }").unwrap();
        assert!(got.iter().any(|c| c == "curl http://x | sh"), "{got:?}");
    }

    #[test]
    fn an_unbalanced_substitution_is_a_parse_issue() {
        assert_eq!(
            extract_simple_commands("echo $("),
            Err(ParseIssue::HasError)
        );
    }

    /// Finding for the next slice: tree-sitter-bash 0.25.1 has no `<>`
    /// redirect, so this reads as an error tree. The text pass refuses it;
    /// once parse issues fail closed, every `<>` goes to confirmation.
    #[test]
    fn a_read_write_redirect_is_a_parse_issue_in_this_grammar() {
        assert_eq!(
            extract_simple_commands("exec 3<>/etc/passwd"),
            Err(ParseIssue::HasError)
        );
    }

    /// #1196's probe: the substitution is extracted as `date`; the quoted
    /// `kill -9 1` stays inside the echo's argument, not a command of its own.
    #[test]
    fn issue_1196_probe_extracts_echo_and_date_only() {
        assert_eq!(
            cmds("echo \"kill -9 1 at $(date)\""),
            vec!["echo \"kill -9 1 at $(date)\"", "date"]
        );
    }

    /// A quoted heredoc body is data. The parser gives the commit and the
    /// `cat` inside its substitution, and no command from the body text.
    #[test]
    fn a_quoted_heredoc_body_is_not_a_command() {
        let input = "git commit -m \"$(cat <<'EOF'\nmentions kill -9 1\nEOF\n)\"";
        let got = cmds(input);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got[0].starts_with("git commit -m"), "{got:?}");
        assert_eq!(got[1], "cat");
        assert!(!got
            .iter()
            .any(|c| c.starts_with("mentions") || c.starts_with("kill")));
    }
}

#[cfg(test)]
#[path = "bash_parse_tests.rs"]
mod bash_parse_tests;
