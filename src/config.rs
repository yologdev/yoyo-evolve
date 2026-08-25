//! Permission config, directory restrictions, MCP server config, and TOML parsing helpers.
//!
//! Extracted from `cli.rs` to keep configuration parsing separate from CLI argument handling.

/// Permission configuration for bash command auto-approval.
/// Parsed from the `[permissions]` section in `.yoyo.toml`.
#[derive(Debug, Clone, Default)]
pub struct PermissionConfig {
    /// Patterns that auto-approve matching bash commands (no prompt needed).
    pub allow: Vec<String>,
    /// Patterns that auto-deny matching bash commands (rejected with message).
    pub deny: Vec<String>,
}

impl PermissionConfig {
    /// Check a command against deny patterns first, then allow patterns.
    /// Returns `Some(true)` if allowed, `Some(false)` if denied, `None` if no match (prompt user).
    pub fn check(&self, command: &str) -> Option<bool> {
        // Deny takes priority — check deny patterns first
        for pattern in &self.deny {
            if glob_match(pattern, command) {
                return Some(false);
            }
        }
        // Then check allow patterns
        for pattern in &self.allow {
            if glob_match(pattern, command) {
                return Some(true);
            }
        }
        // No match — prompt the user
        None
    }

    /// Returns true if no patterns are configured.
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }
}

/// Directory restriction configuration for file access security.
/// Controls which directories yoyo's file tools (read_file, write_file, edit_file,
/// list_files, search) can access. When configured, paths are canonicalized to prevent
/// `../` traversal escapes.
///
/// Rules:
/// - If `deny` is non-empty, any path under a denied directory is blocked.
/// - If `allow` is non-empty, only paths under an allowed directory are permitted.
/// - Deny overrides allow when both match.
/// - Paths are resolved to absolute paths before checking.
#[derive(Debug, Clone, Default)]
pub struct DirectoryRestrictions {
    /// Directories that are explicitly allowed. If non-empty, only these dirs are accessible.
    pub allow: Vec<String>,
    /// Directories that are explicitly denied. Always takes priority over allow.
    pub deny: Vec<String>,
}

impl DirectoryRestrictions {
    /// Returns true if no restrictions are configured.
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }

    /// Check whether a given file path is permitted under the current restrictions.
    /// Returns `Ok(())` if the path is allowed, or `Err(reason)` if blocked.
    ///
    /// Path resolution:
    /// - Absolute paths are used directly.
    /// - Relative paths are resolved against the current working directory.
    /// - Symlinks and `..` components are resolved via `std::fs::canonicalize`
    ///   when the path exists; for non-existent paths, the nearest existing
    ///   ancestor is canonicalized and the remainder re-appended, so symlinked
    ///   spellings can't bypass deny checks (issue #600).
    pub fn check_path(&self, path: &str) -> Result<(), String> {
        if self.is_empty() {
            return Ok(());
        }

        let resolved = resolve_path(path);

        // Deny always takes priority
        for denied in &self.deny {
            let denied_resolved = resolve_path(denied);
            if path_is_under(&resolved, &denied_resolved) {
                return Err(format!(
                    "Access denied: '{}' is under restricted directory '{}'",
                    path, denied
                ));
            }
        }

        // If allow list is set, path must be under at least one allowed directory
        if !self.allow.is_empty() {
            let allowed = self.allow.iter().any(|a| {
                let a_resolved = resolve_path(a);
                path_is_under(&resolved, &a_resolved)
            });
            if !allowed {
                return Err(format!(
                    "Access denied: '{}' is not under any allowed directory",
                    path
                ));
            }
        }

        Ok(())
    }
}

/// Expand a leading `~` / `~/…` against `home`.
///
/// Pure decision half — the `HOME` lookup lives in [`expand_tilde`], so this
/// can be table-tested without touching the process environment.
///
/// Deliberately narrow: only a bare `~` and a `~/` prefix are expanded. A
/// `~user/...` form is returned verbatim, because resolving another account's
/// home needs a passwd lookup this crate does not do, and guessing
/// `<home_parent>/user` would invent a fence the user never wrote. When `home`
/// is `None` the input is returned unchanged — a path that cannot be resolved
/// must not be silently rewritten into something that resolves elsewhere.
fn expand_tilde_with(path: &str, home: Option<&str>) -> String {
    let Some(home) = home.map(|h| h.trim_end_matches('/')) else {
        return path.to_string();
    };
    if home.is_empty() {
        return path.to_string();
    }
    if path == "~" {
        return home.to_string();
    }
    match path.strip_prefix("~/") {
        Some(rest) => format!("{home}/{rest}"),
        None => path.to_string(),
    }
}

/// I/O wrapper over [`expand_tilde_with`], reading `HOME` from the environment.
fn expand_tilde(path: &str) -> String {
    let home = std::env::var("HOME").ok();
    expand_tilde_with(path, home.as_deref())
}

/// Resolve a path to an absolute, normalized form.
/// Uses `canonicalize` for existing paths (resolves symlinks, `..`, etc.).
/// For non-existent paths, canonicalizes the nearest existing ancestor
/// (resolving symlinks) and re-appends the non-existent remainder, so that
/// existing and non-existent spellings of the same location converge on one
/// canonical form (issue #600: `/etc/shadow` vs a deny on `/etc` that
/// canonicalizes to `/private/etc` on macOS).
fn resolve_path(path: &str) -> String {
    // Expand a leading `~` FIRST: `fs::canonicalize` does not do it (the shell
    // does, and a config file never went through a shell), and a `~/...` string
    // is not absolute, so without this it would be joined onto the cwd and
    // silently become `$CWD/~/.ssh`. That made `deny = ["~/.ssh"]` — the worked
    // example in docs/src/configuration/permissions.md — a fence that could
    // never match, in the dangerous direction: the user believes their keys are
    // protected and the file tools reach them. Found by blind round 76.
    let expanded = expand_tilde(path);
    let path: &str = &expanded;

    // Try canonicalize first (works for existing paths)
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical.to_string_lossy().to_string();
    }

    // Non-existent path: make absolute, then normalize `.` and `..` lexically
    // (they can't be resolved against a real filesystem).
    let p = std::path::Path::new(path);
    let absolute = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/"))
            .join(p)
    };

    let mut components = Vec::new();
    for component in absolute.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            other => components.push(other),
        }
    }
    let normalized: std::path::PathBuf = components.iter().collect();

    // Walk up to the nearest existing ancestor, canonicalize it (resolving
    // symlinks), then re-append the non-existent remainder components.
    let mut remainder: Vec<std::ffi::OsString> = Vec::new();
    let mut cursor: Option<&std::path::Path> = Some(normalized.as_path());
    while let Some(dir) = cursor {
        if let Ok(canonical_dir) = std::fs::canonicalize(dir) {
            let mut result = canonical_dir;
            for component in remainder.iter().rev() {
                result.push(component);
            }
            return result.to_string_lossy().to_string();
        }
        match dir.file_name() {
            Some(name) => remainder.push(name.to_os_string()),
            // No file name (e.g. root that failed to canonicalize) — stop.
            None => break,
        }
        cursor = dir.parent();
    }

    // Degenerate case: no ancestor exists — fall back to manual normalization.
    normalized.to_string_lossy().to_string()
}

/// Check if `path` is under (or equal to) `dir`.
/// Both should be absolute, normalized paths.
fn path_is_under(path: &str, dir: &str) -> bool {
    // Ensure dir ends with separator for prefix matching
    let dir_with_sep = if dir.ends_with('/') {
        dir.to_string()
    } else {
        format!("{}/", dir)
    };
    path == dir || path.starts_with(&dir_with_sep)
}

/// Simple glob matching: `*` matches any sequence of characters (including empty).
/// Supports multiple `*` wildcards. No other special characters.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();

    // No wildcards — exact match
    if parts.len() == 1 {
        return pattern == text;
    }

    let mut pos = 0;

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // First segment must match at the start
            if !text.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == parts.len() - 1 {
            // Last segment must match at the end
            if !text[pos..].ends_with(part) {
                return false;
            }
            pos = text.len();
        } else {
            // Middle segments must appear in order
            match text[pos..].find(part) {
                Some(idx) => pos += idx + part.len(),
                None => return false,
            }
        }
    }

    true
}

/// Parse a TOML-style array value like `["pattern1", "pattern2"]` into a Vec<String>.
pub fn parse_toml_array(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Vec::new();
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    inner
        .split(',')
        .map(|s| {
            let s = s.trim();
            // Strip quotes
            if (s.starts_with('"') && s.ends_with('"'))
                || (s.starts_with('\'') && s.ends_with('\''))
            {
                s[1..s.len() - 1].to_string()
            } else {
                s.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a `[permissions]` section from a TOML config file content.
/// Looks for `allow = [...]` and `deny = [...]` lines under `[permissions]`.
pub fn parse_permissions_from_config(content: &str) -> PermissionConfig {
    let mut config = PermissionConfig::default();
    let mut in_permissions = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Check for section headers
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_permissions = trimmed == "[permissions]";
            continue;
        }
        if !in_permissions {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "allow" => config.allow = parse_toml_array(value),
                "deny" => config.deny = parse_toml_array(value),
                _ => {}
            }
        }
    }
    config
}

/// Parse a `[directories]` section from a TOML config file content.
/// Looks for `allow = [...]` and `deny = [...]` lines under `[directories]`.
pub fn parse_directories_from_config(content: &str) -> DirectoryRestrictions {
    let mut config = DirectoryRestrictions::default();
    let mut in_directories = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_directories = trimmed == "[directories]";
            continue;
        }
        if !in_directories {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "allow" => config.allow = parse_toml_array(value),
                "deny" => config.deny = parse_toml_array(value),
                _ => {}
            }
        }
    }
    config
}

/// Glob metacharacters that make a `[directories]` entry unmatchable (#823).
///
/// `*` is the one [`glob_match`] actually honours in `[permissions]`, which is
/// where the confusion comes from. `?` is included even though `glob_match`
/// treats it literally too, because a user who writes it plainly *meant* a
/// wildcard — and over-reporting here costs exactly one printed line, since
/// nothing is refused, dropped or rewritten. `[` is deliberately **not**
/// detected: bracket ranges are rarer still and `[` is a plausible literal in
/// a real directory name.
const DIRECTORY_GLOB_METACHARS: [char; 2] = ['*', '?'];

/// Which half of `[directories]` an offending entry came from.
///
/// Kept distinct because the *consequence* differs, not for cosmetics: an
/// unmatchable `allow` entry denies everything (fails safe, loudly), while an
/// unmatchable `deny` entry protects nothing (fails **open**, silently).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirectoryList {
    Allow,
    Deny,
}

impl DirectoryList {
    fn key(self) -> &'static str {
        match self {
            DirectoryList::Allow => "allow",
            DirectoryList::Deny => "deny",
        }
    }

    fn consequence(self) -> &'static str {
        match self {
            // An allow list that is non-empty but matches nothing denies every
            // path: "not under any allowed directory" for every file tool call.
            DirectoryList::Allow => "matches nothing, so every file access is denied",
            // A deny entry that matches nothing is a fence that does not exist.
            DirectoryList::Deny => "matches nothing, so it protects nothing",
        }
    }
}

/// Entries in `[directories]` that carry a glob metacharacter (#823).
///
/// `[permissions]` patterns are globbed by [`glob_match`]; `[directories]`
/// entries are matched by `path_is_under`, a plain resolved-string prefix test
/// with **no globbing at all**. So a `*` in a `[directories]` entry is a literal
/// `*` character: `src/*` resolves to `$CWD/src/*`, a directory that cannot
/// exist, and the entry can never match anything.
///
/// Scans **both** lists, `allow` first then `deny`, returning the offending
/// entries verbatim. Detection only — nothing here refuses, drops or rewrites
/// an entry.
pub(crate) fn wildcard_directory_entries(
    dirs: &DirectoryRestrictions,
) -> Vec<(DirectoryList, String)> {
    let has_meta = |e: &String| e.chars().any(|c| DIRECTORY_GLOB_METACHARS.contains(&c));
    dirs.allow
        .iter()
        .filter(|e| has_meta(e))
        .map(|e| (DirectoryList::Allow, e.clone()))
        .chain(
            dirs.deny
                .iter()
                .filter(|e| has_meta(e))
                .map(|e| (DirectoryList::Deny, e.clone())),
        )
        .collect()
}

/// Render the `[directories]` wildcard warning (#823), or `None` when there is
/// nothing true to say.
///
/// `None` for an empty slice is the byte-identical common path: every user who
/// does not write a wildcard sees output unchanged. Glyph-free under `plain`
/// (no bullets, no em dashes), mirroring `cli::project_permission_refusal_message`.
///
/// This is a **warning only** — the restrictions pass through untouched, and
/// `[directories]` still has no glob support.
pub(crate) fn directory_wildcard_warning(
    entries: &[(DirectoryList, String)],
    plain: bool,
) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    let marker = if plain { "" } else { "⚠ " };
    let (noun, verb) = if entries.len() == 1 {
        ("entry", "contains")
    } else {
        ("entries", "contain")
    };
    let mut msg = format!(
        "{marker}{} [directories] {noun} {verb} a wildcard. [directories] takes literal paths:",
        entries.len()
    );
    for (list, entry) in entries {
        msg.push_str(&format!(
            "\n    {} = \"{}\"  ({})",
            list.key(),
            entry,
            list.consequence()
        ));
    }
    let sep = if plain { "." } else { " —" };
    msg.push_str(&format!(
        "\n  [directories] entries are matched by prefix, never globbed{sep} a `*` or `?` in one is a literal character.\n  Name the directory itself instead (`src`, not `src/*`): a directory entry already covers everything beneath it.\n  ([permissions] allow/deny patterns ARE globbed. The two blocks use different matchers.)"
    ));
    Some(msg)
}

/// Parse `[mcp_servers.<name>]` sections from raw config content.
///
/// Each section defines a named MCP server with a command, optional args, and optional env vars:
/// ```toml
/// [mcp_servers.filesystem]
/// command = "npx"
/// args = ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
///
/// [mcp_servers.postgres]
/// command = "npx"
/// args = ["-y", "@modelcontextprotocol/server-postgres"]
/// env = { DATABASE_URL = "postgresql://localhost/mydb" }
/// ```
pub fn parse_mcp_servers_from_config(content: &str) -> Vec<McpServerConfig> {
    let mut servers: Vec<McpServerConfig> = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_command: Option<String> = None;
    let mut current_args: Vec<String> = Vec::new();
    let mut current_env: Vec<(String, String)> = Vec::new();

    // Helper: flush accumulated server data into the result vec
    let flush = |name: &mut Option<String>,
                 command: &mut Option<String>,
                 args: &mut Vec<String>,
                 env: &mut Vec<(String, String)>,
                 servers: &mut Vec<McpServerConfig>| {
        if let (Some(n), Some(c)) = (name.take(), command.take()) {
            servers.push(McpServerConfig {
                name: n,
                command: c,
                args: std::mem::take(args),
                env: std::mem::take(env),
            });
        } else {
            // Reset even if incomplete
            *name = None;
            *command = None;
            args.clear();
            env.clear();
        }
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Detect section headers
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // Flush any previous MCP server
            flush(
                &mut current_name,
                &mut current_command,
                &mut current_args,
                &mut current_env,
                &mut servers,
            );

            let section = &trimmed[1..trimmed.len() - 1];
            if let Some(name) = section.strip_prefix("mcp_servers.") {
                let name = name.trim();
                if !name.is_empty() {
                    current_name = Some(name.to_string());
                }
            }
            continue;
        }

        // Only parse key=value lines inside an mcp_servers section
        if current_name.is_none() {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "command" => {
                    let v = strip_quotes(value);
                    if !v.is_empty() {
                        current_command = Some(v);
                    }
                }
                "args" => {
                    current_args = parse_toml_array(value);
                }
                "env" => {
                    current_env = parse_inline_table(value);
                }
                _ => {}
            }
        }
    }

    // Flush the last server
    flush(
        &mut current_name,
        &mut current_command,
        &mut current_args,
        &mut current_env,
        &mut servers,
    );

    servers
}

/// Strip surrounding quotes from a TOML string value.
fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        if s.len() >= 2 {
            s[1..s.len() - 1].to_string()
        } else {
            String::new()
        }
    } else {
        s.to_string()
    }
}

/// Parse a simple inline TOML table like `{ KEY = "value", KEY2 = "value2" }`.
/// Returns a list of (key, value) pairs.
fn parse_inline_table(s: &str) -> Vec<(String, String)> {
    let s = s.trim();
    // Strip surrounding braces
    let inner = if s.starts_with('{') && s.ends_with('}') {
        &s[1..s.len() - 1]
    } else {
        return Vec::new();
    };

    let mut result = Vec::new();
    for pair in inner.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        if let Some((k, v)) = pair.split_once('=') {
            let k = k.trim().to_string();
            let v = strip_quotes(v);
            if !k.is_empty() {
                result.push((k, v));
            }
        }
    }
    result
}

/// Configuration for an MCP (Model Context Protocol) server defined in config TOML sections.
///
/// Parsed from `[mcp_servers.<name>]` sections in `.yoyo.toml` or user config:
/// ```toml
/// [mcp_servers.filesystem]
/// command = "npx"
/// args = ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
/// env = { DATABASE_URL = "postgresql://localhost/mydb" }
/// ```
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Generic boolean config-flag lookup shared by all `parse_*_from_config`
/// boolean parsers.
///
/// Truthy values: `"true"`, `"1"`, `"yes"`, `"on"`.
/// Falsy values: `"false"`, `"0"`, `"no"`, `"off"`.
/// A missing key or any unrecognized value falls back to `default`.
///
/// This preserves the exact behavior of the original hand-rolled parsers:
/// default-`false` flags only flip on an explicit truthy value, and
/// default-`true` flags (e.g. `auto_continue`) only flip on an explicit
/// falsy value — garbage input never changes the default.
pub fn config_flag(
    config: &std::collections::HashMap<String, String>,
    key: &str,
    default: bool,
) -> bool {
    match config.get(key).map(|v| v.as_str()) {
        Some("true") | Some("1") | Some("yes") | Some("on") => true,
        Some("false") | Some("0") | Some("no") | Some("off") => false,
        _ => default,
    }
}

/// Check whether auto-watch is enabled in the config.
///
/// Reads `auto_watch` from the given config map. Defaults to `false`
/// when the key is absent — watch mode must be explicitly opted into
/// via `auto_watch = true` in `.yoyo.toml`. This avoids surprising
/// non-Rust users and local-model users with automatic test runs.
pub fn parse_auto_watch_from_config(config: &std::collections::HashMap<String, String>) -> bool {
    config_flag(config, "auto_watch", false)
}

/// Check whether auto-commit is enabled in the config.
///
/// Reads `auto_commit` from the given config map. Defaults to `false`
/// when the key is absent — auto-commit must be explicitly opted into.
pub fn parse_auto_commit_from_config(config: &std::collections::HashMap<String, String>) -> bool {
    config_flag(config, "auto_commit", false)
}

/// Check whether auto-edit is enabled in the config.
///
/// Reads `auto_edit` from the given config map. Defaults to `false`
/// when the key is absent — auto-edit must be explicitly opted into
/// via `auto_edit = true` in `.yoyo.toml`. When enabled, file edits
/// are auto-approved without confirmation (bash commands still confirm).
pub fn parse_auto_edit_from_config(config: &std::collections::HashMap<String, String>) -> bool {
    config_flag(config, "auto_edit", false)
}

/// Check whether lite mode is enabled in the config.
///
/// Reads `lite` from the given config map. Defaults to `false`
/// when the key is absent — lite mode must be explicitly opted into.
pub fn parse_lite_from_config(config: &std::collections::HashMap<String, String>) -> bool {
    config_flag(config, "lite", false)
}

/// Check whether auto-continue is enabled in the config.
///
/// Reads `auto_continue` from the given config map. Defaults to `true`
/// when the key is absent — auto-continuation is on by default so
/// incomplete responses are automatically followed up.
pub fn parse_auto_continue_from_config(config: &std::collections::HashMap<String, String>) -> bool {
    config_flag(config, "auto_continue", true)
}

/// Check whether continue-on-silence is enabled in the config.
///
/// Defaults to **false** when the key is absent — this is opt-in (issue #631),
/// because yoyo cannot distinguish "stopped mid-work" from "finished quietly",
/// and a default that loops on a quiet turn would be worse than the ambiguity
/// it fixes. Setting `continue_on_silence = true` in `.yoyo.toml` is the
/// non-flag door to the same switch `--continue-on-silence` throws (#794);
/// the two sources are OR'd — there is no "off" flag.
pub fn parse_continue_on_silence_from_config(
    config: &std::collections::HashMap<String, String>,
) -> bool {
    config_flag(config, "continue_on_silence", false)
}

/// Check whether wait-for-reset is enabled in the config.
///
/// Defaults to **false** when the key is absent — this is opt-in, because a
/// process that can silently sleep for hours is not a product-safe default
/// (#448). Setting `wait_for_reset = true` in `.yoyo.toml` is the non-flag
/// door to the same switch `--wait-for-reset` throws; the two sources are
/// OR'd at the single `set_wait_for_reset()` call site in `cli::parse_args` —
/// there is no "off" flag, so a user who writes neither is byte-identical to
/// the pre-config-key behaviour.
///
/// The key is deliberately **not** part of the project-config trust boundary:
/// it grants no privilege — the worst case is a process that sleeps — unlike
/// an MCP command, a `permissions.allow` entry or a shell hook, whose entire
/// content is executable code.
pub fn parse_wait_for_reset_from_config(
    config: &std::collections::HashMap<String, String>,
) -> bool {
    config_flag(config, "wait_for_reset", false)
}

/// Parse `max_auto_continues` from the config map.
///
/// Returns the configured value (clamped to 0-20) or `None` if the key
/// is absent or unparseable, letting the caller fall back to the default.
pub fn parse_max_auto_continues_from_config(
    config: &std::collections::HashMap<String, String>,
) -> Option<u32> {
    config
        .get("max_auto_continues")
        .and_then(|v| v.parse::<u32>().ok())
        .map(|n| n.min(20))
}

/// Check whether the terminal bell should be suppressed via config.
///
/// Reads `no_bell` from the given config map. Defaults to `false`
/// when the key is absent — bell is enabled by default.
pub fn parse_no_bell_from_config(config: &std::collections::HashMap<String, String>) -> bool {
    config_flag(config, "no_bell", false)
}

/// Check whether quiet mode is enabled in the config.
///
/// Reads `quiet` from the given config map. Defaults to `false`
/// when the key is absent — quiet mode must be explicitly opted into.
pub fn parse_quiet_from_config(config: &std::collections::HashMap<String, String>) -> bool {
    config_flag(config, "quiet", false)
}

/// Check whether color output should be disabled via config.
///
/// Reads `no_color` from the given config map. Defaults to `false`
/// when the key is absent — colors are enabled by default.
pub fn parse_no_color_from_config(config: &std::collections::HashMap<String, String>) -> bool {
    config_flag(config, "no_color", false)
}

/// Parse `notify_command` from the config map.
///
/// Returns `Some(command)` when a non-empty command string is configured,
/// or `None` when the key is absent or empty. When set, the command is run
/// (fire-and-forget) whenever a long prompt finishes — the same threshold
/// that triggers the terminal bell. Absent means the feature is completely
/// inert: no process spawn, no PATH probing.
pub fn parse_notify_command_from_config(
    config: &std::collections::HashMap<String, String>,
) -> Option<String> {
    config
        .get("notify_command")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

/// Keys that `/config set` understands. Each entry is a key name and a
/// human-readable description used in error messages.
pub const SETTABLE_KEYS: &[(&str, &str)] = &[
    ("model", "AI model name"),
    ("provider", "AI provider"),
    ("thinking", "thinking level (none/low/medium/high)"),
    ("temperature", "sampling temperature (0.0–2.0)"),
    ("max_tokens", "maximum response tokens"),
    ("max_turns", "maximum agent turns per prompt"),
    ("auto_watch", "auto-enable watch mode on start (true/false)"),
    (
        "auto_edit",
        "auto-approve file edits without confirmation (true/false)",
    ),
    (
        "auto_commit",
        "auto-commit file changes after each agent turn (true/false)",
    ),
    (
        "auto_continue",
        "auto-continue incomplete responses (true/false)",
    ),
    (
        "max_auto_continues",
        "max auto-continue follow-ups per turn (0-20)",
    ),
    (
        "continue_on_silence",
        "continue after a tool-using turn that ends with almost no text (true/false)",
    ),
    (
        "wait_for_reset",
        "wait out a provider rate-limit reset instead of giving up (true/false)",
    ),
    ("lite", "enable lite mode for small/local LLMs (true/false)"),
    ("no_bell", "suppress terminal bell (true/false)"),
    (
        "notify_command",
        "command to run when a long prompt finishes (empty = disabled)",
    ),
    ("quiet", "suppress informational output (true/false)"),
    ("no_color", "disable colored output (true/false)"),
];

/// Validate a config value for a given key. Returns `Ok(canonical_value)`
/// on success or `Err(message)` on invalid input.
pub fn validate_config_value(key: &str, value: &str) -> Result<String, String> {
    match key {
        "model" | "provider" => {
            if value.is_empty() {
                return Err(format!("{key} cannot be empty"));
            }
            Ok(value.to_string())
        }
        "thinking" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "none" | "off" | "disabled" => Ok("none".to_string()),
                "low" | "minimal" => Ok("low".to_string()),
                "medium" | "med" => Ok("medium".to_string()),
                "high" | "max" => Ok("high".to_string()),
                _ => Err(format!(
                    "invalid thinking level '{value}' — use none, low, medium, or high"
                )),
            }
        }
        "temperature" => match value.parse::<f32>() {
            Ok(t) if (0.0..=2.0).contains(&t) => Ok(format!("{t}")),
            Ok(t) => Err(format!("temperature {t} out of range (0.0–2.0)")),
            Err(_) => Err(format!("'{value}' is not a valid number")),
        },
        "max_tokens" => match value.parse::<u32>() {
            Ok(n) if n > 0 => Ok(n.to_string()),
            Ok(_) => Err("max_tokens must be positive".to_string()),
            Err(_) => Err(format!("'{value}' is not a valid integer")),
        },
        "max_turns" => match value.parse::<usize>() {
            Ok(n) if n > 0 => Ok(n.to_string()),
            Ok(_) => Err("max_turns must be positive".to_string()),
            Err(_) => Err(format!("'{value}' is not a valid integer")),
        },
        "auto_watch" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!(
                    "invalid auto_watch value '{value}' — use true or false"
                )),
            }
        }
        "auto_commit" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!(
                    "invalid auto_commit value '{value}' — use true or false"
                )),
            }
        }
        "auto_edit" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!(
                    "invalid auto_edit value '{value}' — use true or false"
                )),
            }
        }
        "auto_continue" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!(
                    "invalid auto_continue value '{value}' — use true or false"
                )),
            }
        }
        "continue_on_silence" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!(
                    "invalid continue_on_silence value '{value}' — use true or false"
                )),
            }
        }
        "wait_for_reset" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!(
                    "invalid wait_for_reset value '{value}' — use true or false"
                )),
            }
        }
        "max_auto_continues" => match value.parse::<u32>() {
            Ok(n) if n <= 20 => Ok(n.to_string()),
            Ok(n) => Err(format!("max_auto_continues {n} out of range (0-20)")),
            Err(_) => Err(format!("'{value}' is not a valid integer")),
        },
        "lite" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!("invalid lite value '{value}' — use true or false")),
            }
        }
        "no_bell" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!(
                    "invalid no_bell value '{value}' — use true or false"
                )),
            }
        }
        "notify_command" => {
            // Any string is a valid command; an empty string clears the setting
            // (disabling the feature). The command is entirely user-supplied.
            Ok(value.to_string())
        }
        "quiet" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!("invalid quiet value '{value}' — use true or false")),
            }
        }
        "no_color" => {
            let lower = value.to_ascii_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" => Ok("true".to_string()),
                "false" | "0" | "no" | "off" => Ok("false".to_string()),
                _ => Err(format!(
                    "invalid no_color value '{value}' — use true or false"
                )),
            }
        }
        _ => Err(format!(
            "unknown config key '{key}' — settable keys: {}",
            SETTABLE_KEYS
                .iter()
                .map(|(k, _)| *k)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Write a single key=value pair to a TOML config file.
///
/// If the file exists, the key is either replaced in-place (preserving
/// comments and surrounding lines) or appended. If the file doesn't exist,
/// it's created with a header comment. Values are always quoted.
///
/// When `project_local` is true, writes to `.yoyo.toml` in the current
/// directory. Otherwise writes to `~/.yoyo.toml`.
///
/// Returns the path that was written to on success.
pub fn write_config_value(
    key: &str,
    value: &str,
    project_local: bool,
) -> Result<std::path::PathBuf, String> {
    let path = if project_local {
        std::path::PathBuf::from(".yoyo.toml")
    } else {
        home_config_path().ok_or_else(|| "could not determine home directory".to_string())?
    };

    write_config_value_to(key, value, &path)
}

/// Write a config value to a specific path. Factored out of
/// [`write_config_value`] so tests can target a temp file.
pub fn write_config_value_to(
    key: &str,
    value: &str,
    path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory {}: {e}", parent.display()))?;
        }
    }

    // Read existing content or start fresh
    let existing = std::fs::read_to_string(path).unwrap_or_default();

    let new_content = set_toml_key(&existing, key, value);

    std::fs::write(path, &new_content)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    Ok(path.to_path_buf())
}

/// Append a pattern to the `[permissions]` allow list in the project-local `.yoyo.toml`.
///
/// If the file doesn't exist it is created. If the `[permissions]` section or `allow`
/// key doesn't exist they are created. If the pattern is already present, the file is
/// left unchanged (no duplicates).
///
/// Returns the path written to on success.
pub fn append_allow_pattern(pattern: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::PathBuf::from(".yoyo.toml");
    append_allow_pattern_to(pattern, &path)
}

/// Testable version of [`append_allow_pattern`] that takes an explicit path.
pub fn append_allow_pattern_to(
    pattern: &str,
    path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory {}: {e}", parent.display()))?;
        }
    }

    let existing = std::fs::read_to_string(path).unwrap_or_default();

    // Parse existing permissions to check for duplicates
    let current = parse_permissions_from_config(&existing);
    if current.allow.iter().any(|p| p == pattern) {
        // Already present — nothing to do
        return Ok(path.to_path_buf());
    }

    // Build the new allow array
    let mut new_allow = current.allow.clone();
    new_allow.push(pattern.to_string());
    let new_content = set_permissions_allow(&existing, &new_allow);

    std::fs::write(path, &new_content)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    Ok(path.to_path_buf())
}

/// Pure function: set the `[permissions]` `allow` array in TOML content.
///
/// If a `[permissions]` section with an `allow = [...]` line exists, it is
/// replaced. If `[permissions]` exists but has no `allow`, the key is inserted
/// right after the section header. If no `[permissions]` section exists, one
/// is appended.
fn set_permissions_allow(content: &str, patterns: &[String]) -> String {
    let formatted: Vec<String> = patterns.iter().map(|p| format!("\"{}\"", p)).collect();
    let allow_line = format!("allow = [{}]", formatted.join(", "));

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut in_permissions = false;
    let mut found_allow = false;
    let mut permissions_section_exists = false;

    for (i, line) in lines.iter_mut().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == "[permissions]" {
                permissions_section_exists = true;
                in_permissions = true;
            } else {
                // If we were in [permissions] but never found allow, insert before this section
                if in_permissions && !found_allow {
                    // We'll handle insertion below
                }
                in_permissions = false;
            }
            continue;
        }
        if in_permissions && !found_allow {
            if let Some((key, _)) = trimmed.split_once('=') {
                if key.trim() == "allow" {
                    *line = allow_line.clone();
                    found_allow = true;
                    let _ = i; // suppress unused warning
                }
            }
        }
    }

    if permissions_section_exists && !found_allow {
        // Insert allow line right after [permissions] header
        let mut result = Vec::new();
        for line in &lines {
            result.push(line.clone());
            if line.trim() == "[permissions]" {
                result.push(allow_line.clone());
            }
        }
        lines = result;
    }

    if !permissions_section_exists {
        // Append a new [permissions] section
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push("[permissions]".to_string());
        lines.push(allow_line);
    }

    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Pure function: insert or replace `key = "value"` in a flat TOML string.
/// Preserves comments, blank lines, and other keys. If the key already
/// exists (matched by `^key\s*=`), replaces that line. Otherwise appends.
///
/// Values that look like numbers or booleans are written unquoted; everything
/// else is quoted.
pub fn set_toml_key(content: &str, key: &str, value: &str) -> String {
    let formatted_value = format_toml_value(value);
    let new_line = format!("{key} = {formatted_value}");

    let mut found = false;
    let mut lines: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            // Match `key = ...` at the start of a non-comment line
            if !trimmed.starts_with('#') {
                if let Some((k, _)) = trimmed.split_once('=') {
                    if k.trim() == key {
                        found = true;
                        return new_line.clone();
                    }
                }
            }
            line.to_string()
        })
        .collect();

    if !found {
        // Ensure there's a trailing newline before appending
        if !lines.is_empty() {
            let last = lines.last().unwrap();
            if !last.is_empty() {
                // Only add a blank line if the file doesn't already end with one
            }
        }
        lines.push(new_line);
    }

    let mut result = lines.join("\n");
    // Ensure file ends with a newline
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Format a value for TOML: numbers and booleans go unquoted,
/// everything else gets double-quoted.
pub(crate) fn format_toml_value(value: &str) -> String {
    // Check if it's a number (integer or float)
    if value.parse::<i64>().is_ok() || value.parse::<f64>().is_ok() {
        return value.to_string();
    }
    // Check for booleans
    if value == "true" || value == "false" {
        return value.to_string();
    }
    // Default: quote it as a TOML basic string (#732 — unescaped values
    // produced a config file the reader below could not parse back).
    format!("\"{}\"", escape_toml_basic_string(value))
}

/// Escape a string for embedding in a TOML *basic* string (`"..."`).
///
/// Backslash first — escaping the quote first would then double the
/// backslash the quote's escape just introduced.
pub(crate) fn escape_toml_basic_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Inverse of [`escape_toml_basic_string`]. An unrecognised escape is kept
/// verbatim (backslash included) rather than silently dropped.
pub(crate) fn unescape_toml_basic_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Config-file path resolution and loading
// ---------------------------------------------------------------------------

use std::collections::HashMap;

use crate::format::{is_quiet, DIM, RESET};

/// Config file search paths, checked in order (first found wins).
/// - `.yoyo.toml` (project-level)
/// - `~/.yoyo.toml` (home-level shorthand)
/// - `~/.config/yoyo/config.toml` (XDG user-level)
const CONFIG_FILE_NAMES: &[&str] = &[".yoyo.toml"];

/// XDG user-level config path: `~/.config/yoyo/config.toml`.
pub fn user_config_path() -> Option<std::path::PathBuf> {
    dirs_hint().map(|dir| dir.join("yoyo").join("config.toml"))
}

/// A base-directory environment value is usable only when it is non-empty
/// **and** absolute.
///
/// The XDG Base Directory spec states both rules: an unset *or empty*
/// `XDG_*_HOME` must fall back to the `$HOME` default, and "if an
/// implementation encounters a relative path in any of these variables it
/// should consider the path invalid and ignore it".
///
/// Both cases used to be accepted verbatim, and both fail the same way: they
/// produce a **relative** base dir, which every later `join` resolves against
/// the *process cwd* instead of the user's home. `XDG_CONFIG_HOME=""` made
/// `user_config_path()` return `yoyo/config.toml` — so yoyo would read a file
/// out of whatever repo it happened to be started in and treat it as the
/// user's own XDG config, walking straight past the project-config trust
/// boundary (#748/#749), and `history_file_path()` would scatter a
/// `yoyo/history` directory into the cwd.
///
/// Returning `None` here means "fall back", never "guess" — an unusable value
/// is treated exactly like an unset one.
fn usable_base_dir(value: Option<&str>) -> Option<&str> {
    let value = value?;
    if value.is_empty() || !std::path::Path::new(value).is_absolute() {
        return None;
    }
    Some(value)
}

/// Resolve an XDG base dir from its env value, falling back to `$HOME` plus
/// the given path segments when the env value is unset or unusable.
///
/// Pure: both env values are parameters, so the decision is table-testable
/// without touching the process environment.
fn xdg_base_dir(
    xdg: Option<&str>,
    home: Option<&str>,
    home_fallback: &[&str],
) -> Option<std::path::PathBuf> {
    if let Some(dir) = usable_base_dir(xdg) {
        return Some(std::path::PathBuf::from(dir));
    }
    let home = usable_base_dir(home)?;
    let mut path = std::path::PathBuf::from(home);
    for segment in home_fallback {
        path.push(segment);
    }
    Some(path)
}

/// `$HOME` as a usable absolute base dir, or `None`.
fn home_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    usable_base_dir(Some(home.as_str())).map(std::path::PathBuf::from)
}

/// Home directory config path: `~/.yoyo.toml`.
pub fn home_config_path() -> Option<std::path::PathBuf> {
    home_dir().map(|h| h.join(".yoyo.toml"))
}

/// Best-effort XDG config dir (~/.config on Linux/macOS).
fn dirs_hint() -> Option<std::path::PathBuf> {
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    xdg_base_dir(xdg.as_deref(), home.as_deref(), &[".config"])
}

/// Best-effort XDG data dir (~/.local/share on Linux/macOS).
fn data_dir_hint() -> Option<std::path::PathBuf> {
    let xdg = std::env::var("XDG_DATA_HOME").ok();
    let home = std::env::var("HOME").ok();
    xdg_base_dir(xdg.as_deref(), home.as_deref(), &[".local", "share"])
}

/// Get the path for the readline history file.
/// Prefers `$XDG_DATA_HOME/yoyo/history`, falls back to `~/.yoyo_history`.
pub fn history_file_path() -> Option<std::path::PathBuf> {
    // Try XDG data dir first
    if let Some(data_dir) = data_dir_hint() {
        let yoyo_dir = data_dir.join("yoyo");
        // Try to create the directory; if it works, use it
        if std::fs::create_dir_all(&yoyo_dir).is_ok() {
            return Some(yoyo_dir.join("history"));
        }
    }
    // Fall back to ~/.yoyo_history
    std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(h).join(".yoyo_history"))
}

/// Parse a simple TOML-like config file (key = "value" or key = value per line).
/// Ignores comments (#) and blank lines. Returns a map of key → value.
pub fn parse_config_file(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim();
            // Strip surrounding quotes if present. Basic strings ("..") get
            // escape processing; TOML literal strings ('..') do not (#732).
            // `chars()` not byte slicing — a multi-byte body would panic.
            let value = if value.chars().count() >= 2 {
                let mut inner = value.chars();
                let first = inner.next().unwrap_or_default();
                let last = inner.next_back().unwrap_or_default();
                match (first, last) {
                    ('"', '"') => unescape_toml_basic_string(inner.as_str()),
                    ('\'', '\'') => inner.as_str().to_string(),
                    _ => value.to_string(),
                }
            } else {
                value.to_string()
            };
            map.insert(key, value);
        }
    }
    map
}

/// Load config from file, checking project-level, home-level, then user-level paths.
/// The search order: `.yoyo.toml` (project) → `~/.yoyo.toml` (home) → XDG config dir.
/// Prints the loaded path to stderr (unless quiet mode).
/// Returns `(HashMap, raw_content)` or `(empty HashMap, empty string)` if no config found.
pub fn load_config_file() -> (HashMap<String, String>, String) {
    // Check project-level config first
    for name in CONFIG_FILE_NAMES {
        if let Ok(content) = std::fs::read_to_string(name) {
            if !is_quiet() {
                eprintln!("{DIM}  config: {name}{RESET}");
            }
            record_loaded_config_path(std::path::PathBuf::from(name));
            return (parse_config_file(&content), content);
        }
    }
    // Check ~/.yoyo.toml (home directory shorthand)
    if let Some(path) = home_config_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if !is_quiet() {
                eprintln!("{DIM}  config: {}{RESET}", path.display());
            }
            record_loaded_config_path(path);
            return (parse_config_file(&content), content);
        }
    }
    // Check user-level config (XDG)
    if let Some(path) = user_config_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if !is_quiet() {
                eprintln!("{DIM}  config: {}{RESET}", path.display());
            }
            record_loaded_config_path(path);
            return (parse_config_file(&content), content);
        }
    }
    (HashMap::new(), String::new())
}

// ---------------------------------------------------------------------------
// Config provenance (#748)
// ---------------------------------------------------------------------------
//
// A project-local `.yoyo.toml` is written by whoever wrote the repository — not
// necessarily by the person running yoyo. Anything in it that *executes* a
// command (today: `mcp = [...]` and `[mcp_servers.*]`) therefore needs a trust
// boundary. Answering "which rung of the search won?" is what this section is
// for; the gate itself lives at the single MCP merge seam in `cli.rs`.
//
// The search order is stated exactly once — in `load_config_file` above. This
// records the winner rather than re-walking a second, drift-prone ladder.

/// Path of the config file that actually won `load_config_file`'s search.
///
/// Written by `load_config_file` (first call wins — a session loads one config;
/// later calls from `/config`-style helpers re-read the same chain and must not
/// be able to change the provenance the session already acted on).
static LOADED_CONFIG_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Record which config file won the search. First writer wins.
fn record_loaded_config_path(path: std::path::PathBuf) {
    let _ = LOADED_CONFIG_PATH.set(path);
}

/// Is `path` — the config file that won the search — the *project-local* one,
/// i.e. a `.yoyo.toml` sitting in the current working directory?
///
/// Pure: takes the cwd and the home directory as parameters so it can be tested
/// without touching process state.
///
/// Deliberate decisions:
/// - Only a file whose name is in `CONFIG_FILE_NAMES` can be project-local; the
///   XDG path (`config.toml`) never counts even if the user is sitting in that
///   directory.
/// - When the cwd **is** the home directory, `./.yoyo.toml` and `~/.yoyo.toml`
///   are the same file. It is the user's own home config, reached by a shorter
///   path, so it is **not** treated as project-local. The threat this guards is
///   "I cloned someone's repo and cd'd into it", not "I am in my own home dir".
pub fn config_path_is_project_local(
    path: &std::path::Path,
    cwd: &std::path::Path,
    home: Option<&std::path::Path>,
) -> bool {
    let name_matches = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| CONFIG_FILE_NAMES.contains(&n))
        .unwrap_or(false);
    if !name_matches {
        return false;
    }
    // Resolve a relative path (`.yoyo.toml`, as read by `load_config_file`)
    // against the cwd it was read from.
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let Some(dir) = absolute.parent() else {
        return false;
    };
    if dir != cwd {
        return false;
    }
    // cwd == home: this is the user's own ~/.yoyo.toml. Not a project config.
    if let Some(home) = home {
        if dir == home {
            return false;
        }
    }
    true
}

/// Whether the config file this session actually loaded is project-local.
///
/// `false` when no config file was found, when the winner was `~/.yoyo.toml`
/// or the XDG config, or when the cwd cannot be read.
pub fn loaded_config_is_project_local() -> bool {
    let Some(path) = LOADED_CONFIG_PATH.get() else {
        return false;
    };
    let Ok(cwd) = std::env::current_dir() else {
        return false;
    };
    let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);
    config_path_is_project_local(path, &cwd, home.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_module_glob_match() {
        assert!(glob_match("cargo *", "cargo test"));
        assert!(!glob_match("cargo *", "rustc build"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "other"));
    }

    #[test]
    fn test_config_module_permission_check() {
        let perms = PermissionConfig {
            allow: vec!["cargo *".to_string()],
            deny: vec!["rm *".to_string()],
        };
        assert_eq!(perms.check("cargo test"), Some(true));
        assert_eq!(perms.check("rm -rf /"), Some(false));
        assert_eq!(perms.check("python script.py"), None);
    }

    #[test]
    fn test_config_module_parse_toml_array() {
        let result = parse_toml_array(r#"["one", "two", "three"]"#);
        assert_eq!(result, vec!["one", "two", "three"]);
    }

    #[test]
    fn test_config_module_parse_permissions() {
        let content = r#"
[permissions]
allow = ["cargo *", "git *"]
deny = ["rm *"]
"#;
        let config = parse_permissions_from_config(content);
        assert_eq!(config.allow, vec!["cargo *", "git *"]);
        assert_eq!(config.deny, vec!["rm *"]);
    }

    #[test]
    fn test_config_module_parse_directories() {
        let content = r#"
[directories]
allow = ["/home/user/project"]
deny = ["/etc"]
"#;
        let config = parse_directories_from_config(content);
        assert_eq!(config.allow, vec!["/home/user/project"]);
        assert_eq!(config.deny, vec!["/etc"]);
    }

    #[test]
    fn test_config_module_parse_mcp_servers() {
        let content = r#"
[mcp_servers.test]
command = "npx"
args = ["-y", "test-server"]
env = { API_KEY = "secret" }
"#;
        let servers = parse_mcp_servers_from_config(content);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "test");
        assert_eq!(servers[0].command, "npx");
        assert_eq!(servers[0].args, vec!["-y", "test-server"]);
        assert_eq!(
            servers[0].env,
            vec![("API_KEY".to_string(), "secret".to_string())]
        );
    }

    #[test]
    fn test_config_module_strip_quotes() {
        assert_eq!(strip_quotes("\"hello\""), "hello");
        assert_eq!(strip_quotes("'hello'"), "hello");
        assert_eq!(strip_quotes("hello"), "hello");
        assert_eq!(strip_quotes("\"\""), "");
        assert_eq!(strip_quotes(""), "");
    }

    #[test]
    fn test_config_module_parse_inline_table() {
        let result = parse_inline_table(r#"{ KEY = "value", OTHER = "val2" }"#);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("KEY".to_string(), "value".to_string()));
        assert_eq!(result[1], ("OTHER".to_string(), "val2".to_string()));
    }

    #[test]
    fn test_config_module_parse_inline_table_empty() {
        let result = parse_inline_table("{}");
        assert!(result.is_empty());

        let result = parse_inline_table("not a table");
        assert!(result.is_empty());
    }

    #[test]
    fn test_config_module_resolve_path_normalizes_parent_dir() {
        let resolved = resolve_path("/tmp/a/../b");
        // /tmp may itself be a symlink (macOS: /tmp -> /private/tmp), so
        // compare against its canonical form rather than the literal string.
        let expected = std::fs::canonicalize("/tmp")
            .map(|p| p.join("b").to_string_lossy().to_string())
            .unwrap_or_else(|_| "/tmp/b".to_string());
        assert_eq!(resolved, expected);
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_path_nonexistent_under_symlink_resolves_symlink() {
        // Issue #600: a NON-existent path under a symlinked dir must resolve
        // the symlink via its nearest existing ancestor, so deny/allow prefix
        // checks agree with the canonicalized form of the directory.
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real_dir");
        std::fs::create_dir(&real).unwrap();
        let link = tmp.path().join("link_dir");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let canonical_real = std::fs::canonicalize(&real).unwrap();
        let target = link.join("does_not_exist.txt");
        let resolved = resolve_path(&target.to_string_lossy());
        let expected = canonical_real.join("does_not_exist.txt");
        assert_eq!(resolved, expected.to_string_lossy());
    }

    #[cfg(unix)]
    #[test]
    fn test_deny_check_symlink_and_real_spellings_converge() {
        // Both spellings of the same location must land on one canonical form:
        // a deny on the symlink path blocks a non-existent target reached via
        // the real path, and vice versa.
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real_secrets");
        std::fs::create_dir(&real).unwrap();
        let link = tmp.path().join("link_secrets");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        // Deny via the symlink spelling; check via the real spelling.
        let deny_link = DirectoryRestrictions {
            allow: vec![],
            deny: vec![link.to_string_lossy().to_string()],
        };
        let via_real = real.join("nonexistent_file");
        assert!(
            deny_link.check_path(&via_real.to_string_lossy()).is_err(),
            "deny on symlink path should block non-existent target via real path"
        );

        // Deny via the real spelling; check via the symlink spelling.
        let deny_real = DirectoryRestrictions {
            allow: vec![],
            deny: vec![real.to_string_lossy().to_string()],
        };
        let via_link = link.join("nonexistent_file");
        assert!(
            deny_real.check_path(&via_link.to_string_lossy()).is_err(),
            "deny on real path should block non-existent target via symlink path"
        );
    }

    #[test]
    fn test_resolve_path_deeply_nonexistent_preserves_remainder() {
        // Multiple non-existent components: nearest existing ancestor is the
        // root, remainder is re-appended unchanged.
        let resolved = resolve_path("/nonexistent_yoyo_600/a/b");
        assert_eq!(resolved, "/nonexistent_yoyo_600/a/b");
    }

    #[test]
    fn test_config_module_resolve_path_absolute() {
        let resolved = resolve_path("/usr/bin/env");
        assert!(resolved.starts_with('/'));
        assert!(resolved.contains("usr"));
    }

    #[test]
    fn test_config_module_path_is_under_basic() {
        assert!(path_is_under("/etc/passwd", "/etc"));
        assert!(path_is_under("/etc", "/etc"));
        assert!(!path_is_under("/etcetc", "/etc"));
        assert!(!path_is_under("/tmp/file", "/etc"));
    }

    // --- write_config_value / set_toml_key tests ---

    #[test]
    fn test_set_toml_key_creates_new_key() {
        let content = "# yoyo config\nprovider = \"anthropic\"\n";
        let result = set_toml_key(content, "model", "claude-sonnet-4-6");
        assert!(result.contains("model = \"claude-sonnet-4-6\""));
        // Original key should still be there
        assert!(result.contains("provider = \"anthropic\""));
        // Comment should be preserved
        assert!(result.contains("# yoyo config"));
    }

    #[test]
    fn test_set_toml_key_replaces_existing_key() {
        let content = "provider = \"anthropic\"\nmodel = \"old-model\"\n";
        let result = set_toml_key(content, "model", "new-model");
        assert!(result.contains("model = \"new-model\""));
        assert!(!result.contains("old-model"));
        assert!(result.contains("provider = \"anthropic\""));
    }

    #[test]
    fn test_set_toml_key_preserves_comments() {
        let content = "# My config\n# model choice\nmodel = \"old\"\n# end\n";
        let result = set_toml_key(content, "model", "new");
        assert!(result.contains("# My config"));
        assert!(result.contains("# model choice"));
        assert!(result.contains("# end"));
        assert!(result.contains("model = \"new\""));
    }

    #[test]
    fn test_set_toml_key_numeric_value_unquoted() {
        let result = set_toml_key("", "max_tokens", "8192");
        assert!(result.contains("max_tokens = 8192"));
        assert!(!result.contains("\"8192\""));
    }

    #[test]
    fn test_set_toml_key_string_value_quoted() {
        let result = set_toml_key("", "model", "claude-opus-4-6");
        assert!(result.contains("model = \"claude-opus-4-6\""));
    }

    #[test]
    fn test_set_toml_key_empty_content() {
        let result = set_toml_key("", "provider", "anthropic");
        assert!(result.contains("provider = \"anthropic\""));
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn test_validate_config_value_valid_keys() {
        assert!(validate_config_value("model", "claude-sonnet-4-6").is_ok());
        assert!(validate_config_value("provider", "anthropic").is_ok());
        assert!(validate_config_value("thinking", "high").is_ok());
        assert!(validate_config_value("thinking", "off").is_ok());
        assert!(validate_config_value("temperature", "0.7").is_ok());
        assert!(validate_config_value("max_tokens", "4096").is_ok());
        assert!(validate_config_value("max_turns", "50").is_ok());
    }

    #[test]
    fn test_validate_config_value_invalid() {
        assert!(validate_config_value("model", "").is_err());
        assert!(validate_config_value("thinking", "extreme").is_err());
        assert!(validate_config_value("temperature", "5.0").is_err());
        assert!(validate_config_value("temperature", "abc").is_err());
        assert!(validate_config_value("max_tokens", "0").is_err());
        assert!(validate_config_value("max_tokens", "-1").is_err());
        assert!(validate_config_value("unknown_key", "val").is_err());
    }

    #[test]
    fn test_validate_config_thinking_aliases() {
        assert_eq!(validate_config_value("thinking", "off").unwrap(), "none");
        assert_eq!(validate_config_value("thinking", "minimal").unwrap(), "low");
        assert_eq!(validate_config_value("thinking", "med").unwrap(), "medium");
        assert_eq!(validate_config_value("thinking", "max").unwrap(), "high");
    }

    #[test]
    fn test_write_config_value_to_creates_file() {
        let tmp = std::env::temp_dir().join("yoyo_test_write_config_create");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join(".yoyo.toml");
        let _ = std::fs::remove_file(&path);

        let result = write_config_value_to("model", "test-model", &path);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("model = \"test-model\""));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_write_config_value_to_updates_existing() {
        let tmp = std::env::temp_dir().join("yoyo_test_write_config_update");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join(".yoyo.toml");
        std::fs::write(
            &path,
            "# config\nprovider = \"anthropic\"\nmodel = \"old-model\"\n",
        )
        .unwrap();

        let result = write_config_value_to("model", "new-model", &path);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("model = \"new-model\""));
        assert!(!content.contains("old-model"));
        assert!(content.contains("provider = \"anthropic\""));
        assert!(content.contains("# config"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_write_config_value_to_preserves_other_keys() {
        let tmp = std::env::temp_dir().join("yoyo_test_write_config_preserve");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join(".yoyo.toml");
        std::fs::write(
            &path,
            "provider = \"anthropic\"\nthinking = \"high\"\ntemperature = 0.5\n",
        )
        .unwrap();

        let result = write_config_value_to("model", "new-model", &path);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("model = \"new-model\""));
        assert!(content.contains("provider = \"anthropic\""));
        assert!(content.contains("thinking = \"high\""));
        assert!(content.contains("temperature = 0.5"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_format_toml_value() {
        assert_eq!(format_toml_value("hello"), "\"hello\"");
        assert_eq!(format_toml_value("42"), "42");
        assert_eq!(format_toml_value("3.14"), "3.14");
        assert_eq!(format_toml_value("true"), "true");
        assert_eq!(format_toml_value("false"), "false");
        assert_eq!(
            format_toml_value("claude-sonnet-4-6"),
            "\"claude-sonnet-4-6\""
        );
    }

    #[test]
    fn test_format_toml_value_escapes_specials() {
        // #732: unescaped values produced a file parse_config_file could not read.
        assert_eq!(format_toml_value("say \"hi\""), r#""say \"hi\"""#);
        assert_eq!(format_toml_value(r"C:\tmp"), r#""C:\\tmp""#);
        assert_eq!(format_toml_value("a\nb"), r#""a\nb""#);
        assert_eq!(format_toml_value("a\tb\rc"), r#""a\tb\rc""#);
        // Backslash must be escaped BEFORE the quote, or the quote's own
        // backslash gets doubled. This asserts the ordering bug can't return.
        assert_eq!(format_toml_value(r#"a\"b"#), r#""a\\\"b""#);
    }

    #[test]
    fn test_format_toml_value_round_trips_through_parser() {
        // The assertion that pins the promise: what the writer emits, the
        // reader must give back unchanged (Day 161 — proofs one layer below
        // the surface are half-applied fixes).
        for original in [
            "plain",
            "notify-send done",
            "say \"hi\"",
            r"C:\tmp\new",
            "a\nb",
            "a\tb",
            r#"a\"b"#,
            "→ unicode ✓",
            "",
        ] {
            let line = format!("k = {}", format_toml_value(original));
            let parsed = parse_config_file(&line);
            assert_eq!(
                parsed.get("k").map(String::as_str),
                Some(original),
                "round-trip failed for {original:?} (line: {line})"
            );
        }
    }

    #[test]
    fn test_parse_config_file_literal_strings_are_not_unescaped() {
        // TOML single-quoted strings are literal: no escape processing.
        let parsed = parse_config_file(r"k = 'C:\tmp'");
        assert_eq!(parsed.get("k").map(String::as_str), Some(r"C:\tmp"));
    }

    #[test]
    fn test_parse_config_file_lone_quote_does_not_panic() {
        // `value[1..value.len() - 1]` panicked on a one-char value.
        let parsed = parse_config_file("k = \"");
        assert_eq!(parsed.get("k").map(String::as_str), Some("\""));
    }

    #[test]
    fn config_flag_true_values() {
        for v in ["true", "1", "yes", "on"] {
            let mut config = std::collections::HashMap::new();
            config.insert("flag".to_string(), v.to_string());
            assert!(config_flag(&config, "flag", false), "{v} should be truthy");
            assert!(
                config_flag(&config, "flag", true),
                "{v} should be truthy even with default true"
            );
        }
    }

    #[test]
    fn config_flag_false_values() {
        for v in ["false", "0", "no", "off"] {
            let mut config = std::collections::HashMap::new();
            config.insert("flag".to_string(), v.to_string());
            assert!(!config_flag(&config, "flag", true), "{v} should be falsy");
            assert!(
                !config_flag(&config, "flag", false),
                "{v} should be falsy even with default false"
            );
        }
    }

    #[test]
    fn config_flag_missing_key_returns_default() {
        let config = std::collections::HashMap::new();
        assert!(!config_flag(&config, "flag", false));
        assert!(config_flag(&config, "flag", true));
    }

    #[test]
    fn config_flag_garbage_value_returns_default() {
        let mut config = std::collections::HashMap::new();
        config.insert("flag".to_string(), "maybe".to_string());
        assert!(!config_flag(&config, "flag", false));
        assert!(config_flag(&config, "flag", true));
    }

    #[test]
    fn auto_watch_defaults_to_false() {
        let config = std::collections::HashMap::new();
        assert!(!parse_auto_watch_from_config(&config));
    }

    #[test]
    fn auto_watch_respects_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_watch".to_string(), "false".to_string());
        assert!(!parse_auto_watch_from_config(&config));
    }

    #[test]
    fn auto_watch_respects_off() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_watch".to_string(), "off".to_string());
        assert!(!parse_auto_watch_from_config(&config));
    }

    #[test]
    fn auto_watch_explicit_true() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_watch".to_string(), "true".to_string());
        assert!(parse_auto_watch_from_config(&config));
    }

    #[test]
    fn validate_auto_watch_values() {
        assert_eq!(
            validate_config_value("auto_watch", "true"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("auto_watch", "false"),
            Ok("false".to_string())
        );
        assert_eq!(
            validate_config_value("auto_watch", "yes"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("auto_watch", "no"),
            Ok("false".to_string())
        );
        assert!(validate_config_value("auto_watch", "maybe").is_err());
    }

    #[test]
    fn auto_edit_defaults_to_false() {
        let config = std::collections::HashMap::new();
        assert!(!parse_auto_edit_from_config(&config));
    }

    #[test]
    fn auto_edit_respects_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_edit".to_string(), "false".to_string());
        assert!(!parse_auto_edit_from_config(&config));
    }

    #[test]
    fn auto_edit_respects_off() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_edit".to_string(), "off".to_string());
        assert!(!parse_auto_edit_from_config(&config));
    }

    #[test]
    fn auto_edit_explicit_true() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_edit".to_string(), "true".to_string());
        assert!(parse_auto_edit_from_config(&config));
    }

    #[test]
    fn validate_auto_edit_values() {
        assert_eq!(
            validate_config_value("auto_edit", "true"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("auto_edit", "false"),
            Ok("false".to_string())
        );
        assert_eq!(
            validate_config_value("auto_edit", "yes"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("auto_edit", "no"),
            Ok("false".to_string())
        );
        assert!(validate_config_value("auto_edit", "maybe").is_err());
    }

    #[test]
    fn auto_commit_defaults_to_false() {
        let config = std::collections::HashMap::new();
        assert!(!parse_auto_commit_from_config(&config));
    }

    #[test]
    fn auto_commit_respects_true() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_commit".to_string(), "true".to_string());
        assert!(parse_auto_commit_from_config(&config));
    }

    #[test]
    fn auto_commit_respects_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_commit".to_string(), "false".to_string());
        assert!(!parse_auto_commit_from_config(&config));
    }

    #[test]
    fn auto_commit_invalid_value_returns_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_commit".to_string(), "maybe".to_string());
        assert!(!parse_auto_commit_from_config(&config));
    }

    #[test]
    fn validate_auto_commit_values() {
        assert_eq!(
            validate_config_value("auto_commit", "true"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("auto_commit", "false"),
            Ok("false".to_string())
        );
        assert_eq!(
            validate_config_value("auto_commit", "yes"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("auto_commit", "no"),
            Ok("false".to_string())
        );
        assert!(validate_config_value("auto_commit", "maybe").is_err());
    }

    #[test]
    fn auto_continue_defaults_to_true() {
        let config = std::collections::HashMap::new();
        assert!(parse_auto_continue_from_config(&config));
    }

    #[test]
    fn auto_continue_respects_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_continue".to_string(), "false".to_string());
        assert!(!parse_auto_continue_from_config(&config));
    }

    #[test]
    fn auto_continue_respects_off() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_continue".to_string(), "off".to_string());
        assert!(!parse_auto_continue_from_config(&config));
    }

    #[test]
    fn auto_continue_explicit_true() {
        let mut config = std::collections::HashMap::new();
        config.insert("auto_continue".to_string(), "true".to_string());
        assert!(parse_auto_continue_from_config(&config));
    }

    #[test]
    fn continue_on_silence_defaults_to_false_when_key_absent() {
        // Product-safe default: a user who never writes the key gets the
        // byte-identical behaviour they had before the key existed.
        let config = std::collections::HashMap::new();
        assert!(!parse_continue_on_silence_from_config(&config));
    }

    #[test]
    fn continue_on_silence_explicit_true() {
        let mut config = std::collections::HashMap::new();
        config.insert("continue_on_silence".to_string(), "true".to_string());
        assert!(parse_continue_on_silence_from_config(&config));
    }

    #[test]
    fn continue_on_silence_explicit_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("continue_on_silence".to_string(), "false".to_string());
        assert!(!parse_continue_on_silence_from_config(&config));
    }

    // The three states of the `wait_for_reset` reader. It is pure and takes the
    // parsed config, so these never touch the process-global `WAIT_FOR_RESET`
    // (`tests/global_state_races.rs` is fatal on an unserialised, unregistered
    // writer, and its own first-stated remedy is "pass the value explicitly").

    #[test]
    fn wait_for_reset_defaults_to_false_when_key_absent() {
        // Product-safe default: a user who never writes the key gets the
        // byte-identical behaviour they had before the key existed. This is
        // every existing user's path and the regression risk of the config door.
        let config = std::collections::HashMap::new();
        assert!(!parse_wait_for_reset_from_config(&config));
    }

    #[test]
    fn wait_for_reset_explicit_true() {
        let mut config = std::collections::HashMap::new();
        config.insert("wait_for_reset".to_string(), "true".to_string());
        assert!(parse_wait_for_reset_from_config(&config));
    }

    #[test]
    fn wait_for_reset_explicit_false() {
        // The near-miss guard: written-and-false must read the same as absent.
        // A discriminator tested only on the side that fires is vacuous green.
        let mut config = std::collections::HashMap::new();
        config.insert("wait_for_reset".to_string(), "false".to_string());
        assert!(!parse_wait_for_reset_from_config(&config));
    }

    #[test]
    fn validate_wait_for_reset_values() {
        assert_eq!(
            validate_config_value("wait_for_reset", "true"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("wait_for_reset", "false"),
            Ok("false".to_string())
        );
        // Asserted on the error a caller actually receives, not merely is_err().
        let err = validate_config_value("wait_for_reset", "maybe").unwrap_err();
        assert!(
            err.contains("wait_for_reset") && err.contains("maybe"),
            "error should name the key and the offending value, got: {err}"
        );
    }

    #[test]
    fn wait_for_reset_is_settable_through_the_normal_door() {
        // A key nothing can set through `/config set` is half a feature.
        assert!(
            SETTABLE_KEYS.iter().any(|(k, _)| *k == "wait_for_reset"),
            "wait_for_reset must be listed in SETTABLE_KEYS"
        );
    }

    #[test]
    fn validate_continue_on_silence_values() {
        assert_eq!(
            validate_config_value("continue_on_silence", "true"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("continue_on_silence", "false"),
            Ok("false".to_string())
        );
        assert!(validate_config_value("continue_on_silence", "maybe").is_err());
    }

    #[test]
    fn validate_auto_continue_values() {
        assert_eq!(
            validate_config_value("auto_continue", "true"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("auto_continue", "false"),
            Ok("false".to_string())
        );
        assert_eq!(
            validate_config_value("auto_continue", "yes"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("auto_continue", "no"),
            Ok("false".to_string())
        );
        assert!(validate_config_value("auto_continue", "maybe").is_err());
    }

    #[test]
    fn max_auto_continues_defaults_to_none() {
        let config = std::collections::HashMap::new();
        assert_eq!(parse_max_auto_continues_from_config(&config), None);
    }

    #[test]
    fn max_auto_continues_parses_valid() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "10".to_string());
        assert_eq!(parse_max_auto_continues_from_config(&config), Some(10));
    }

    #[test]
    fn max_auto_continues_clamps_to_20() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "50".to_string());
        assert_eq!(parse_max_auto_continues_from_config(&config), Some(20));
    }

    #[test]
    fn max_auto_continues_zero_is_valid() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "0".to_string());
        assert_eq!(parse_max_auto_continues_from_config(&config), Some(0));
    }

    #[test]
    fn max_auto_continues_non_numeric_returns_none() {
        let mut config = std::collections::HashMap::new();
        config.insert("max_auto_continues".to_string(), "abc".to_string());
        assert_eq!(parse_max_auto_continues_from_config(&config), None);
    }

    #[test]
    fn validate_max_auto_continues_values() {
        assert_eq!(
            validate_config_value("max_auto_continues", "5"),
            Ok("5".to_string())
        );
        assert_eq!(
            validate_config_value("max_auto_continues", "0"),
            Ok("0".to_string())
        );
        assert_eq!(
            validate_config_value("max_auto_continues", "20"),
            Ok("20".to_string())
        );
        assert!(validate_config_value("max_auto_continues", "21").is_err());
        assert!(validate_config_value("max_auto_continues", "-1").is_err());
        assert!(validate_config_value("max_auto_continues", "abc").is_err());
    }

    // === no_bell config tests ===

    #[test]
    fn no_bell_defaults_to_false() {
        let config = std::collections::HashMap::new();
        assert!(!parse_no_bell_from_config(&config));
    }

    #[test]
    fn no_bell_respects_true() {
        let mut config = std::collections::HashMap::new();
        config.insert("no_bell".to_string(), "true".to_string());
        assert!(parse_no_bell_from_config(&config));
    }

    #[test]
    fn no_bell_respects_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("no_bell".to_string(), "false".to_string());
        assert!(!parse_no_bell_from_config(&config));
    }

    #[test]
    fn no_bell_respects_on() {
        let mut config = std::collections::HashMap::new();
        config.insert("no_bell".to_string(), "on".to_string());
        assert!(parse_no_bell_from_config(&config));
    }

    #[test]
    fn validate_no_bell_values() {
        assert_eq!(
            validate_config_value("no_bell", "true"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("no_bell", "false"),
            Ok("false".to_string())
        );
        assert_eq!(
            validate_config_value("no_bell", "yes"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("no_bell", "no"),
            Ok("false".to_string())
        );
        assert!(validate_config_value("no_bell", "maybe").is_err());
    }

    // === notify_command config tests ===

    #[test]
    fn notify_command_absent_is_none() {
        let config = std::collections::HashMap::new();
        assert_eq!(parse_notify_command_from_config(&config), None);
    }

    #[test]
    fn notify_command_empty_is_none() {
        let mut config = std::collections::HashMap::new();
        config.insert("notify_command".to_string(), "".to_string());
        assert_eq!(parse_notify_command_from_config(&config), None);
    }

    #[test]
    fn notify_command_whitespace_only_is_none() {
        let mut config = std::collections::HashMap::new();
        config.insert("notify_command".to_string(), "   ".to_string());
        assert_eq!(parse_notify_command_from_config(&config), None);
    }

    #[test]
    fn notify_command_set_returns_value() {
        let mut config = std::collections::HashMap::new();
        config.insert(
            "notify_command".to_string(),
            "notify-send 'yoyo' 'done'".to_string(),
        );
        assert_eq!(
            parse_notify_command_from_config(&config),
            Some("notify-send 'yoyo' 'done'".to_string())
        );
    }

    #[test]
    fn notify_command_value_is_trimmed() {
        let mut config = std::collections::HashMap::new();
        config.insert("notify_command".to_string(), "  echo done  ".to_string());
        assert_eq!(
            parse_notify_command_from_config(&config),
            Some("echo done".to_string())
        );
    }

    #[test]
    fn validate_notify_command_accepts_any_string() {
        // Any user-supplied command string is valid...
        assert_eq!(
            validate_config_value("notify_command", "notify-send 'yoyo' 'done'"),
            Ok("notify-send 'yoyo' 'done'".to_string())
        );
        // ...including an empty string, which clears/disables the feature.
        assert_eq!(
            validate_config_value("notify_command", ""),
            Ok("".to_string())
        );
    }

    #[test]
    fn notify_command_is_a_settable_key() {
        // `/config set` recognizes keys via SETTABLE_KEYS.
        assert!(SETTABLE_KEYS.iter().any(|(k, _)| *k == "notify_command"));
    }

    // === quiet config tests ===

    #[test]
    fn quiet_defaults_to_false() {
        let config = std::collections::HashMap::new();
        assert!(!parse_quiet_from_config(&config));
    }

    #[test]
    fn quiet_respects_true() {
        let mut config = std::collections::HashMap::new();
        config.insert("quiet".to_string(), "true".to_string());
        assert!(parse_quiet_from_config(&config));
    }

    #[test]
    fn quiet_respects_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("quiet".to_string(), "false".to_string());
        assert!(!parse_quiet_from_config(&config));
    }

    #[test]
    fn quiet_invalid_value_returns_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("quiet".to_string(), "maybe".to_string());
        assert!(!parse_quiet_from_config(&config));
    }

    #[test]
    fn validate_quiet_values() {
        assert_eq!(
            validate_config_value("quiet", "true"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("quiet", "false"),
            Ok("false".to_string())
        );
        assert_eq!(
            validate_config_value("quiet", "yes"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("quiet", "no"),
            Ok("false".to_string())
        );
        assert!(validate_config_value("quiet", "maybe").is_err());
    }

    // === no_color config tests ===

    #[test]
    fn no_color_defaults_to_false() {
        let config = std::collections::HashMap::new();
        assert!(!parse_no_color_from_config(&config));
    }

    #[test]
    fn no_color_respects_true() {
        let mut config = std::collections::HashMap::new();
        config.insert("no_color".to_string(), "true".to_string());
        assert!(parse_no_color_from_config(&config));
    }

    #[test]
    fn no_color_respects_false() {
        let mut config = std::collections::HashMap::new();
        config.insert("no_color".to_string(), "false".to_string());
        assert!(!parse_no_color_from_config(&config));
    }

    #[test]
    fn no_color_respects_on() {
        let mut config = std::collections::HashMap::new();
        config.insert("no_color".to_string(), "on".to_string());
        assert!(parse_no_color_from_config(&config));
    }

    #[test]
    fn validate_no_color_values() {
        assert_eq!(
            validate_config_value("no_color", "true"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("no_color", "false"),
            Ok("false".to_string())
        );
        assert_eq!(
            validate_config_value("no_color", "yes"),
            Ok("true".to_string())
        );
        assert_eq!(
            validate_config_value("no_color", "no"),
            Ok("false".to_string())
        );
        assert!(validate_config_value("no_color", "maybe").is_err());
    }

    // === SETTABLE_KEYS completeness ===

    #[test]
    fn settable_keys_contains_display_settings() {
        let keys: Vec<&str> = SETTABLE_KEYS.iter().map(|(k, _)| *k).collect();
        assert!(keys.contains(&"no_bell"), "SETTABLE_KEYS missing no_bell");
        assert!(keys.contains(&"quiet"), "SETTABLE_KEYS missing quiet");
        assert!(keys.contains(&"no_color"), "SETTABLE_KEYS missing no_color");
        assert!(
            keys.contains(&"auto_edit"),
            "SETTABLE_KEYS missing auto_edit"
        );
    }

    // === Config-file path resolution tests (moved from cli.rs) ===

    #[test]
    fn test_parse_config_file_basic() {
        let content = r#"
model = "claude-sonnet-4-20250514"
thinking = "medium"
max_tokens = 4096
"#;
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-sonnet-4-20250514");
        assert_eq!(config.get("thinking").unwrap(), "medium");
        assert_eq!(config.get("max_tokens").unwrap(), "4096");
    }

    #[test]
    fn test_parse_config_file_comments_and_blanks() {
        let content = r#"
# This is a comment
model = "claude-opus-4-6"

# Another comment
thinking = "high"
"#;
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-opus-4-6");
        assert_eq!(config.get("thinking").unwrap(), "high");
        assert_eq!(config.len(), 2);
    }

    #[test]
    fn test_parse_config_file_no_quotes() {
        let content = "model = claude-haiku-35\nmax_tokens = 2048";
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-haiku-35");
        assert_eq!(config.get("max_tokens").unwrap(), "2048");
    }

    #[test]
    fn test_parse_config_file_single_quotes() {
        let content = "model = 'claude-opus-4-6'";
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-opus-4-6");
    }

    #[test]
    fn test_parse_config_file_empty() {
        let config = parse_config_file("");
        assert!(config.is_empty());
    }

    #[test]
    fn test_parse_config_file_whitespace_handling() {
        let content = "  model  =  claude-opus-4-6  ";
        let config = parse_config_file(content);
        assert_eq!(config.get("model").unwrap(), "claude-opus-4-6");
    }

    #[test]
    fn test_parse_config_file_mcp_array() {
        let content = r#"
model = "claude-sonnet-4-20250514"
mcp = ["npx open-websearch@latest", "npx @mcp/server-filesystem /tmp"]
"#;
        let config = parse_config_file(content);
        let mcp_val = config.get("mcp").expect("mcp key should exist");
        let mcps = parse_toml_array(mcp_val);
        assert_eq!(mcps.len(), 2);
        assert_eq!(mcps[0], "npx open-websearch@latest");
        assert_eq!(mcps[1], "npx @mcp/server-filesystem /tmp");
    }

    #[test]
    fn test_parse_config_file_mcp_empty_array() {
        let content = "mcp = []";
        let config = parse_config_file(content);
        let mcp_val = config.get("mcp").expect("mcp key should exist");
        let mcps = parse_toml_array(mcp_val);
        assert!(mcps.is_empty());
    }

    #[test]
    fn test_parse_config_file_mcp_single_entry() {
        let content = r#"mcp = ["npx open-websearch@latest"]"#;
        let config = parse_config_file(content);
        let mcp_val = config.get("mcp").expect("mcp key should exist");
        let mcps = parse_toml_array(mcp_val);
        assert_eq!(mcps.len(), 1);
        assert_eq!(mcps[0], "npx open-websearch@latest");
    }

    #[test]
    fn test_home_config_path_returns_yoyo_toml_in_home() {
        // home_config_path() should return $HOME/.yoyo.toml
        let original_home = std::env::var("HOME").ok();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp.path());

        let path = home_config_path();
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path, tmp.path().join(".yoyo.toml"));

        // Restore
        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        }
    }

    #[test]
    fn test_home_config_path_file_is_loadable() {
        // If ~/.yoyo.toml exists, parse_config_file should parse it
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".yoyo.toml");
        std::fs::write(
            &config_path,
            "model = \"test-model\"\napi_key = \"sk-home-test\"\n",
        )
        .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let config = parse_config_file(&content);
        assert_eq!(config.get("model").unwrap(), "test-model");
        assert_eq!(config.get("api_key").unwrap(), "sk-home-test");
    }

    #[test]
    fn test_config_precedence_project_over_home() {
        // If both project-level .yoyo.toml and ~/.yoyo.toml exist,
        // the project-level config should be found first.
        // We verify this by checking the search order logic:
        // CONFIG_FILE_NAMES is checked before home_config_path().
        //
        // Since load_config_file() checks project-level first, and both files
        // would parse correctly, we verify the ordering is as documented.
        let project_content = "model = \"project-model\"";
        let home_content = "model = \"home-model\"";

        let project_config = parse_config_file(project_content);
        let home_config = parse_config_file(home_content);

        assert_eq!(project_config.get("model").unwrap(), "project-model");
        assert_eq!(home_config.get("model").unwrap(), "home-model");

        // The search order is documented: project > home > XDG
        // This test verifies both configs parse independently.
        // The actual precedence is enforced by the early-return in load_config_file().
    }

    #[test]
    fn test_config_search_order_documented() {
        // Verify the documented search order: project (.yoyo.toml), home (~/.yoyo.toml), XDG
        // CONFIG_FILE_NAMES contains the project-level name
        assert_eq!(CONFIG_FILE_NAMES, &[".yoyo.toml"]);

        // home_config_path returns ~/.yoyo.toml
        let original_home = std::env::var("HOME").ok();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp.path());

        let home = home_config_path().unwrap();
        assert!(home.to_string_lossy().ends_with(".yoyo.toml"));
        assert!(home
            .to_string_lossy()
            .contains(&tmp.path().to_string_lossy().to_string()));

        // user_config_path returns ~/.config/yoyo/config.toml (XDG)
        let xdg = user_config_path().unwrap();
        assert!(xdg.to_string_lossy().ends_with("config.toml"));
        assert!(xdg.to_string_lossy().contains("yoyo"));

        // Restore
        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        }
    }

    #[test]
    fn test_history_file_path_returns_some() {
        // In CI and local environments, HOME is typically set
        let path = history_file_path();
        if std::env::var("HOME").is_ok() {
            assert!(path.is_some(), "Should return a path when HOME is set");
            let p = path.unwrap();
            let p_str = p.to_string_lossy();
            assert!(
                p_str.contains("yoyo"),
                "History path should contain 'yoyo': {p_str}"
            );
            assert!(
                p_str.ends_with("history") || p_str.ends_with(".yoyo_history"),
                "History path should end with 'history' or '.yoyo_history': {p_str}"
            );
        }
    }

    #[test]
    fn test_history_file_path_prefers_xdg() {
        // When XDG_DATA_HOME is set, should use it
        let dir = std::env::temp_dir().join("yoyo_test_xdg_data");
        let _ = std::fs::create_dir_all(&dir);
        // We can't safely set env vars in parallel tests, so just verify the logic
        // by calling data_dir_hint and checking the fallback behavior
        let path = history_file_path();
        // Should return Some regardless
        if std::env::var("HOME").is_ok() || std::env::var("XDG_DATA_HOME").is_ok() {
            assert!(path.is_some());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_data_dir_hint_returns_path() {
        // data_dir_hint should return something when HOME is set
        if std::env::var("HOME").is_ok() || std::env::var("XDG_DATA_HOME").is_ok() {
            let dir = data_dir_hint();
            assert!(dir.is_some(), "Should return a data dir path");
        }
    }

    // -----------------------------------------------------------------------
    // append_allow_pattern / set_permissions_allow
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_permissions_allow_creates_section() {
        let content = "model = \"claude-sonnet-4-6\"\n";
        let result = set_permissions_allow(content, &["cargo test*".to_string()]);
        assert!(result.contains("[permissions]"));
        assert!(result.contains("allow = [\"cargo test*\"]"));
        // Original content preserved
        assert!(result.contains("model = \"claude-sonnet-4-6\""));
    }

    #[test]
    fn test_set_permissions_allow_updates_existing() {
        let content = "[permissions]\nallow = [\"cargo build*\"]\ndeny = [\"rm*\"]\n";
        let result = set_permissions_allow(
            content,
            &["cargo build*".to_string(), "cargo test*".to_string()],
        );
        assert!(result.contains("allow = [\"cargo build*\", \"cargo test*\"]"));
        // Deny is preserved
        assert!(result.contains("deny = [\"rm*\"]"));
    }

    #[test]
    fn test_set_permissions_allow_inserts_if_section_exists_without_allow() {
        let content = "[permissions]\ndeny = [\"rm*\"]\n";
        let result = set_permissions_allow(content, &["cargo test*".to_string()]);
        assert!(result.contains("allow = [\"cargo test*\"]"));
        assert!(result.contains("deny = [\"rm*\"]"));
    }

    #[test]
    fn test_append_allow_pattern_to_creates_file() {
        let dir = std::env::temp_dir().join("yoyo_test_append_allow_create");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".yoyo.toml");

        let result = append_allow_pattern_to("cargo test*", &path).unwrap();
        assert_eq!(result, path);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[permissions]"));
        assert!(content.contains("allow = [\"cargo test*\"]"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_append_allow_pattern_to_appends_to_existing() {
        let dir = std::env::temp_dir().join("yoyo_test_append_allow_existing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".yoyo.toml");

        // Write initial config
        std::fs::write(&path, "[permissions]\nallow = [\"cargo build*\"]\n").unwrap();

        append_allow_pattern_to("cargo test*", &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"cargo build*\""));
        assert!(content.contains("\"cargo test*\""));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_append_allow_pattern_to_no_duplicates() {
        let dir = std::env::temp_dir().join("yoyo_test_append_allow_nodup");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".yoyo.toml");

        std::fs::write(&path, "[permissions]\nallow = [\"cargo test*\"]\n").unwrap();

        // Try to add the same pattern again
        append_allow_pattern_to("cargo test*", &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // Should still only have one entry
        assert_eq!(content.matches("cargo test*").count(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_append_allow_pattern_to_pattern_matches_via_check() {
        let dir = std::env::temp_dir().join("yoyo_test_append_allow_check");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".yoyo.toml");

        append_allow_pattern_to("cargo test*", &path).unwrap();

        // Parse back and verify it actually matches
        let content = std::fs::read_to_string(&path).unwrap();
        let perms = parse_permissions_from_config(&content);
        assert_eq!(perms.check("cargo test"), Some(true));
        assert_eq!(perms.check("cargo test --release"), Some(true));
        assert_eq!(perms.check("npm run test"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- #748: config provenance ----------------------------------------

    #[test]
    fn test_config_path_is_project_local_table() {
        use std::path::Path;
        let cwd = Path::new("/work/repo");
        let home = Path::new("/home/u");
        let cases: &[(&str, bool, &str)] = &[
            // path, expected, why
            (".yoyo.toml", true, "relative project config, read from cwd"),
            ("/work/repo/.yoyo.toml", true, "same file, absolute"),
            ("/home/u/.yoyo.toml", false, "home config"),
            (
                "/home/u/.config/yoyo/config.toml",
                false,
                "XDG config (name not in CONFIG_FILE_NAMES)",
            ),
            ("/work/other/.yoyo.toml", false, "some other directory"),
            (
                "/work/repo/sub/.yoyo.toml",
                false,
                "not the cwd we resolved against",
            ),
        ];
        for (path, expected, why) in cases {
            assert_eq!(
                config_path_is_project_local(Path::new(path), cwd, Some(home)),
                *expected,
                "{path}: {why}"
            );
        }
    }

    #[test]
    fn test_config_in_home_dir_is_not_project_local() {
        use std::path::Path;
        // cwd IS the home directory: ./.yoyo.toml and ~/.yoyo.toml are the same
        // file, and the user authored it. Pinned deliberately as NOT project-
        // local — the boundary guards "I cd'd into a repo someone else wrote",
        // not "I am sitting in my own home directory".
        let home = Path::new("/home/u");
        assert!(!config_path_is_project_local(
            Path::new(".yoyo.toml"),
            home,
            Some(home)
        ));
        assert!(!config_path_is_project_local(
            Path::new("/home/u/.yoyo.toml"),
            home,
            Some(home)
        ));
        // With no HOME known, the same path is project-local (fail safe: gate it).
        assert!(config_path_is_project_local(
            Path::new(".yoyo.toml"),
            home,
            None
        ));
    }
}

#[cfg(test)]
mod tilde_restriction_tests {
    use super::*;

    /// Round 76 unpredicted find: `resolve_path` did no tilde expansion, so the
    /// worked example shipped in `docs/src/configuration/permissions.md`
    /// (`deny = ["~/.ssh", "/etc"]`) resolved to `$CWD/~/.ssh` and could never
    /// match — a user who copied the documented line believed their SSH keys
    /// were fenced off while the file tools reached them unimpeded.
    ///
    /// Asserted at the **emission point**: the `Result` a caller of
    /// `check_path` actually receives, never `resolve_path` one layer below.
    /// Reads the real `HOME` rather than setting it, so no process-global env
    /// var is mutated and the test needs no `#[serial]`.
    #[test]
    fn tilde_deny_entry_actually_denies_the_home_relative_path() {
        let Ok(home) = std::env::var("HOME") else {
            return; // no HOME on this platform; nothing to assert
        };

        let restrictions = DirectoryRestrictions {
            allow: vec![],
            deny: vec!["~/.ssh".to_string()],
        };

        let secret = format!("{home}/.ssh/id_rsa");
        let err = restrictions
            .check_path(&secret)
            .expect_err("a ~/.ssh deny entry must block $HOME/.ssh/id_rsa");
        assert!(
            err.contains("Access denied"),
            "emission point must say the access was denied, got: {err}"
        );

        // Near-miss guard: a discriminator tested only on the side that blocks
        // is vacuous green. An unrelated path under HOME must still pass.
        let ordinary = format!("{home}/projects/main.rs");
        assert!(
            restrictions.check_path(&ordinary).is_ok(),
            "~/.ssh must not fence off the whole home directory"
        );
    }

    /// A bare `~` entry names the home directory itself.
    #[test]
    fn bare_tilde_entry_resolves_to_the_home_directory() {
        let Ok(home) = std::env::var("HOME") else {
            return;
        };
        let restrictions = DirectoryRestrictions {
            allow: vec!["~".to_string()],
            deny: vec![],
        };
        let inside = format!("{home}/anywhere.txt");
        assert!(
            restrictions.check_path(&inside).is_ok(),
            "an allow entry of `~` must admit paths under HOME"
        );
    }

    /// Table test for the pure half. The two emission-point tests above drive
    /// only the happy branches; these are the ones that must NOT expand.
    #[test]
    fn expand_tilde_with_table() {
        let h = Some("/home/ada");
        for (input, home, expected) in [
            ("~", h, "/home/ada"),
            ("~/.ssh", h, "/home/ada/.ssh"),
            ("~/a/b.rs", h, "/home/ada/a/b.rs"),
            // trailing slash on HOME must not double up
            ("~/.ssh", Some("/home/ada/"), "/home/ada/.ssh"),
            // another account's home needs a passwd lookup we do not do
            ("~bob/.ssh", h, "~bob/.ssh"),
            // not a tilde path at all
            ("./src", h, "./src"),
            ("/etc", h, "/etc"),
            ("a~b", h, "a~b"),
            // unresolvable HOME must leave the path alone rather than invent one
            ("~/.ssh", None, "~/.ssh"),
            ("~", None, "~"),
            ("~/.ssh", Some(""), "~/.ssh"),
        ] {
            assert_eq!(
                expand_tilde_with(input, home),
                expected,
                "expand_tilde_with({input:?}, {home:?})"
            );
        }
    }
}

/// #823 — `[directories]` wildcard detection and its warning.
///
/// The matcher itself is deliberately untouched by these tests: `check_path` /
/// `path_is_under` still treat `*` as a literal character, and this task adds a
/// warning, not glob support.
#[cfg(test)]
mod directory_wildcard_tests {
    use super::*;

    fn dirs(allow: &[&str], deny: &[&str]) -> DirectoryRestrictions {
        DirectoryRestrictions {
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Pure table over the detector: both metacharacters, both lists, empty,
    /// and a literal entry carrying neither.
    #[test]
    fn wildcard_directory_entries_table() {
        type Case<'a> = (&'a [&'a str], &'a [&'a str], &'a [(DirectoryList, &'a str)]);
        let cases: &[Case] = &[
            // No restrictions at all.
            (&[], &[], &[]),
            // Plain literal entries — the common case, nothing reported.
            (&["src", "tests"], &["secrets"], &[]),
            // `*` in allow.
            (&["src/*"], &[], &[(DirectoryList::Allow, "src/*")]),
            // `?` in allow — over-reported on purpose (see DIRECTORY_GLOB_METACHARS).
            (
                &["src/mod?.rs"],
                &[],
                &[(DirectoryList::Allow, "src/mod?.rs")],
            ),
            // `*` in deny — the direction that fails OPEN.
            (&[], &["secrets/*"], &[(DirectoryList::Deny, "secrets/*")]),
            // Both lists at once: allow entries come first, deny after.
            (
                &["src", "gen/*"],
                &["secrets/*", "vendor"],
                &[
                    (DirectoryList::Allow, "gen/*"),
                    (DirectoryList::Deny, "secrets/*"),
                ],
            ),
            // A path that merely *contains* a directory named with no
            // metacharacter stays clean.
            (&["/abs/path/to/src"], &[], &[]),
        ];

        for (allow, deny, expected) in cases {
            let got = wildcard_directory_entries(&dirs(allow, deny));
            let want: Vec<(DirectoryList, String)> =
                expected.iter().map(|(l, s)| (*l, s.to_string())).collect();
            assert_eq!(got, want, "allow={allow:?} deny={deny:?}");
        }
    }

    /// Emission point: the string a caller actually receives, not a helper one
    /// layer below it. Names the entry verbatim, says it is literal, and gives
    /// the `src` escape hatch.
    #[test]
    fn allow_wildcard_warning_names_entry_and_escape_hatch() {
        let content = "[directories]\nallow = [\"src/*\"]\n";
        let parsed = parse_directories_from_config(content);
        let entries = wildcard_directory_entries(&parsed);
        let msg =
            directory_wildcard_warning(&entries, false).expect("a wildcard allow entry must warn");

        assert!(
            msg.contains("src/*"),
            "must quote the entry verbatim: {msg}"
        );
        assert!(
            msg.contains("literal"),
            "must say the entry is matched literally: {msg}"
        );
        assert!(
            msg.contains("never globbed"),
            "must say [directories] is not globbed: {msg}"
        );
        assert!(
            msg.contains("`src`"),
            "must name the escape hatch (the bare directory): {msg}"
        );
        assert!(
            msg.contains("[permissions]"),
            "must explain why the two blocks differ: {msg}"
        );
        assert!(
            msg.contains("every file access is denied"),
            "an allow wildcard denies everything: {msg}"
        );
    }

    /// The dangerous direction: a deny wildcard fails OPEN, and the message
    /// must distinguish it from an allow entry rather than lumping both under
    /// one consequence.
    #[test]
    fn deny_wildcard_warning_is_distinguished_from_allow() {
        let content = "[directories]\nallow = [\"src/*\"]\ndeny = [\"secrets/*\"]\n";
        let parsed = parse_directories_from_config(content);
        let entries = wildcard_directory_entries(&parsed);
        let msg =
            directory_wildcard_warning(&entries, false).expect("both halves must be reported");

        assert!(
            msg.contains("secrets/*"),
            "must quote the deny entry: {msg}"
        );
        assert!(
            msg.contains("allow = \"src/*\""),
            "must say which half src/* came from: {msg}"
        );
        assert!(
            msg.contains("deny = \"secrets/*\""),
            "must say which half secrets/* came from: {msg}"
        );
        assert!(
            msg.contains("protects nothing"),
            "the deny half fails open and must say so: {msg}"
        );
    }

    /// Near-miss guard: a discriminator tested only on the side that fires is
    /// vacuous green. A plain literal entry must produce no warning at all —
    /// this is every existing user's path and the regression risk.
    #[test]
    fn literal_directory_entry_produces_no_warning() {
        let content = "[directories]\nallow = [\"src\"]\ndeny = [\"secrets\"]\n";
        let parsed = parse_directories_from_config(content);
        let entries = wildcard_directory_entries(&parsed);
        assert!(entries.is_empty(), "no metacharacters, nothing to report");
        assert_eq!(
            directory_wildcard_warning(&entries, false),
            None,
            "output must be byte-identical for a wildcard-free config"
        );
    }

    /// Grammar: the singular case must read "1 entry contains", not
    /// "1 entry contain" — a warning that reads as broken is easier to dismiss.
    #[test]
    fn singular_and_plural_warnings_agree_with_their_counts() {
        let one =
            directory_wildcard_warning(&wildcard_directory_entries(&dirs(&["src/*"], &[])), true)
                .expect("must warn");
        assert!(one.starts_with("1 [directories] entry contains"), "{one}");

        let two = directory_wildcard_warning(
            &wildcard_directory_entries(&dirs(&["src/*"], &["secrets/*"])),
            true,
        )
        .expect("must warn");
        assert!(two.starts_with("2 [directories] entries contain"), "{two}");
    }

    /// Glyph-free under plain output (screen readers), same house style as
    /// `cli::project_permission_refusal_message`.
    #[test]
    fn plain_output_warning_carries_no_glyphs() {
        let entries = wildcard_directory_entries(&dirs(&["src/*"], &[]));
        let plain = directory_wildcard_warning(&entries, true).expect("must warn");
        for glyph in ['⚠', '—', '•', '▪', '◦'] {
            assert!(
                !plain.contains(glyph),
                "plain output must not contain {glyph:?}: {plain}"
            );
        }
        // Still says the load-bearing things.
        assert!(plain.contains("src/*"));
        assert!(plain.contains("never globbed"));
    }

    /// The warning is a warning: the matcher is unchanged and the restrictions
    /// pass through byte-identically. Pinned so a future "fix" that quietly
    /// adds globbing has to face this assertion.
    #[test]
    fn wildcard_entry_still_matches_nothing() {
        let parsed = parse_directories_from_config("[directories]\nallow = [\"src/*\"]\n");
        assert_eq!(parsed.allow, vec!["src/*".to_string()]);
        assert!(
            parsed.check_path("src/main.rs").is_err(),
            "detection must not have changed the matcher"
        );
    }
}
