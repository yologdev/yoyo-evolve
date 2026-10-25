//! Dev workflow command handlers: /doctor, /health, /fix.

use crate::cli;
use crate::commands_project::{detect_project_type, ProjectType};
use crate::commands_run::get_last_failed_run;
use crate::commands_session::auto_compact_if_needed;
use crate::format::*;
use crate::git::{git_branch, run_git};
use crate::prompt::run_prompt;

use yoagent::agent::Agent;
use yoagent::*;

// ── /doctor ──────────────────────────────────────────────────────────────

/// Status of a single doctor check.
#[derive(Debug, Clone, PartialEq)]
pub enum DoctorStatus {
    Pass,
    Fail,
    Warn,
}

/// Where a doctor report is printed, so each row's advice is runnable there
/// (#1075): a shell `yoyo doctor` reader cannot type `/init`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DoctorSurface {
    Repl,
    Shell,
}

/// A single diagnostic check result from `/doctor`.
#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorStatus,
    pub detail: String,
}

/// Run all environment diagnostic checks and return structured results.
///
/// This is separated from the display logic so it can be tested.
/// `restrictions` is the session's startup directory fence: the project
/// context row lists only files the loader would admit (#1043).
pub fn run_doctor_checks(
    provider: &str,
    model: &str,
    restrictions: &crate::config::DirectoryRestrictions,
    surface: DoctorSurface,
) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    // 1. Version
    checks.push(DoctorCheck {
        name: "Version".to_string(),
        status: DoctorStatus::Pass,
        detail: cli::VERSION.to_string(),
    });

    // 2. Git installed
    match run_git(&["--version"]) {
        Ok(ver_output) => {
            let ver = ver_output.replace("git version ", "");
            checks.push(DoctorCheck {
                name: "Git".to_string(),
                status: DoctorStatus::Pass,
                detail: format!("installed ({ver})"),
            });
        }
        _ => {
            checks.push(DoctorCheck {
                name: "Git".to_string(),
                status: DoctorStatus::Fail,
                detail: "not found".to_string(),
            });
        }
    }

    // 3. Git repo
    match run_git(&["rev-parse", "--is-inside-work-tree"]) {
        Ok(_) => {
            let branch = git_branch().unwrap_or_else(|| "detached".to_string());
            checks.push(DoctorCheck {
                name: "Git repo".to_string(),
                status: DoctorStatus::Pass,
                detail: format!("yes (branch: {branch})"),
            });
        }
        _ => {
            checks.push(DoctorCheck {
                name: "Git repo".to_string(),
                status: DoctorStatus::Warn,
                detail: "not inside a git repository".to_string(),
            });
        }
    }

    // 4. Provider
    checks.push(DoctorCheck {
        name: "Provider".to_string(),
        status: DoctorStatus::Pass,
        detail: provider.to_string(),
    });

    // 5. API key
    let env_var = cli::provider_api_key_env(provider);
    match env_var {
        Some(var_name) => {
            if std::env::var(var_name).is_ok() {
                checks.push(DoctorCheck {
                    name: "API key".to_string(),
                    status: DoctorStatus::Pass,
                    detail: format!("set ({var_name})"),
                });
            } else {
                checks.push(DoctorCheck {
                    name: "API key".to_string(),
                    status: DoctorStatus::Fail,
                    detail: format!("{var_name} not set"),
                });
            }
        }
        None => {
            // Unknown provider — can't check env var
            if provider == "ollama" {
                checks.push(DoctorCheck {
                    name: "API key".to_string(),
                    status: DoctorStatus::Pass,
                    detail: "not required (ollama)".to_string(),
                });
            } else {
                checks.push(DoctorCheck {
                    name: "API key".to_string(),
                    status: DoctorStatus::Warn,
                    detail: format!("unknown env var for provider '{provider}'"),
                });
            }
        }
    }

    // 6. Model
    checks.push(DoctorCheck {
        name: "Model".to_string(),
        status: DoctorStatus::Pass,
        detail: model.to_string(),
    });

    // 7. Config file
    let mut config_found = Vec::new();
    if std::path::Path::new(".yoyo.toml").exists() {
        config_found.push(".yoyo.toml");
    }
    if let Some(user_path) = cli::user_config_path() {
        if user_path.exists() {
            config_found.push("~/.config/yoyo/config.toml");
        }
    }
    if config_found.is_empty() {
        checks.push(DoctorCheck {
            name: "Config file".to_string(),
            status: DoctorStatus::Warn,
            detail: "none found (.yoyo.toml or ~/.config/yoyo/config.toml)".to_string(),
        });
    } else {
        checks.push(DoctorCheck {
            name: "Config file".to_string(),
            status: DoctorStatus::Pass,
            detail: format!("found: {}", config_found.join(", ")),
        });
    }

    // 8. Project context
    let (context_files, _) = cli::list_project_context_files(restrictions);
    if context_files.is_empty() {
        checks.push(DoctorCheck {
            name: "Project context".to_string(),
            status: DoctorStatus::Warn,
            detail: no_context_file_detail(surface).to_string(),
        });
    } else {
        let descriptions: Vec<String> = context_files
            .iter()
            .map(|(name, lines)| format!("{name} ({lines} lines)"))
            .collect();
        checks.push(DoctorCheck {
            name: "Project context".to_string(),
            status: DoctorStatus::Pass,
            detail: descriptions.join(", "),
        });
    }

    // 9. Curl
    match std::process::Command::new("curl").arg("--version").output() {
        Ok(output) if output.status.success() => {
            checks.push(DoctorCheck {
                name: "Curl".to_string(),
                status: DoctorStatus::Pass,
                detail: "installed (for /docs and /web)".to_string(),
            });
        }
        _ => {
            checks.push(DoctorCheck {
                name: "Curl".to_string(),
                status: DoctorStatus::Warn,
                detail: "not found (/docs and /web won't work)".to_string(),
            });
        }
    }

    // 10. Memory dir (.yoyo/)
    if std::path::Path::new(".yoyo").is_dir() {
        checks.push(DoctorCheck {
            name: "Memory dir".to_string(),
            status: DoctorStatus::Pass,
            detail: ".yoyo/ found".to_string(),
        });
    } else {
        checks.push(DoctorCheck {
            name: "Memory dir".to_string(),
            status: DoctorStatus::Warn,
            detail: memory_dir_missing_detail(surface).to_string(),
        });
    }

    // 11. RTK (Rust Token Killer) — optional tool output compression
    {
        let rtk_available = crate::rtk::detect_rtk();
        let rtk_disabled = crate::rtk::is_rtk_disabled();
        if rtk_available && !rtk_disabled {
            checks.push(DoctorCheck {
                name: "RTK".to_string(),
                status: DoctorStatus::Pass,
                detail: "installed (auto-compressing tool output)".to_string(),
            });
        } else if rtk_available && rtk_disabled {
            checks.push(DoctorCheck {
                name: "RTK".to_string(),
                status: DoctorStatus::Warn,
                detail: "installed but disabled (--no-rtk flag)".to_string(),
            });
        } else {
            checks.push(DoctorCheck {
                name: "RTK".to_string(),
                status: DoctorStatus::Pass,
                detail: "not installed (optional — compresses build output)".to_string(),
            });
        }
    }

    // 12. Project-type toolchain checks
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    let toolchain = toolchain_checks_for_project(&project_type);
    checks.extend(toolchain);

    // 13. Skill context cost — reports the recurring context spend of loaded
    //     skills (SKILL.md bytes → rough tokens). Honest naming: this reports
    //     COST, not "unused" detection (we have no usage telemetry here).
    let skill_bytes = discovered_skill_bytes();
    let skill_tokens = skill_bytes_to_tokens(skill_bytes);
    let (status, detail) = skill_context_cost_status(skill_tokens);
    checks.push(DoctorCheck {
        name: "Skill context cost".to_string(),
        status,
        detail,
    });

    checks
}

/// Project-context row text when no context file exists (#1075): each surface
/// names the `init` it can actually run.
fn no_context_file_detail(surface: DoctorSurface) -> &'static str {
    match surface {
        DoctorSurface::Repl => "no context file (create YOYO.md or run /init)",
        DoctorSurface::Shell => "no context file (create YOYO.md or run yoyo init)",
    }
}

/// Memory-dir row text when `.yoyo/` is missing (#1075). Both surfaces name
/// `mkdir -p .yoyo`, the command `doctor_fix_action` offers below the report.
/// The REPL also has `/remember <note>`, which creates `.yoyo/` via
/// `save_memories_to`'s `create_dir_all`; a bare `/remember` only prints
/// usage, and the shell has no `remember` verb at all.
fn memory_dir_missing_detail(surface: DoctorSurface) -> &'static str {
    match surface {
        DoctorSurface::Repl => ".yoyo/ not found (mkdir -p .yoyo to create, or /remember <note>)",
        DoctorSurface::Shell => ".yoyo/ not found (mkdir -p .yoyo to create)",
    }
}

/// Conservative threshold (in estimated tokens) above which loaded skills'
/// combined context cost is worth reviewing. Skills are injected into the
/// system prompt every turn, so their bytes are recurring context spend —
/// this is a rough heuristic, not a hard limit.
const SKILL_CONTEXT_COST_WARN_TOKENS: usize = 8000;

/// Rough token estimate from a byte count (bytes / 4, rounded up).
/// Kept as a tiny pure fn so the estimation is independently testable.
/// Documented as a heuristic — real tokenization varies by tokenizer.
pub fn skill_bytes_to_tokens(bytes: usize) -> usize {
    bytes.div_ceil(4)
}

/// Decide the `/doctor` status + message for the combined context cost of
/// loaded skills, given the total estimated token cost.
///
/// This reports COST honestly — it does NOT claim to detect *unused* skills
/// (that would need usage telemetry we don't have in a product context).
///
/// - 0 tokens → Pass ("no skills loaded") — a neutral state, never a failure.
/// - at or under the threshold → Pass (names the total).
/// - over the threshold → Warn (names the total, suggests review).
pub fn skill_context_cost_status(total_estimated_tokens: usize) -> (DoctorStatus, String) {
    if total_estimated_tokens == 0 {
        return (DoctorStatus::Pass, "no skills loaded".to_string());
    }
    if total_estimated_tokens > SKILL_CONTEXT_COST_WARN_TOKENS {
        (
            DoctorStatus::Warn,
            format!(
                "~{total_estimated_tokens} tokens of skill context (over ~{SKILL_CONTEXT_COST_WARN_TOKENS}) — review which skills you need"
            ),
        )
    } else {
        (
            DoctorStatus::Pass,
            format!("~{total_estimated_tokens} tokens of skill context"),
        )
    }
}

/// Sum the byte sizes of every `SKILL.md` in every directory skills can be
/// loaded from on this run: every auto-discovery dir AND any `--skills`
/// directories. Missing dirs contribute 0.
///
/// Product-safe: returns 0 when no skill dirs exist (any project, any setup).
fn discovered_skill_bytes() -> usize {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from);
    let dirs = skill_source_dirs(&cli::skill_flag_dirs(), home.as_deref(), xdg.as_deref());
    skill_bytes_in_dirs(&dirs)
}

/// Enumerate every directory skills can be loaded from, in discovery order.
///
/// Pure (no I/O) so the *enumeration itself* is testable — the previous version
/// of this audit hardcoded only the project + global auto-discovery dirs inline,
/// so skills supplied with `--skills` were structurally invisible and a repo
/// loading all of its skills that way was told "no skills loaded".
///
/// #728: `/skill install`'s destination is an auto-discovery source too, so it
/// belongs here as well. If a directory can load skills and this audit cannot
/// see it, the audit under-reports and calls it a pass — the exact blind spot
/// this function exists to close. `skill_bytes_in_dirs` dedups by canonicalized
/// path, so listing a dir twice cannot double-count.
fn skill_source_dirs(
    flag_dirs: &[std::path::PathBuf],
    home: Option<&std::path::Path>,
    xdg: Option<&std::path::Path>,
) -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = vec![std::path::PathBuf::from(".yoyo/skills")];
    if let Some(home) = home {
        dirs.push(home.join(".yoyo/skills"));
    }
    if let Some(install) = crate::commands_skill::skill_install_dir_from(xdg, home) {
        dirs.push(install);
    }
    dirs.extend(flag_dirs.iter().cloned());
    dirs
}

/// Sum `SKILL.md` bytes across several candidate skill dirs, counting each
/// real directory at most once.
///
/// The dedup matters: `--skills .yoyo/skills` names a directory auto-discovery
/// already covers, and double-counting would conjure the "over budget" warning
/// out of arithmetic rather than out of context spend. Paths that can't be
/// canonicalized (missing dirs) fall back to the literal path as the key.
fn skill_bytes_in_dirs(dirs: &[std::path::PathBuf]) -> usize {
    let mut seen: Vec<std::path::PathBuf> = Vec::new();
    let mut total = 0usize;
    for dir in dirs {
        let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        total += skill_bytes_in_dir(dir);
    }
    total
}

/// Sum byte sizes of `SKILL.md` files directly inside subdirectories of `dir`.
/// (Skills live at `<dir>/<skill-name>/SKILL.md`.) Missing dir → 0.
fn skill_bytes_in_dir(dir: &std::path::Path) -> usize {
    let mut total = 0usize;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let skill_md = entry.path().join("SKILL.md");
        if let Ok(meta) = std::fs::metadata(&skill_md) {
            total += meta.len() as usize;
        }
    }
    total
}

/// Return toolchain version checks for a given project type.
///
/// These check whether required development tools are installed
/// (e.g., compiler, build tool, package manager) — not whether the
/// project builds or tests pass (that's `health_checks_for_project`).
pub fn toolchain_checks_for_project(project_type: &ProjectType) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    /// Helper: run `cmd --version` (or custom args) and return a DoctorCheck.
    fn check_tool(name: &str, cmd: &str, args: &[&str]) -> DoctorCheck {
        match std::process::Command::new(cmd).args(args).output() {
            Ok(output) if output.status.success() => {
                let raw = String::from_utf8_lossy(&output.stdout);
                let ver = raw.lines().next().unwrap_or("").trim().to_string();
                DoctorCheck {
                    name: name.to_string(),
                    status: DoctorStatus::Pass,
                    detail: if ver.is_empty() {
                        "installed".to_string()
                    } else {
                        format!("installed ({ver})")
                    },
                }
            }
            _ => DoctorCheck {
                name: name.to_string(),
                status: DoctorStatus::Fail,
                detail: "not found".to_string(),
            },
        }
    }

    match project_type {
        ProjectType::Java => {
            checks.push(check_tool("Java", "java", &["--version"]));
            // Check JAVA_HOME env var
            if std::env::var("JAVA_HOME").is_ok() {
                checks.push(DoctorCheck {
                    name: "JAVA_HOME".to_string(),
                    status: DoctorStatus::Pass,
                    detail: std::env::var("JAVA_HOME").unwrap_or_default(),
                });
            } else {
                checks.push(DoctorCheck {
                    name: "JAVA_HOME".to_string(),
                    status: DoctorStatus::Warn,
                    detail: "not set".to_string(),
                });
            }
            // Check build tool — Maven or Gradle
            if std::path::Path::new("pom.xml").exists() {
                checks.push(check_tool("Maven", "mvn", &["--version"]));
            } else {
                checks.push(check_tool("Gradle", "gradle", &["--version"]));
            }
        }
        ProjectType::Ruby => {
            checks.push(check_tool("Ruby", "ruby", &["--version"]));
            checks.push(check_tool("Bundler", "bundle", &["--version"]));
            checks.push(check_tool("Gem", "gem", &["--version"]));
        }
        ProjectType::Cpp => {
            checks.push(check_tool("CMake", "cmake", &["--version"]));
            checks.push(check_tool("Make", "make", &["--version"]));
            // Try cc first, fall back to g++
            let cc = check_tool("C compiler", "cc", &["--version"]);
            if cc.status == DoctorStatus::Fail {
                checks.push(check_tool("C++ compiler", "g++", &["--version"]));
            } else {
                checks.push(cc);
            }
        }
        _ => {} // Other project types don't need additional toolchain checks here
    }

    checks
}

/// Display the doctor report from a list of checks.
pub fn print_doctor_report(checks: &[DoctorCheck]) {
    println!("\n  {BOLD}🩺 yoyo doctor{RESET}");
    println!("  {DIM}─────────────────────────────{RESET}");

    for check in checks {
        let (icon, color) = match check.status {
            DoctorStatus::Pass => ("✓", &GREEN),
            DoctorStatus::Fail => ("✗", &RED),
            DoctorStatus::Warn => ("⚠", &YELLOW),
        };
        println!(
            "  {color}{icon}{RESET} {BOLD}{}{RESET}: {}",
            check.name, check.detail
        );
    }

    let passed = checks
        .iter()
        .filter(|c| c.status == DoctorStatus::Pass)
        .count();
    let total = checks.len();
    let summary_color = if passed == total { &GREEN } else { &YELLOW };
    println!("\n  {summary_color}{passed}/{total} checks passed{RESET}\n");

    // Contextual handoff: if anything needs attention, point the user at the
    // command that acts on it instead of leaving them to find it.
    if let Some(hint) = doctor_handoff_hint(checks) {
        println!("  {DIM}{hint}{RESET}\n");
    }
}

/// Build the footer printed under the doctor rows when anything is not Pass.
///
/// Returns `None` on a fully-green run so the clean output stays pristine.
///
/// #1074: this used to say "Try /fix to attempt repairs, or /health for a full
/// check." for any non-Pass row. That pointer was wrong for every row doctor
/// has: `/fix` re-runs the project's build/lint/test commands
/// (`health_checks_for_project`), and no doctor row is one of those (version,
/// git, API key, config, context, curl, memory dir, RTK, toolchain presence,
/// skill cost). It was also printed to shell `yoyo doctor` users, who cannot
/// type a slash command. So the footer names no command: it gives honest
/// counts (failed vs warning, since both are counted) and points at the rows,
/// which already carry the detail. If a doctor row ever becomes a check `/fix`
/// re-runs, this is where a pointer would go, and it would then need to know
/// whether it is printing to the REPL or the shell.
pub fn doctor_handoff_hint(checks: &[DoctorCheck]) -> Option<String> {
    let fails = checks
        .iter()
        .filter(|c| c.status == DoctorStatus::Fail)
        .count();
    let warns = checks
        .iter()
        .filter(|c| c.status == DoctorStatus::Warn)
        .count();
    doctor_handoff_hint_from_counts(fails, warns)
}

/// Format the footer from raw Fail and Warn counts.
///
/// Split out so it's unit-testable without constructing `DoctorCheck`s.
fn doctor_handoff_hint_from_counts(fails: usize, warns: usize) -> Option<String> {
    let failed = match fails {
        0 => None,
        1 => Some("1 check failed".to_string()),
        n => Some(format!("{n} checks failed")),
    };
    let warned = match warns {
        0 => None,
        1 => Some("1 warning".to_string()),
        n => Some(format!("{n} warnings")),
    };
    let counts = match (failed, warned) {
        (None, None) => return None,
        (Some(f), None) => f,
        (None, Some(w)) => w,
        (Some(f), Some(w)) => format!("{f}, {w}"),
    };
    Some(format!("→ {counts}. See each row above for details."))
}

/// Handle the `/doctor` command.
pub fn handle_doctor(
    provider: &str,
    model: &str,
    restrictions: &crate::config::DirectoryRestrictions,
    surface: DoctorSurface,
) {
    let checks = run_doctor_checks(provider, model, restrictions, surface);
    print_doctor_report(&checks);
    // Contextual guidance beats reference guidance: after the report, take the
    // last step Claude Code takes — offer to apply the one safe, well-defined
    // fix when one exists. Interactive-only
    // (TTY-gated) so piped/harness mode stays report-only and never blocks.
    maybe_offer_fix(&checks);
}

/// A concrete, safe fix action `/doctor` can offer to run for the user.
///
/// `label` is a human-readable description of what will happen; `command` is
/// the shell command to run. Both are surfaced before we prompt, so the user
/// sees exactly what they're confirming.
#[derive(Debug, Clone, PartialEq)]
pub struct DoctorFix {
    pub label: String,
    pub command: String,
}

/// Decide the single safe fix (if any) `/doctor` should offer to apply.
///
/// Pure decision logic (no I/O) so it's unit-testable in isolation. Deliberately
/// conservative (retreat size): we only return a fix for checks whose repair is
/// well-defined, additive, non-destructive, and project-agnostic. Today that's
/// exactly one — a missing `.yoyo/` memory dir, fixed by creating it. Every
/// other Warn/Fail is left to its row's own detail text because it
/// either needs the agent or isn't a single safe shell command.
///
/// Returns `None` on a green run and for any check we can't safely auto-fix, so
/// the offer is silent unless we're certain the command is harmless.
pub fn doctor_fix_action(checks: &[DoctorCheck]) -> Option<DoctorFix> {
    for check in checks {
        if check.name == "Memory dir" && check.status == DoctorStatus::Warn {
            // `mkdir -p` is idempotent and purely additive — never destructive,
            // never a Rust/CI assumption. Safe for any user's project.
            return Some(DoctorFix {
                label: "Create the missing .yoyo/ memory directory".to_string(),
                command: "mkdir -p .yoyo".to_string(),
            });
        }
    }
    None
}

/// Interactive tail of `/doctor`: if there's a safe fix and we're on a TTY,
/// prompt to apply it. No-op in piped/non-interactive mode (preserves the
/// report-only, plain-text harness contract) and when no safe fix exists.
fn maybe_offer_fix(checks: &[DoctorCheck]) {
    let Some(fix) = doctor_fix_action(checks) else {
        return;
    };

    // Only prompt when stdin is an interactive terminal. In piped/harness mode
    // we skip entirely — never hang waiting for input, never change behavior.
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return;
    }

    print!(
        "  {BOLD}{}{RESET}\n  {DIM}$ {}{RESET}\n  {BOLD}Apply this fix now? [y/N]: {RESET}",
        fix.label, fix.command
    );
    use std::io::Write;
    std::io::stdout().flush().ok();

    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        println!("\n  {DIM}Skipped.{RESET}\n");
        return;
    }
    let answer = answer.trim().to_lowercase();
    if answer != "y" && answer != "yes" {
        // Default N: clean no-op, leave state untouched.
        println!("  {DIM}No changes made.{RESET}\n");
        return;
    }

    // Run the safe command and report the HONEST outcome — never pre-announce
    // success before checking the exit code.
    println!("  {DIM}Running: {}{RESET}", fix.command);
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(&fix.command)
        .status()
    {
        Ok(status) if status.success() => {
            println!("  {GREEN}✓ Fix applied.{RESET}\n");
        }
        Ok(status) => {
            let code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            println!("  {RED}✗ Fix command exited with status {code}.{RESET}\n");
        }
        Err(e) => {
            println!("  {RED}✗ Could not run fix command: {e}{RESET}\n");
        }
    }
}

/// Return health check commands for a given project type.
#[allow(clippy::vec_init_then_push, unused_mut)]
pub fn health_checks_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => {
            let mut checks = vec![("build", vec!["cargo", "build"])];
            #[cfg(not(test))]
            checks.push(("test", vec!["cargo", "test"]));
            checks.push((
                "clippy",
                vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
            ));
            checks.push(("fmt", vec!["cargo", "fmt", "--", "--check"]));
            checks
        }
        ProjectType::Node => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["npm", "test"]));
            checks.push(("lint", vec!["npx", "eslint", "."]));
            checks
        }
        ProjectType::Python => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["python", "-m", "pytest"]));
            checks.push(("lint", vec!["python", "-m", "flake8", "."]));
            checks.push(("typecheck", vec!["python", "-m", "mypy", "."]));
            checks
        }
        ProjectType::Go => {
            let mut checks = vec![("build", vec!["go", "build", "./..."])];
            #[cfg(not(test))]
            checks.push(("test", vec!["go", "test", "./..."]));
            checks.push(("vet", vec!["go", "vet", "./..."]));
            checks
        }
        ProjectType::Make => {
            // In test builds the push is cfg-gated out, leaving `checks`
            // effectively immutable — but mut is required for production.
            #[cfg(not(test))]
            {
                vec![("test", vec!["make", "test"])]
            }
            #[cfg(test)]
            {
                vec![]
            }
        }
        ProjectType::Java => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            if std::path::Path::new("pom.xml").exists() {
                checks.push(("build", vec!["mvn", "compile"]));
                #[cfg(not(test))]
                checks.push(("test", vec!["mvn", "test"]));
            } else {
                checks.push(("build", vec!["./gradlew", "build"]));
                #[cfg(not(test))]
                checks.push(("test", vec!["./gradlew", "test"]));
            }
            checks
        }
        ProjectType::Ruby => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["bundle", "exec", "rake", "test"]));
            checks.push(("lint", vec!["bundle", "exec", "rubocop"]));
            checks
        }
        ProjectType::Cpp => {
            let mut checks = vec![("build", vec!["cmake", "--build", "build"])];
            #[cfg(not(test))]
            checks.push(("test", vec!["ctest", "--test-dir", "build"]));
            checks
        }
        ProjectType::Unknown => vec![],
    }
}

/// The outcome of one health-check command, in a portable shape.
///
/// Deliberately **not** `std::process::Output`: constructing one in a test
/// needs `ExitStatusExt`, which is platform-specific, and this crate ships
/// Windows builds. Carrying `success`/`stdout`/`stderr` keeps the decision
/// half drivable from a test on every target.
struct CheckOutcome {
    success: bool,
    stdout: String,
    stderr: String,
}

/// The impure half: actually spawn one health-check command.
///
/// This is the only thing the two wrappers below inject, and it is why no
/// `#[test]` in this module spawns `cargo` any more (#832). A test that shells
/// `cargo build` rebuilds the bin with whatever feature set the nested
/// invocation defaults to and uplifts it over the **shared** path
/// `target/debug/yoyo`, so the featured binary the integration tests reach
/// through `CARGO_BIN_EXE_yoyo` silently becomes the plain one.
fn run_check_command(args: &[&str]) -> Result<CheckOutcome, String> {
    match std::process::Command::new(args[0])
        .args(&args[1..])
        .output()
    {
        Ok(o) => Ok(CheckOutcome {
            success: o.status.success(),
            stdout: String::from_utf8_lossy(&o.stdout).to_string(),
            stderr: String::from_utf8_lossy(&o.stderr).to_string(),
        }),
        Err(e) => Err(e.to_string()),
    }
}

/// Run health checks for a specific project type. Returns (name, passed, detail) tuples.
///
/// Thin wrapper over [`run_health_check_for_project_with`], supplying the real
/// subprocess runner. Output is unchanged.
pub fn run_health_check_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    run_health_check_for_project_with(project_type, &run_check_command)
}

/// Decision/rendering half of [`run_health_check_for_project`], with the
/// subprocess injected. Same checks, same order, same strings.
fn run_health_check_for_project_with(
    project_type: &ProjectType,
    run: &dyn Fn(&[&str]) -> Result<CheckOutcome, String>,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let start = std::time::Instant::now();
        let output = run(&args);
        let elapsed = format_duration(start.elapsed());
        match output {
            Ok(o) if o.success => {
                results.push((name, true, format!("ok ({elapsed})")));
            }
            Ok(o) => {
                let first_line = o.stderr.lines().next().unwrap_or("(unknown error)");
                results.push((
                    name,
                    false,
                    format!(
                        "FAIL ({elapsed}): {}",
                        truncate_with_ellipsis(first_line, 80)
                    ),
                ));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Run health checks and capture full error output for failures.
///
/// Thin wrapper over [`run_health_checks_full_output_with`], supplying the real
/// subprocess runner. Output is unchanged.
pub fn run_health_checks_full_output(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    run_health_checks_full_output_with(project_type, &run_check_command)
}

/// Decision/rendering half of [`run_health_checks_full_output`], with the
/// subprocess injected. Same checks, same order, same strings.
fn run_health_checks_full_output_with(
    project_type: &ProjectType,
    run: &dyn Fn(&[&str]) -> Result<CheckOutcome, String>,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        match run(&args) {
            Ok(o) if o.success => {
                results.push((name, true, String::new()));
            }
            Ok(o) => {
                let mut full_output = String::new();
                if !o.stdout.is_empty() {
                    full_output.push_str(&o.stdout);
                }
                if !o.stderr.is_empty() {
                    if !full_output.is_empty() {
                        full_output.push('\n');
                    }
                    full_output.push_str(&o.stderr);
                }
                results.push((name, false, full_output));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Build a prompt describing health check failures for the AI to fix.
pub fn build_fix_prompt(failures: &[(&str, &str)]) -> String {
    if failures.is_empty() {
        return String::new();
    }
    let mut prompt = String::from(
        "Fix the following build/lint errors in this project. Read the relevant files, understand the errors, and apply fixes:\n\n",
    );
    for (name, output) in failures {
        prompt.push_str(&format!("## {name} errors:\n```\n{output}\n```\n\n"));
    }
    prompt.push_str(
        "After fixing, run the failing checks again to verify. Fix any remaining issues.",
    );
    prompt
}

/// Exit status of the "is there a project here at all?" step, shared by the
/// shell doors `yoyo health` and `yoyo lint` (#982): 1 when no recognized
/// project was found (nothing ran, so a script must not read it as success),
/// 0 otherwise. Says nothing about whether the checks that follow pass.
pub(crate) fn no_project_status(project_type: &ProjectType) -> i32 {
    if *project_type == ProjectType::Unknown {
        1
    } else {
        0
    }
}

pub fn handle_health() {
    let _ = handle_health_status();
}

/// `/health` with an exit status for the shell door (`yoyo health`, #982):
/// 1 when no recognized project was found, 1 when any check that ran failed
/// (including a check whose command could not be spawned), and 0 when every
/// check passed or none is configured for the project type (nothing ran,
/// nothing failed). The code is 1 rather than a child's own code because
/// several checks run and no single child's code speaks for all of them.
/// The REPL calls `handle_health`, which discards the code.
pub fn handle_health_status() -> i32 {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    let no_project = no_project_status(&project_type);
    if no_project != 0 {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return no_project;
    }
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_check_for_project(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return 0;
    }
    let all_passed = results.iter().all(|(_, passed, _)| *passed);
    for (name, passed, detail) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        println!("  {icon} {name}: {detail}");
    }
    if all_passed {
        println!("\n{GREEN}  All checks passed ✓{RESET}\n");
    } else {
        println!("\n{RED}  Some checks failed ✗{RESET}\n");
    }
    health_exit_code(all_passed)
}

/// Exit status for a health run whose checks all executed: 0 when every
/// check passed, 1 when any failed (#982 — stdout already said "Some checks
/// failed ✗", so a script reading `$?` must not hear success).
pub(crate) fn health_exit_code(all_passed: bool) -> i32 {
    if all_passed {
        0
    } else {
        1
    }
}

/// Handle the /fix command. Returns Some(fix_prompt) if failures were sent to AI, None otherwise.
pub async fn handle_fix(
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }
    println!("{DIM}  Detected project: {project_type}{RESET}");
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_checks_full_output(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return None;
    }
    for (name, passed, _) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        let status = if *passed { "ok" } else { "FAIL" };
        println!("  {icon} {name}: {status}");
    }
    let failures: Vec<(&str, &str)> = results
        .iter()
        .filter(|(_, passed, _)| !passed)
        .map(|(name, _, output)| (*name, output.as_str()))
        .collect();
    if failures.is_empty() {
        println!("\n{GREEN}  All checks passed — nothing to fix ✓{RESET}\n");
        return None;
    }
    let fail_count = failures.len();
    println!("\n{YELLOW}  Sending {fail_count} failure(s) to AI for fixing...{RESET}\n");
    let mut fix_prompt = build_fix_prompt(&failures);
    // Include last failed /run output if available (gives the agent more context)
    if let Some(last_run) = get_last_failed_run() {
        fix_prompt.push_str(&format!(
            "\n\nAdditional context — the last `/run` command failed with exit code {}:\nstderr:\n```\n{}\n```\nstdout:\n```\n{}\n```",
            last_run.exit_code, last_run.stderr, last_run.stdout
        ));
    }
    run_prompt(agent, &fix_prompt, session_total, model).await;
    auto_compact_if_needed(agent);
    Some(fix_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};

    // #982: the shell doors `yoyo health` / `yoyo lint` exit with this status.
    // Driven through the real detector on real directories, never a typed-in
    // ProjectType, so the detection step is covered too.
    #[test]
    fn test_no_project_status_is_1_in_an_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let pt = detect_project_type(tmp.path());
        assert_eq!(
            pt,
            ProjectType::Unknown,
            "fixture must really be project-less"
        );
        assert_eq!(no_project_status(&pt), 1);
    }

    #[test]
    fn test_no_project_status_is_0_when_a_project_marker_exists() {
        for marker in ["Cargo.toml", "Makefile"] {
            let tmp = tempfile::tempdir().unwrap();
            std::fs::write(tmp.path().join(marker), "").unwrap();
            let pt = detect_project_type(tmp.path());
            assert_ne!(pt, ProjectType::Unknown, "{marker} must be detected");
            assert_eq!(no_project_status(&pt), 0, "{marker}");
        }
    }

    #[test]
    fn test_skill_bytes_to_tokens() {
        assert_eq!(skill_bytes_to_tokens(0), 0);
        // rounds up: 1..=4 bytes → 1 token
        assert_eq!(skill_bytes_to_tokens(1), 1);
        assert_eq!(skill_bytes_to_tokens(4), 1);
        assert_eq!(skill_bytes_to_tokens(5), 2);
        assert_eq!(skill_bytes_to_tokens(8000), 2000);
    }

    #[test]
    fn test_skill_source_dirs_includes_flag_dirs() {
        // The bug this pins: skills loaded via `--skills <dir>` were invisible to
        // the /doctor cost audit, which only ever looked at the project + global
        // auto-discovery dirs. On a repo that passes `--skills ./skills` (this one does) the audit
        // summed 0 bytes and reported "no skills loaded" — a false clean bill.
        let flag = vec![std::path::PathBuf::from("./skills")];
        let dirs = skill_source_dirs(&flag, Some(std::path::Path::new("/home/tester")), None);
        assert!(
            dirs.contains(&std::path::PathBuf::from("./skills")),
            "flag skill dirs must be part of the cost audit's source set: {dirs:?}"
        );
        assert!(dirs.contains(&std::path::PathBuf::from(".yoyo/skills")));
        assert!(dirs.contains(&std::path::PathBuf::from("/home/tester/.yoyo/skills")));
    }

    #[test]
    fn test_skill_source_dirs_without_home_still_has_project_and_flags() {
        let flag = vec![std::path::PathBuf::from("/opt/team-skills")];
        let dirs = skill_source_dirs(&flag, None, None);
        assert!(dirs.contains(&std::path::PathBuf::from(".yoyo/skills")));
        assert!(dirs.contains(&std::path::PathBuf::from("/opt/team-skills")));
        assert_eq!(
            dirs.len(),
            2,
            "no HOME and no XDG → no global dir and no install dir: {dirs:?}"
        );
    }

    #[test]
    fn test_skill_source_dirs_includes_the_install_dir() {
        // #728: `/skill install` writes into ~/.config/yoyo/skills/, which is now
        // an auto-discovery source. A directory that can LOAD skills but that the
        // cost audit cannot SEE makes the audit under-report and call it a pass —
        // the same blind spot the `--skills` half fixed on Day 151.
        let dirs = skill_source_dirs(&[], Some(std::path::Path::new("/home/tester")), None);
        assert!(
            dirs.contains(&std::path::PathBuf::from(
                "/home/tester/.config/yoyo/skills"
            )),
            "the /skill install destination must be audited too: {dirs:?}"
        );
    }

    #[test]
    fn test_skill_source_dirs_install_dir_honours_xdg() {
        // XDG_CONFIG_HOME wins over ~/.config — and the rule is not re-derived
        // here, it comes from commands_skill::skill_install_dir_from.
        let dirs = skill_source_dirs(
            &[],
            Some(std::path::Path::new("/home/tester")),
            Some(std::path::Path::new("/xdg")),
        );
        assert!(
            dirs.contains(&std::path::PathBuf::from("/xdg/yoyo/skills")),
            "XDG_CONFIG_HOME must decide the install dir: {dirs:?}"
        );
        assert!(
            !dirs.contains(&std::path::PathBuf::from(
                "/home/tester/.config/yoyo/skills"
            )),
            "with XDG set, the ~/.config fallback must not also be counted: {dirs:?}"
        );
    }

    #[test]
    fn test_skill_bytes_in_dirs_sums_across_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        std::fs::create_dir_all(a.join("one")).unwrap();
        std::fs::create_dir_all(b.join("two")).unwrap();
        std::fs::write(a.join("one/SKILL.md"), "x".repeat(100)).unwrap();
        std::fs::write(b.join("two/SKILL.md"), "y".repeat(50)).unwrap();
        assert_eq!(skill_bytes_in_dirs(&[a.clone(), b.clone()]), 150);
    }

    #[test]
    fn test_skill_bytes_in_dirs_dedups_same_dir() {
        // `--skills .yoyo/skills` names a dir auto-discovery already covers.
        // Counting it twice would over-report the warning into existence.
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("skills");
        std::fs::create_dir_all(d.join("one")).unwrap();
        std::fs::write(d.join("one/SKILL.md"), "z".repeat(80)).unwrap();
        assert_eq!(skill_bytes_in_dirs(&[d.clone(), d.clone()]), 80);
        // Same directory reached by a different-looking path: also once.
        let alias = d.join(".").join("..").join("skills");
        assert_eq!(skill_bytes_in_dirs(&[d.clone(), alias]), 80);
    }

    #[test]
    fn test_skill_bytes_in_dirs_missing_dirs_are_zero_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope");
        assert_eq!(skill_bytes_in_dirs(&[missing]), 0);
        assert_eq!(skill_bytes_in_dirs(&[]), 0);
    }

    #[test]
    fn test_skill_context_cost_no_skills_is_neutral_pass() {
        let (status, msg) = skill_context_cost_status(0);
        assert_eq!(status, DoctorStatus::Pass);
        assert!(msg.contains("no skills loaded"), "msg was: {msg}");
    }

    #[test]
    fn test_skill_context_cost_small_is_pass() {
        let (status, msg) = skill_context_cost_status(2000);
        assert_eq!(status, DoctorStatus::Pass);
        assert!(msg.contains("2000"), "msg was: {msg}");
    }

    #[test]
    fn test_skill_context_cost_large_is_warn_with_count() {
        let (status, msg) = skill_context_cost_status(12000);
        assert_eq!(status, DoctorStatus::Warn);
        assert!(msg.contains("12000"), "msg should name the count: {msg}");
    }

    #[test]
    fn test_skill_context_cost_boundary_at_threshold_is_pass() {
        // Exactly at the threshold must NOT warn (paired near-miss, day-122/123).
        let (status, _) = skill_context_cost_status(SKILL_CONTEXT_COST_WARN_TOKENS);
        assert_eq!(status, DoctorStatus::Pass);
    }

    #[test]
    fn test_skill_context_cost_boundary_over_threshold_is_warn() {
        // One token over the threshold flips to Warn — the minimal near-miss.
        let (status, _) = skill_context_cost_status(SKILL_CONTEXT_COST_WARN_TOKENS + 1);
        assert_eq!(status, DoctorStatus::Warn);
    }

    #[test]
    fn health_checks_rust_has_build() {
        let checks = health_checks_for_project(&ProjectType::Rust);
        assert!(checks.iter().any(|(name, _)| *name == "build"));
    }

    #[test]
    fn health_checks_unknown_empty() {
        let checks = health_checks_for_project(&ProjectType::Unknown);
        assert!(checks.is_empty());
    }

    #[test]
    fn doctor_checks_include_rtk() {
        let checks = run_doctor_checks(
            "anthropic",
            "test-model",
            &Default::default(),
            DoctorSurface::Repl,
        );
        assert!(
            checks.iter().any(|c| c.name == "RTK"),
            "doctor checks should include an RTK entry"
        );
        // RTK check should always be Pass (never Fail), since it's optional
        let rtk_check = checks.iter().find(|c| c.name == "RTK").unwrap();
        assert_ne!(
            rtk_check.status,
            DoctorStatus::Fail,
            "RTK should never be Fail — it's optional"
        );
    }

    // ── build_fix_prompt ────────────────────────────────────────────

    #[test]
    fn build_fix_prompt_empty() {
        let prompt = build_fix_prompt(&[]);
        assert!(prompt.is_empty());
    }

    #[test]
    fn build_fix_prompt_with_failures() {
        let failures = vec![("build", "error[E0308]: mismatched types")];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("build errors"));
        assert!(prompt.contains("E0308"));
        assert!(prompt.contains("Fix"));
    }

    #[test]
    fn build_fix_prompt_multiple_failures() {
        let failures = vec![
            ("build", "build error output"),
            ("clippy", "clippy warning output"),
        ];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("## build errors"));
        assert!(prompt.contains("## clippy errors"));
    }

    // ── build_lint_fix_prompt ──────────────────────────────────────────
    // ── format_tree_from_paths ──────────────────────────────────────

    // ── moved from commands.rs (issue #260) ────────────────────────

    #[test]
    fn test_health_check_function() {
        // Driven through `run_health_check_for_project_with` with a stub
        // runner, never the real one. The pre-#832 version called
        // `run_health_check_for_project`, which shells a nested, feature-less
        // `cargo build`; that uplifts the plain binary over the SHARED path
        // `target/debug/yoyo`, so the featured binary the integration tests
        // reach through `CARGO_BIN_EXE_yoyo` silently became the plain one.
        // It also cost ~14s for this one test.
        // "cargo test" is still skipped under #[cfg(test)] to avoid recursion.
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);

        let seen = std::cell::RefCell::new(Vec::new());
        let run = |args: &[&str]| -> Result<CheckOutcome, String> {
            seen.borrow_mut().push(args.join(" "));
            Ok(CheckOutcome {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            })
        };
        let results = run_health_check_for_project_with(&project_type, &run);
        assert!(
            !results.is_empty(),
            "Health check should return at least one result"
        );
        for (name, passed, _) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
        // "test" check should be excluded under cfg(test)
        assert!(
            !results.iter().any(|(name, _, _)| *name == "test"),
            "cargo test check should be skipped to avoid recursion"
        );
        // The build check is still literally `cargo build` — pinned here
        // without ever spawning it.
        let seen = seen.into_inner();
        assert!(
            seen.iter().any(|c| c == "cargo build"),
            "expected a `cargo build` check, got {seen:?}"
        );
        assert!(
            !seen.iter().any(|c| c == "cargo test"),
            "no `cargo test` check under cfg(test), got {seen:?}"
        );
    }

    #[test]
    fn test_health_checks_for_rust_project() {
        let checks = health_checks_for_project(&ProjectType::Rust);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Rust should have build check");
        assert!(names.contains(&"clippy"), "Rust should have clippy check");
        assert!(names.contains(&"fmt"), "Rust should have fmt check");
        // test is excluded under cfg(test)
        assert!(
            !names.contains(&"test"),
            "test should be excluded in cfg(test)"
        );
    }

    #[test]
    fn test_health_checks_for_node_project() {
        let checks = health_checks_for_project(&ProjectType::Node);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Node should have lint check");
    }

    #[test]
    fn test_health_checks_for_go_project() {
        let checks = health_checks_for_project(&ProjectType::Go);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Go should have build check");
        assert!(names.contains(&"vet"), "Go should have vet check");
    }

    #[test]
    fn test_health_checks_for_python_project() {
        let checks = health_checks_for_project(&ProjectType::Python);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Python should have lint check");
        assert!(names.contains(&"typecheck"), "Python should have typecheck");
    }

    #[test]
    fn test_health_checks_for_unknown_returns_empty() {
        let checks = health_checks_for_project(&ProjectType::Unknown);
        assert!(checks.is_empty(), "Unknown project should return no checks");
    }

    #[test]
    fn test_run_command_recognized() {
        assert!(!is_unknown_command("/run"));
        assert!(!is_unknown_command("/run echo hello"));
        assert!(!is_unknown_command("/run ls -la"));
    }

    #[test]
    fn test_fix_command_recognized() {
        assert!(!is_unknown_command("/fix"));
        assert!(
            KNOWN_COMMANDS.contains(&"/fix"),
            "/fix should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_run_health_checks_full_output_returns_results() {
        // Driven through `run_health_checks_full_output_with` with a stub
        // runner — see `test_health_check_function` for why no test in this
        // module may spawn cargo (#832).
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);

        let run = |args: &[&str]| -> Result<CheckOutcome, String> {
            if args.first() == Some(&"cargo") && args.get(1) == Some(&"build") {
                Ok(CheckOutcome {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            } else {
                Ok(CheckOutcome {
                    success: false,
                    stdout: "out".to_string(),
                    stderr: "err".to_string(),
                })
            }
        };
        let results = run_health_checks_full_output_with(&project_type, &run);
        assert!(
            !results.is_empty(),
            "Should return at least one check result"
        );
        for (name, passed, _output) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
        // A failing check reports stdout and stderr joined by a newline — the
        // branch the pre-#832 test could never reach, since a real `cargo
        // build` in this repo succeeds.
        let failed = results
            .iter()
            .find(|(_, passed, _)| !*passed)
            .expect("stub fails every non-build check");
        assert_eq!(failed.2, "out\nerr");
        // A passing check carries no captured output.
        let built = results
            .iter()
            .find(|(name, _, _)| *name == "build")
            .expect("build check present");
        assert_eq!(built.2, "");
    }

    #[test]
    fn test_build_fix_prompt_with_failures() {
        let failures = vec![
            (
                "build",
                "error[E0308]: mismatched types\n  --> src/main.rs:42",
            ),
            (
                "clippy",
                "warning: unused variable `x`\n  --> src/lib.rs:10",
            ),
        ];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("build"), "Prompt should mention build");
        assert!(prompt.contains("clippy"), "Prompt should mention clippy");
        assert!(
            prompt.contains("error[E0308]"),
            "Prompt should include build error"
        );
        assert!(
            prompt.contains("unused variable"),
            "Prompt should include clippy warning"
        );
    }

    #[test]
    fn test_build_fix_prompt_empty_failures() {
        let failures: Vec<(&str, &str)> = vec![];
        let prompt = build_fix_prompt(&failures);
        assert!(
            prompt.is_empty() || prompt.contains("Fix"),
            "Empty failures should produce empty or minimal prompt"
        );
    }

    // --- Java project health checks ---

    #[test]
    fn test_health_checks_for_java_project() {
        let checks = health_checks_for_project(&ProjectType::Java);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Java should have build check");
        // test is excluded under cfg(test)
        assert!(
            !names.contains(&"test"),
            "test should be excluded in cfg(test)"
        );
    }

    // --- Ruby project health checks ---

    #[test]
    fn test_health_checks_for_ruby_project() {
        let checks = health_checks_for_project(&ProjectType::Ruby);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Ruby should have lint check");
        // test is excluded under cfg(test)
        assert!(
            !names.contains(&"test"),
            "test should be excluded in cfg(test)"
        );
    }

    // --- Cpp project health checks ---

    #[test]
    fn test_health_checks_for_cpp_project() {
        let checks = health_checks_for_project(&ProjectType::Cpp);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Cpp should have build check");
        // test is excluded under cfg(test)
        assert!(
            !names.contains(&"test"),
            "test should be excluded in cfg(test)"
        );
    }

    // --- Make project health checks ---

    #[test]
    fn test_health_checks_for_make_project() {
        let checks = health_checks_for_project(&ProjectType::Make);
        // Under cfg(test), Make returns empty (test is the only check and it's gated)
        assert!(
            checks.is_empty(),
            "Make project should have no checks in cfg(test)"
        );
    }

    // --- DoctorCheck and DoctorStatus infrastructure ---

    #[test]
    fn test_doctor_status_equality() {
        assert_eq!(DoctorStatus::Pass, DoctorStatus::Pass);
        assert_eq!(DoctorStatus::Fail, DoctorStatus::Fail);
        assert_eq!(DoctorStatus::Warn, DoctorStatus::Warn);
        assert_ne!(DoctorStatus::Pass, DoctorStatus::Fail);
        assert_ne!(DoctorStatus::Pass, DoctorStatus::Warn);
        assert_ne!(DoctorStatus::Fail, DoctorStatus::Warn);
    }

    #[test]
    fn test_doctor_check_construction() {
        let check = DoctorCheck {
            name: "Test tool".to_string(),
            status: DoctorStatus::Pass,
            detail: "v1.2.3".to_string(),
        };
        assert_eq!(check.name, "Test tool");
        assert_eq!(check.status, DoctorStatus::Pass);
        assert_eq!(check.detail, "v1.2.3");

        let cloned = check.clone();
        assert_eq!(cloned.name, check.name);
        assert_eq!(cloned.status, check.status);
        assert_eq!(cloned.detail, check.detail);
    }

    #[test]
    fn test_run_doctor_checks_structure() {
        let checks = run_doctor_checks(
            "anthropic",
            "test-model",
            &Default::default(),
            DoctorSurface::Repl,
        );
        // Should always have Version, Git, Git repo, Provider, API key, Model at minimum
        let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Version"), "Should check version");
        assert!(names.contains(&"Git"), "Should check git");
        assert!(names.contains(&"Provider"), "Should check provider");
        assert!(names.contains(&"Model"), "Should check model");
        assert!(names.contains(&"RTK"), "Should check RTK");

        // Provider should reflect what we passed in
        let provider_check = checks.iter().find(|c| c.name == "Provider").unwrap();
        assert_eq!(provider_check.detail, "anthropic");
        assert_eq!(provider_check.status, DoctorStatus::Pass);

        // Model should reflect what we passed in
        let model_check = checks.iter().find(|c| c.name == "Model").unwrap();
        assert_eq!(model_check.detail, "test-model");
        assert_eq!(model_check.status, DoctorStatus::Pass);
    }

    #[test]
    fn test_print_doctor_report_all_pass() {
        // Just ensure it doesn't panic — output goes to stdout
        let checks = vec![
            DoctorCheck {
                name: "A".to_string(),
                status: DoctorStatus::Pass,
                detail: "ok".to_string(),
            },
            DoctorCheck {
                name: "B".to_string(),
                status: DoctorStatus::Pass,
                detail: "ok".to_string(),
            },
        ];
        print_doctor_report(&checks); // should not panic
    }

    #[test]
    fn test_print_doctor_report_mixed_statuses() {
        let checks = vec![
            DoctorCheck {
                name: "Pass check".to_string(),
                status: DoctorStatus::Pass,
                detail: "all good".to_string(),
            },
            DoctorCheck {
                name: "Warn check".to_string(),
                status: DoctorStatus::Warn,
                detail: "something to note".to_string(),
            },
            DoctorCheck {
                name: "Fail check".to_string(),
                status: DoctorStatus::Fail,
                detail: "broken".to_string(),
            },
        ];
        print_doctor_report(&checks); // should not panic
    }

    #[test]
    fn test_print_doctor_report_empty() {
        print_doctor_report(&[]); // should not panic, 0/0 checks passed
    }

    // --- Toolchain checks for project types ---

    #[test]
    fn test_toolchain_checks_java() {
        let checks = toolchain_checks_for_project(&ProjectType::Java);
        let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Java"), "Java toolchain should check java");
        assert!(
            names.contains(&"JAVA_HOME"),
            "Java toolchain should check JAVA_HOME"
        );
        // Should have either Maven or Gradle
        assert!(
            names.contains(&"Maven") || names.contains(&"Gradle"),
            "Java toolchain should check build tool"
        );
    }

    #[test]
    fn test_toolchain_checks_ruby() {
        let checks = toolchain_checks_for_project(&ProjectType::Ruby);
        let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Ruby"), "Ruby toolchain should check ruby");
        assert!(
            names.contains(&"Bundler"),
            "Ruby toolchain should check bundler"
        );
        assert!(names.contains(&"Gem"), "Ruby toolchain should check gem");
        assert_eq!(
            checks.len(),
            3,
            "Ruby should have exactly 3 toolchain checks"
        );
    }

    #[test]
    fn test_toolchain_checks_cpp() {
        let checks = toolchain_checks_for_project(&ProjectType::Cpp);
        let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"CMake"), "Cpp toolchain should check cmake");
        assert!(names.contains(&"Make"), "Cpp toolchain should check make");
        // Should have either C compiler or C++ compiler
        assert!(
            names.contains(&"C compiler") || names.contains(&"C++ compiler"),
            "Cpp toolchain should check a compiler"
        );
        assert_eq!(
            checks.len(),
            3,
            "Cpp should have exactly 3 toolchain checks"
        );
    }

    #[test]
    fn test_toolchain_checks_unknown_empty() {
        let checks = toolchain_checks_for_project(&ProjectType::Unknown);
        assert!(
            checks.is_empty(),
            "Unknown project should return no toolchain checks"
        );
    }

    #[test]
    fn test_toolchain_checks_rust_empty() {
        // Rust toolchain checks happen via health_checks_for_project, not toolchain_checks
        let checks = toolchain_checks_for_project(&ProjectType::Rust);
        assert!(
            checks.is_empty(),
            "Rust doesn't need separate toolchain checks here"
        );
    }

    fn check(status: DoctorStatus) -> DoctorCheck {
        DoctorCheck {
            name: "x".to_string(),
            status,
            detail: String::new(),
        }
    }

    #[test]
    fn test_handoff_hint_all_green_is_none() {
        assert_eq!(doctor_handoff_hint_from_counts(0, 0), None);
        let checks = vec![check(DoctorStatus::Pass), check(DoctorStatus::Pass)];
        assert_eq!(doctor_handoff_hint(&checks), None);
    }

    /// Build a named check with a given status (the bare `check()` helper
    /// names everything "x", which `doctor_fix_action` deliberately ignores).
    fn named_check(name: &str, status: DoctorStatus) -> DoctorCheck {
        DoctorCheck {
            name: name.to_string(),
            status,
            detail: String::new(),
        }
    }

    #[test]
    fn test_fix_action_no_issues_is_none() {
        // A fully-green run offers nothing — nothing to fix.
        let checks = vec![
            named_check("Version", DoctorStatus::Pass),
            named_check("Memory dir", DoctorStatus::Pass),
        ];
        assert!(doctor_fix_action(&checks).is_none());
        assert!(doctor_fix_action(&[]).is_none());
    }

    #[test]
    fn test_fix_action_missing_memory_dir_offers_safe_command() {
        // The "Memory dir" Warn is the one check with a well-defined, safe,
        // project-agnostic fix: create the missing directory.
        let checks = vec![
            named_check("Version", DoctorStatus::Pass),
            named_check("Memory dir", DoctorStatus::Warn),
        ];
        let fix = doctor_fix_action(&checks).expect("missing memory dir -> Some");
        assert!(
            fix.command.contains(".yoyo"),
            "fix should target .yoyo/: {}",
            fix.command
        );
        assert!(!fix.label.is_empty(), "fix must carry a human label");
    }

    #[test]
    fn test_fix_action_is_never_destructive() {
        // Per safety lessons: the offered command must never start with a
        // destructive verb. Assert the negative for the fix we DO offer.
        let checks = vec![named_check("Memory dir", DoctorStatus::Warn)];
        let fix = doctor_fix_action(&checks).expect("Some");
        let cmd = fix.command.trim_start().to_lowercase();
        for forbidden in ["rm ", "rm\t", "git reset", "git checkout", "git clean"] {
            assert!(
                !cmd.starts_with(forbidden) && !cmd.contains(&format!("&& {forbidden}")),
                "fix command must be non-destructive, got: {}",
                fix.command
            );
        }
    }

    #[test]
    fn test_fix_action_ignores_unfixable_warns() {
        // Warns without a known safe fix (e.g. Config file, curl) must NOT
        // trigger an offer — we only act on fixes the check logic knows.
        let checks = vec![
            named_check("Config file", DoctorStatus::Warn),
            named_check("Curl", DoctorStatus::Warn),
            named_check("Git", DoctorStatus::Fail),
        ];
        assert!(
            doctor_fix_action(&checks).is_none(),
            "no known-safe fix for these -> None"
        );
    }

    #[test]
    fn test_handoff_hint_single_warn_names_count_not_fix() {
        // Was `..._mentions_fix` until #1074: that test pinned the bug. No
        // doctor row is a check `/fix` re-runs, so the hint must not name it.
        let hint = doctor_handoff_hint_from_counts(0, 1).expect("1 warn -> Some");
        assert_eq!(hint, "→ 1 warning. See each row above for details.");
        assert!(!hint.contains("/fix"), "no /fix pointer: {hint}");
    }

    #[test]
    fn test_handoff_hint_multiple_fails_names_count() {
        let hint = doctor_handoff_hint_from_counts(2, 0).expect("2 fails -> Some");
        assert_eq!(hint, "→ 2 checks failed. See each row above for details.");
    }

    /// #1074: the footer as a table over real doctor rows. `print_doctor_report`
    /// is the one renderer behind both the REPL `/doctor` and the shell
    /// `yoyo doctor` (dispatch_sub.rs calls the same `handle_doctor`), so one
    /// expected string per row set covers both surfaces, and the property that
    /// makes that safe is pinned separately below: the hint names no command.
    #[test]
    fn test_handoff_hint_table_over_real_rows() {
        use DoctorStatus::*;
        let fresh_dir_trio = vec![
            named_check("Version", Pass),
            named_check("Config file", Warn),
            named_check("Project context", Warn),
            named_check("Memory dir", Warn),
        ];
        let missing_tool_and_key = vec![
            named_check("API key", Fail),
            named_check("Git", Fail),
            named_check("Config file", Warn),
        ];
        let one_of_each = vec![
            named_check("Git", Fail),
            named_check("Curl", Warn),
            named_check("RTK", Pass),
        ];
        let all_pass = vec![named_check("Version", Pass), named_check("Git", Pass)];
        let table: Vec<(&str, Vec<DoctorCheck>, Option<&str>)> = vec![
            (
                "fresh git-init dir: three warnings",
                fresh_dir_trio,
                Some("→ 3 warnings. See each row above for details."),
            ),
            (
                "fails and a warning",
                missing_tool_and_key,
                Some("→ 2 checks failed, 1 warning. See each row above for details."),
            ),
            (
                "singular both",
                one_of_each,
                Some("→ 1 check failed, 1 warning. See each row above for details."),
            ),
            // Near-miss / whole regression surface: a green run prints no
            // footer at all, exactly as before #1074.
            ("all pass", all_pass, None),
            ("no rows", vec![], None),
        ];
        for (label, rows, want) in table {
            let got = doctor_handoff_hint(&rows);
            assert_eq!(got.as_deref(), want, "row set: {label}");
        }
    }

    /// #1074: no doctor row is a build/lint/test check (those are what `/fix`
    /// re-runs), and the shell `yoyo doctor` reader cannot type a slash
    /// command, so the footer names no command at all, for any status mix.
    #[test]
    fn test_handoff_hint_never_names_a_command() {
        for fails in 0..4 {
            for warns in 0..4 {
                let Some(hint) = doctor_handoff_hint_from_counts(fails, warns) else {
                    assert_eq!((fails, warns), (0, 0), "only all-green is silent");
                    continue;
                };
                for cmd in ["/fix", "/health", "/init", "/remember", "yoyo "] {
                    assert!(
                        !hint.contains(cmd),
                        "footer must not name {cmd} ({fails} fail, {warns} warn): {hint}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_handoff_hint_counts_warn_and_fail_only() {
        let checks = vec![
            check(DoctorStatus::Pass),
            check(DoctorStatus::Warn),
            check(DoctorStatus::Fail),
            check(DoctorStatus::Pass),
        ];
        let hint = doctor_handoff_hint(&checks).expect("2 non-pass -> Some");
        // Since #1074 the two non-Pass kinds are counted apart, and the two
        // Pass rows are in neither count.
        assert_eq!(
            hint, "→ 1 check failed, 1 warning. See each row above for details.",
            "should count only Warn+Fail"
        );
    }

    /// #1075: each row's advice must be runnable on the surface it is printed
    /// on. Shell gets `yoyo init`; the REPL text is byte-identical to before.
    #[test]
    fn test_no_context_file_detail_per_surface() {
        let shell = no_context_file_detail(DoctorSurface::Shell);
        assert_eq!(shell, "no context file (create YOYO.md or run yoyo init)");
        assert!(!shell.contains("/init"), "{shell}");
        // Near-miss: the REPL user's row is unchanged.
        assert_eq!(
            no_context_file_detail(DoctorSurface::Repl),
            "no context file (create YOYO.md or run /init)"
        );
    }

    /// #1075: a bare `/remember` creates nothing and the shell has no
    /// `remember`, so both surfaces name `mkdir -p .yoyo`, the same command
    /// `doctor_fix_action` offers for this row.
    #[test]
    fn test_memory_dir_missing_detail_names_the_offered_fix() {
        let shell = memory_dir_missing_detail(DoctorSurface::Shell);
        let repl = memory_dir_missing_detail(DoctorSurface::Repl);
        assert_eq!(shell, ".yoyo/ not found (mkdir -p .yoyo to create)");
        assert_eq!(
            repl,
            ".yoyo/ not found (mkdir -p .yoyo to create, or /remember <note>)"
        );
        assert!(!shell.contains("/remember"), "{shell}");
        let row = vec![named_check("Memory dir", DoctorStatus::Warn)];
        let fix = doctor_fix_action(&row).expect("memory dir warn offers a fix");
        for detail in [shell, repl] {
            assert!(!detail.contains("run /remember to create"), "{detail}");
            assert!(detail.contains(&fix.command), "{detail} vs {}", fix.command);
        }
    }

    #[test]
    fn test_handoff_hint_is_product_safe() {
        // No language/toolchain assumptions in the hint text.
        let hint = doctor_handoff_hint_from_counts(1, 2).expect("Some");
        let lower = hint.to_lowercase();
        for forbidden in ["cargo", "clippy", "rust", "ci", "npm"] {
            assert!(
                !lower.contains(forbidden),
                "hint must be product-safe (no '{forbidden}'): {hint}"
            );
        }
    }
}
