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
            // #1196: a print command's quoted words are a mention, so its text
            // is read quote-stripped and its argv re-spelling is not checked;
            // the commands inside its `$(…)`/backticks are nodes walked next.
            let print = is_print_command(node, cmd);
            // #1210: a git commit / gh message value is a mention the same way.
            let said = message_value_ranges(node, cmd);
            let text = match print {
                true => crate::safety::strip_quoted_regions(text),
                false => blank_mentions(cmd, node.start_byte(), node.end_byte(), &said),
            };
            found = check(&text)
                .or_else(|| match argv_text(node, cmd, &said) {
                    Ok(Some(argv)) if !print => check(&argv),
                    Ok(_) => None,
                    Err(why) => Some(why),
                })
                // #1209: a literal `sh -c` body gets the whole analysis, as if
                // it were typed bare. Past the depth cap, unanalyzable_reason asks.
                .or_else(|| {
                    let body = dash_c_body(node, cmd)?;
                    nested(|| crate::safety::analyze_bash_command(&body)).flatten()
                });
        }
    });
    found
}

/// Commands whose words are printed and never run (#1196). Exact names only:
/// a path, a wrapper (`sudo echo`) or any other command is not one of these,
/// so its quoted text stays command text (fail closed).
const PRINT_ONLY_COMMANDS: &[&str] = &["echo", "printf"];

/// `node` is a simple `echo`/`printf` command, not inside a pipeline (whose
/// next stage may run the text, `echo "…" | sh`) or a redirect.
fn is_print_command(node: tree_sitter::Node, src: &str) -> bool {
    node.kind() == "command"
        && node.parent().is_none_or(|p| p.kind() == "program")
        && node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(src.as_bytes()).ok())
            .is_some_and(|name| PRINT_ONLY_COMMANDS.contains(&name))
}

/// What `analyze_bash_command`'s raw-text pass reads. A lone `echo`/`printf`
/// (#1196) is read quote-stripped (`stripped`); otherwise `cmd` with the
/// quoted text of each git commit / gh message value blanked (#1210), whose
/// `$(…)`/backtick bodies stay. A parse issue leaves `cmd` as it is.
pub(crate) fn raw_pass_text(cmd: &str, stripped: &str) -> String {
    if lone_print_command(cmd) {
        return stripped.to_string();
    }
    let mut said = Vec::new();
    let _ = walk_nodes(cmd, &["command"], &mut |_, node| {
        said.extend(message_value_ranges(node, cmd))
    });
    blank_mentions(cmd, 0, cmd.len(), &said)
}

/// `src[from..to]` with each range in `said` (sorted, inside it) read
/// quote-stripped: quoted literal text becomes blanks, substitutions stay.
fn blank_mentions(src: &str, from: usize, to: usize, said: &[(usize, usize)]) -> String {
    let mut out = String::new();
    let mut at = from;
    for &(s, e) in said.iter().filter(|r| r.0 >= from && r.1 <= to) {
        // Node offsets fall on token edges; `get` keeps a bad one from panicking.
        let (Some(before), Some(value)) = (src.get(at..s), src.get(s..e)) else {
            return src.get(from..to).unwrap_or_default().to_string();
        };
        out.push_str(before);
        out.push_str(&crate::safety::strip_quoted_regions(value));
        at = e;
    }
    out.push_str(src.get(at..to).unwrap_or_default());
    out
}

/// gh's builtin commands whose `--body`/`-b`/`--title`/`-t` value is text
/// posted to GitHub (#1210). An extension or alias (`gh my-ext -b …`) can run
/// its arguments, so only these exact names count.
const GH_MESSAGE_COMMANDS: &[&str] = &["issue", "pr", "release"];

/// #1210: byte ranges (in `src`) of the message values of `node` when it is
/// `git … commit` (`-m`, `--message`, `--message=`, glued `-m…`, `-am …`) or
/// `gh issue|pr|release …` (`--body`, `-b`, `--title`, `-t`, `=`/glued). Git
/// stores that text and gh posts it; neither runs it. Exact program names
/// only, and only a statement on its own or in a `&&`/`||`/`;` list: in a
/// pipeline the next stage may run git's output (fail closed).
fn message_value_ranges(node: tree_sitter::Node, src: &str) -> Vec<(usize, usize)> {
    let mut said = Vec::new();
    if node.kind() != "command"
        || !node
            .parent()
            .is_none_or(|p| matches!(p.kind(), "program" | "list"))
    {
        return said;
    }
    let mut cursor = node.walk();
    let words: Vec<tree_sitter::Node> = node
        .named_children(&mut cursor)
        .skip_while(|w| w.kind() != "command_name")
        .filter(|w| !w.kind().ends_with("redirect"))
        .collect();
    let text = |w: &tree_sitter::Node| w.utf8_text(src.as_bytes()).unwrap_or_default();
    let Some((name, args)) = words.split_first() else {
        return said;
    };
    // (long flags, short flag letters, short letters that may come before it
    // in a cluster, short flags that take the next word as their value).
    let (rest, long, short, cluster, valued): (&[_], &[&str], &str, &str, &str) = match text(name) {
        "git" => {
            // Global options, as `safety::git_write_subcommand` steps them.
            let mut i = 0;
            while args.get(i).is_some_and(|w| text(w).starts_with('-')) {
                i +=
                    1 + usize::from(crate::safety::GIT_GLOBAL_VALUE_OPTS.contains(&text(&args[i])));
            }
            match args.get(i) {
                Some(w) if text(w) == "commit" => {
                    (&args[i + 1..], &["--message"], "m", "anpqsvz", "CFct")
                }
                _ => return said,
            }
        }
        "gh" if args
            .first()
            .is_some_and(|w| GH_MESSAGE_COMMANDS.contains(&text(w))) =>
        {
            (&args[1..], &["--body", "--title"], "bt", "", "")
        }
        _ => return said,
    };
    let mut i = 0;
    while let Some(w) = rest.get(i) {
        let (t, start) = (text(w), w.start_byte());
        i += 1;
        if t == "--" {
            break;
        }
        if long.contains(&t) {
            said.extend(rest.get(i).map(|v| (v.start_byte(), v.end_byte())));
            i += 1;
        } else if let Some(eq) = long.iter().find(|l| t.starts_with(&format!("{l}="))) {
            said.push((start + eq.len() + 1, w.end_byte()));
        } else if t.starts_with('-') && !t.starts_with("--") {
            // A short cluster: flag letters, then a message letter whose value
            // is the rest of the word or the next word.
            for (at, c) in t.char_indices().skip(1) {
                let after = at + c.len_utf8();
                if short.contains(c) {
                    match after == t.len() {
                        true => {
                            said.extend(rest.get(i).map(|v| (v.start_byte(), v.end_byte())));
                            i += 1;
                        }
                        false => said.push((start + after, w.end_byte())),
                    }
                    break;
                }
                if !cluster.contains(c) {
                    i += usize::from(valued.contains(c) && after == t.len());
                    break;
                }
            }
        }
    }
    said
}

/// The whole of `cmd` is one `echo`/`printf` command (#1196): its quoted
/// literal text is a mention, so the raw-text pass reads it quote-stripped.
pub(crate) fn lone_print_command(cmd: &str) -> bool {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
    let Some(tree) = parser
        .set_language(&language)
        .ok()
        .and_then(|_| parser.parse(cmd, None))
    else {
        return false;
    };
    let root = tree.root_node();
    !root.has_error()
        && root.named_child_count() == 1
        && root
            .named_child(0)
            .is_some_and(|n| is_print_command(n, cmd))
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
    matches!(word_only_shape(cmd, literal_words, false), Ok(1))
}

/// The walk behind [`word_only`] and [`tree_allow`]: `Ok(n)` with the number
/// of `command` nodes when every node is word-only, else why not, in user
/// terms. `sequence: false` is one plain command (a single top-level node,
/// no operator); `sequence: true` is Codex's whole contract
/// (`try_parse_word_only_commands_sequence`): also `list` and `pipeline`
/// nodes and the `&&` `||` `;` `|` operators, plus whitespace tokens (a
/// newline between commands).
fn word_only_shape(cmd: &str, literal_words: bool, sequence: bool) -> Result<usize, String> {
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
    const SEQUENCE_KINDS: &[&str] = &["list", "pipeline"];
    const SEQUENCE_TOKENS: &[&str] = &["&&", "||", ";", "|", "'"];
    let unreadable = || "yoyo cannot parse it".to_string();
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
    parser.set_language(&language).map_err(|_| unreadable())?;
    let tree = parser.parse(cmd, None).ok_or_else(unreadable)?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(unreadable());
    }
    if !sequence && root.named_child_count() != 1 {
        return Err("it is more than one command".to_string());
    }
    let mut commands = 0;
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.is_missing() || node.is_error() {
            return Err(unreadable());
        }
        let kind = node.kind();
        if node.is_named() {
            if !WORD_ONLY.contains(&kind) && !(sequence && SEQUENCE_KINDS.contains(&kind)) {
                return Err(shape_reason(kind));
            }
            commands += usize::from(kind == "command");
            let is_word = node.parent().is_some_and(|p| p.kind() == "command");
            if literal_words && is_word && word_is_dynamic(node.utf8_text(cmd.as_bytes())) {
                return Err("it has a word bash rewrites before running it (a glob, \
                            brace list, backslash or `~`)"
                    .to_string());
            }
        } else if kind != "\""
            && !(sequence && (SEQUENCE_TOKENS.contains(&kind) || kind.trim().is_empty()))
        {
            // The only anonymous token a plain command has is a string's quote.
            return Err(shape_reason(kind));
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(commands);
            }
        }
    }
}

/// Why a node kind outside the word-only set sends a command to the prompt.
fn shape_reason(kind: &str) -> String {
    let what = match kind {
        "file_redirect"
        | "heredoc_redirect"
        | "herestring_redirect"
        | "redirected_statement"
        | ">"
        | ">>"
        | "<"
        | "&>"
        | "&>>"
        | ">&"
        | "<&"
        | "<<"
        | "<<-"
        | "<<<"
        | ">|" => "a redirection",
        "command_substitution" | "process_substitution" | "$(" | "`" | "<(" | ">(" => {
            "a command substitution"
        }
        "subshell" | "compound_statement" | "(" | ")" | "{" | "}" => "a subshell or group",
        "for_statement"
        | "c_style_for_statement"
        | "while_statement"
        | "if_statement"
        | "case_statement"
        | "function_definition"
        | "test_command"
        | "negated_command" => "control flow",
        "variable_assignment" | "variable_assignments" => "a variable assignment",
        "simple_expansion" | "expansion" | "arithmetic_expansion" | "$" => "a variable expansion",
        "&" => "a background `&`",
        "comment" => "a comment",
        "ansi_c_string" | "translated_string" => "a `$'…'` string",
        _ => return format!("it contains `{kind}`, which yoyo does not treat as plain words"),
    };
    format!("it contains {what}")
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
            found = eval_in_command_position(node, cmd)
                .or_else(|| argv_text(node, cmd, &[]).err())
                .or_else(|| {
                    let body = dash_c_body(node, cmd)?;
                    nested(|| unanalyzable_reason(&body)).unwrap_or_else(|| {
                        Some(format!(
                            "Unanalyzable command: `sh -c` nested more than {MAX_DASH_C_DEPTH} \
                             deep, so the safety check cannot see what runs"
                        ))
                    })
                });
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

// ---- #1209: the body of `bash -c "…"` is a command -------------------------

/// Shells whose `-c` operand is a command string they parse and run.
const DASH_C_SHELLS: &[&str] = &["bash", "sh", "zsh", "dash", "ksh"];

/// How many `sh -c` levels are read; a deeper one is Unanalyzable.
const MAX_DASH_C_DEPTH: usize = 8;

thread_local! {
    static DASH_C_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `f` one `sh -c` level deeper, or `None` past [`MAX_DASH_C_DEPTH`].
fn nested<T>(f: impl FnOnce() -> T) -> Option<T> {
    DASH_C_DEPTH.with(|d| {
        let n = d.get();
        if n >= MAX_DASH_C_DEPTH {
            return None;
        }
        d.set(n + 1);
        let out = f();
        d.set(n);
        Some(out)
    })
}

/// The command string of a `bash -c BODY` simple command (after wrappers,
/// the same skip as [`eval_in_command_position`], plus `xargs`), quotes
/// removed, when BODY is literal. `None` for any other command, and for a
/// body bash expands at run time (`"$CMD"`, `"… $x"`): that is not statically
/// known, so it keeps the routing it had before #1209.
fn dash_c_body(node: tree_sitter::Node, src: &str) -> Option<String> {
    if node.kind() != "command" {
        return None;
    }
    let mut cursor = node.walk();
    let mut words = node
        .named_children(&mut cursor)
        .skip_while(|w| w.kind() != "command_name")
        .filter(|w| ARGV_WORD_KINDS.contains(&w.kind()));
    let plain = |w: tree_sitter::Node| -> Option<String> {
        let text = w.utf8_text(src.as_bytes()).ok()?;
        let lexed = lex_word(text)?;
        let w = &lexed.chars;
        // What bash rewrites before exec. Unlike `word_is_dynamic`, an escaped
        // char is literal here (the `'\''` inside a single-quoted body).
        let literal = !lexed.ansi_escape
            && !w
                .iter()
                .any(|ch| ch.q == Q::Bare && matches!(ch.c, '$' | '~'))
            && !is_glob(w)
            && matches!(expand_braces(w).as_deref(), Ok([one]) if one == w);
        literal.then(|| w.iter().map(|ch| ch.c).collect())
    };
    // The program: the first word that is not a wrapper or a wrapper's flag.
    let mut first = true;
    let shell = loop {
        let w = words.next()?;
        let word = plain(w)?;
        let base = word.rsplit('/').next().unwrap_or("").to_string();
        let wrapper = crate::safety::RM_COMMAND_WRAPPERS.contains(&base.as_str())
            || crate::safety::COMMAND_WRAPPERS.contains(&base.as_str())
            || (!first && (word.starts_with('-') || word.contains('=')));
        first = false;
        if !wrapper {
            break base;
        }
    };
    if !DASH_C_SHELLS.contains(&shell.as_str()) {
        return None;
    }
    // The shell's options: `-c` (alone or in a cluster such as `-lc`) makes
    // the first operand the body; `-o`/`-O` take a value; `--` ends options.
    let (mut dash_c, mut takes_value, mut options_done) = (false, false, false);
    for w in words {
        let word = plain(w)?;
        if takes_value {
            takes_value = false;
            continue;
        }
        let option =
            !options_done && word.len() > 1 && (word.starts_with('-') || word.starts_with('+'));
        if !option {
            // The first operand: the body under -c, else a script file.
            return dash_c.then_some(word);
        }
        if word == "--" {
            options_done = true;
        } else if !word.starts_with("--") {
            let cluster = &word[1..]; // word starts with an ASCII `-` / `+`
            dash_c |= word.starts_with('-') && cluster.contains('c');
            takes_value = cluster.contains('o') || cluster.contains('O');
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

/// #1202 step 2: what the allow rules make of a command read as a tree.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TreeAllow {
    /// Every simple command is covered and the tree is word-only: run unasked.
    Allowed,
    /// An allow rule covers part of it, but not enough: ask, saying why.
    Ask(String),
    /// No allow rule covers any of it: the ordinary prompt.
    NotCovered,
}

/// Owner's step 2: an allow rule auto-approves only when **every** simple
/// command the tree extracts satisfies `allowed` and the whole tree is
/// word-only in Codex's sense (`try_parse_word_only_commands_sequence`: plain
/// words joined by `&&` `||` `;` `|` or newlines, so no redirect,
/// substitution, expansion, assignment, subshell or control flow). Anything
/// else asks. `allowed` gets each command's source text, the spelling the
/// user's patterns are written in; a word-only tree has no word bash rewrites
/// (`word_is_dynamic`), so that text is what runs, less its quotes.
pub(crate) fn tree_allow(cmd: &str, allowed: &dyn Fn(&str) -> bool) -> TreeAllow {
    let mut uncovered = None;
    let mut covered_any = false;
    let _ = walk_nodes(cmd, &["command"], &mut |text, _| {
        if allowed(text) {
            covered_any = true;
        } else if uncovered.is_none() {
            uncovered = Some(text.replace('\n', " "));
        }
    });
    let shape = word_only_shape(cmd, true, true);
    match (shape, uncovered) {
        (Ok(n), None) if n > 0 => TreeAllow::Allowed,
        _ if !covered_any => TreeAllow::NotCovered,
        (Err(why), _) => TreeAllow::Ask(why),
        (Ok(_), Some(text)) => {
            let shown = crate::format::truncate_with_ellipsis(&text, 60);
            TreeAllow::Ask(format!("`{shown}` is not covered by an allow rule"))
        }
        (Ok(_), None) => TreeAllow::NotCovered, // no command at all
    }
}

/// The confirm closure's allow verdict under step 2, and the reason to print
/// when an allow rule matched but the command still asks. `cmd` is the text
/// the closure got, `inner` its command (`warned_command`), `check` the allow
/// list's verdict on a text. Deny wins. A command the safety analysis warned
/// about is approved only if the old whole-text rule already approved it AND
/// the tree does: step 2 never approves a warned command it did not before.
pub(crate) fn tree_allow_verdict(
    cmd: &str,
    inner: &str,
    check: &dyn Fn(&str) -> Option<bool>,
) -> (Option<bool>, Option<String>) {
    let whole = check(cmd);
    if whole == Some(false) {
        return (whole, None);
    }
    let warned = inner.len() != cmd.len();
    match tree_allow(inner, &|unit| check(unit) == Some(true)) {
        TreeAllow::Allowed if !warned || whole == Some(true) => (Some(true), None),
        TreeAllow::Ask(why) => (None, Some(why)),
        _ if whole == Some(true) => (
            None,
            Some("an allow rule matches the whole text, but not each command in it".into()),
        ),
        _ => (None, None),
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
fn argv_text(
    unit: tree_sitter::Node,
    src: &str,
    said: &[(usize, usize)],
) -> Result<Option<String>, String> {
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
                let (s, e) = (word.start_byte(), word.end_byte());
                if said.iter().any(|r| r.0 >= s && r.1 <= e) {
                    // #1210: a message value is a mention, not argv to re-spell.
                    edits.push((s, e, blank_mentions(src, s, e, said)));
                } else if let Some(words) = respell_word(text, word.kind() == "command_name")? {
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
