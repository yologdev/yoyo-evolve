//! Shared brace scanner for the `tests/` invariant gates.
//!
//! `tests/global_state_races.rs` and `tests/cargo_spawning_tests.rs` both need
//! to carve a `fn` body out of Rust source and ask whether it *directly* calls
//! some named function. Each carried its own byte-identical copy of this
//! scanner until Day 189 (#835); two copies of a rule agree the day they are
//! written and diverge forever after, and these are the parsers underneath two
//! gates — a divergence means one gate silently stops seeing a construct the
//! other sees. One statement, one home.
//!
//! Deliberately NOT shared with `src/commands_refactor::significant_braces`:
//! that scanner answers a different question (which braces are *structural*,
//! for a file-rewriting refactor) with different consumers. They share one
//! lesson, not one implementation.
//!
//! `tests/git_chokepoint.rs` deliberately does not use this either — a line
//! scan plus `#[cfg(test)]` file truncation is sufficient there and much
//! smaller.
//!
//! This lives in `tests/common/mod.rs` (a *subdirectory*) rather than
//! `tests/common.rs` on purpose: cargo compiles every top-level `tests/*.rs`
//! as its own test target, so the latter would emit a spurious "0 tests"
//! binary. It is compiled separately into each consumer's crate, so an item
//! nothing in *that* crate calls is dead code there.
//!
//! Limit, stated rather than implied: this is not a Rust lexer. A macro token
//! tree with deliberately unbalanced braces would mis-scope a body. There are
//! none in `src/` today, and the scanner is pinned by its consumers' tables.

pub fn match_body(src: &[char], from: usize) -> Option<(String, usize)> {
    let open = (from..src.len()).find(|&i| src[i] == '{')?;
    let mut depth = 0usize;
    let mut i = open;
    while i < src.len() {
        let c = src[i];
        match c {
            '/' if i + 1 < src.len() && src[i + 1] == '/' => {
                while i < src.len() && src[i] != '\n' {
                    i += 1;
                }
            }
            '/' if i + 1 < src.len() && src[i + 1] == '*' => {
                let mut nest = 1usize;
                i += 2;
                while i < src.len() && nest > 0 {
                    if src[i] == '/' && i + 1 < src.len() && src[i + 1] == '*' {
                        nest += 1;
                        i += 2;
                    } else if src[i] == '*' && i + 1 < src.len() && src[i + 1] == '/' {
                        nest -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            'r' if raw_string_hashes(src, i).is_some() => {
                let hashes = raw_string_hashes(src, i).unwrap();
                i += 1 + hashes + 1; // r + #* + "
                i = skip_raw_string(src, i, hashes);
            }
            'b' if i + 1 < src.len()
                && src[i + 1] == 'r'
                && raw_string_hashes(src, i + 1).is_some() =>
            {
                let hashes = raw_string_hashes(src, i + 1).unwrap();
                i += 2 + hashes + 1;
                i = skip_raw_string(src, i, hashes);
            }
            '"' => {
                i += 1;
                while i < src.len() {
                    if src[i] == '\\' {
                        i += 2;
                    } else if src[i] == '"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            '\'' => match char_literal_len(src, i) {
                Some(n) => i += n,
                None => i += 1,
            },
            '{' => {
                depth += 1;
                i += 1;
            }
            '}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some((src[open..i].iter().collect(), i));
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// `Some(hash_count)` when a raw string literal opens at `i` (`src[i] == 'r'`).
pub fn raw_string_hashes(src: &[char], i: usize) -> Option<usize> {
    if src.get(i) != Some(&'r') {
        return None;
    }
    // An `r` that is merely the tail of an identifier opens nothing.
    if i > 0 && (src[i - 1].is_alphanumeric() || src[i - 1] == '_') {
        return None;
    }
    let mut j = i + 1;
    let mut hashes = 0usize;
    while src.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if src.get(j) == Some(&'"') {
        Some(hashes)
    } else {
        None
    }
}

/// Advance past a raw string body, given the opening hash count. `i` points
/// just past the opening quote.
pub fn skip_raw_string(src: &[char], mut i: usize, hashes: usize) -> usize {
    while i < src.len() {
        if src[i] == '"' {
            let mut k = i + 1;
            let mut seen = 0usize;
            while seen < hashes && src.get(k) == Some(&'#') {
                seen += 1;
                k += 1;
            }
            if seen == hashes {
                return k;
            }
        }
        i += 1;
    }
    i
}

/// Length of the char literal starting at `i`, or `None` if the tick opens no
/// closed literal (a lifetime: `&'a str`, `'static`).
pub fn char_literal_len(src: &[char], i: usize) -> Option<usize> {
    if src.get(i) != Some(&'\'') {
        return None;
    }
    let mut j = i + 1;
    if src.get(j) == Some(&'\\') {
        j += 1;
        // `\u{7d}` and friends: run to the closing tick.
        while j < src.len() && src[j] != '\'' {
            j += 1;
        }
    } else if j < src.len() {
        j += 1;
    }
    if src.get(j) == Some(&'\'') {
        Some(j - i + 1)
    } else {
        None
    }
}

/// Does `body` contain a *direct* call to `name`?
///
/// Requires the char before the name to be neither an identifier char nor `.`
/// — so `some_module::the_name()` and a bare `the_name()` both count, while
/// `x.the_name()` and `my_the_name()` do not.
pub fn calls_directly(body: &str, name: &str) -> bool {
    let b: Vec<char> = body.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let mut i = 0usize;
    while i + n.len() <= b.len() {
        if b[i..i + n.len()] == n[..] {
            let before_ok =
                i == 0 || !(b[i - 1].is_alphanumeric() || b[i - 1] == '_' || b[i - 1] == '.');
            let mut j = i + n.len();
            while j < b.len() && b[j].is_whitespace() {
                j += 1;
            }
            if before_ok && b.get(j) == Some(&'(') {
                return true;
            }
        }
        i += 1;
    }
    false
}
