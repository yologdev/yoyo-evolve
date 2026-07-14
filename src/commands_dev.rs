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
pub fn run_doctor_checks(provider: &str, model: &str) -> Vec<DoctorCheck> {
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
    let context_files = cli::list_project_context_files();
    if context_files.is_empty() {
        checks.push(DoctorCheck {
            name: "Project context".to_string(),
            status: DoctorStatus::Warn,
            detail: "no context file (create YOYO.md or run /init)".to_string(),
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
            detail: ".yoyo/ not found (run /remember to create)".to_string(),
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

/// Sum the byte sizes of every `SKILL.md` under the standard skill-discovery
/// directories (`.yoyo/skills/` project-local and `~/.yoyo/skills/` global).
///
/// Product-safe: returns 0 when no skill dirs exist (any project, any setup).
fn discovered_skill_bytes() -> usize {
    let mut total = 0usize;
    let mut dirs: Vec<std::path::PathBuf> = vec![std::path::PathBuf::from(".yoyo/skills")];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(std::path::Path::new(&home).join(".yoyo/skills"));
    }
    for dir in dirs {
        total += skill_bytes_in_dir(&dir);
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

/// Build a contextual handoff hint from a slice of doctor checks.
///
/// Contextual guidance beats reference guidance: after `/doctor` reports its
/// checks, if anything is Warn or Fail, point the user at the command that
/// acts on it instead of leaving them to find `/fix` and `/health` on their
/// own. Returns `None` on a fully-green run so the clean output stays pristine.
///
/// The wording is deliberately product-safe — no cargo/clippy/CI assumptions,
/// since the checks themselves already adapt per project type.
pub fn doctor_handoff_hint(checks: &[DoctorCheck]) -> Option<String> {
    let issues = checks
        .iter()
        .filter(|c| c.status != DoctorStatus::Pass)
        .count();
    doctor_handoff_hint_from_count(issues)
}

/// Format the handoff hint from a raw issue count (Warn + Fail).
///
/// Split out so it's unit-testable without constructing `DoctorCheck`s.
fn doctor_handoff_hint_from_count(issues: usize) -> Option<String> {
    if issues == 0 {
        return None;
    }
    let noun = if issues == 1 { "issue" } else { "issues" };
    Some(format!(
        "→ {issues} {noun} found. Try /fix to attempt repairs, or /health for a full check."
    ))
}

/// Handle the `/doctor` command.
pub fn handle_doctor(provider: &str, model: &str) {
    let checks = run_doctor_checks(provider, model);
    print_doctor_report(&checks);
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

/// Run health checks for a specific project type. Returns (name, passed, detail) tuples.
pub fn run_health_check_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let start = std::time::Instant::now();
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        let elapsed = format_duration(start.elapsed());
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, format!("ok ({elapsed})")));
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let first_line = stderr.lines().next().unwrap_or("(unknown error)");
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
pub fn run_health_checks_full_output(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, String::new()));
            }
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                let mut full_output = String::new();
                if !stdout.is_empty() {
                    full_output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !full_output.is_empty() {
                        full_output.push('\n');
                    }
                    full_output.push_str(&stderr);
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

pub fn handle_health() {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return;
    }
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_check_for_project(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return;
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
        let checks = run_doctor_checks("anthropic", "test-model");
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
        // run_health_check_for_project skips "cargo test" under #[cfg(test)] to avoid recursion
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_check_for_project(&project_type);
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
        // In a Rust project, should return results with full error output
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_checks_full_output(&project_type);
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
        let checks = run_doctor_checks("anthropic", "test-model");
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
        assert_eq!(doctor_handoff_hint_from_count(0), None);
        let checks = vec![check(DoctorStatus::Pass), check(DoctorStatus::Pass)];
        assert_eq!(doctor_handoff_hint(&checks), None);
    }

    #[test]
    fn test_handoff_hint_single_warn_mentions_fix() {
        let hint = doctor_handoff_hint_from_count(1).expect("1 issue -> Some");
        assert!(hint.contains("/fix"), "hint should point at /fix: {hint}");
        assert!(
            hint.contains("/health"),
            "hint should point at /health: {hint}"
        );
        assert!(hint.contains('1'), "hint should name the count: {hint}");
        // singular noun
        assert!(hint.contains("1 issue found"), "singular noun: {hint}");
    }

    #[test]
    fn test_handoff_hint_multiple_fails_names_count() {
        let hint = doctor_handoff_hint_from_count(2).expect("2 issues -> Some");
        assert!(
            hint.contains('2'),
            "hint should mention issue count = 2: {hint}"
        );
        assert!(hint.contains("2 issues found"), "plural noun: {hint}");
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
        assert!(hint.contains('2'), "should count only Warn+Fail: {hint}");
    }

    #[test]
    fn test_handoff_hint_is_product_safe() {
        // No language/toolchain assumptions in the hint text.
        let hint = doctor_handoff_hint_from_count(3).expect("Some");
        let lower = hint.to_lowercase();
        for forbidden in ["cargo", "clippy", "rust", "ci", "npm"] {
            assert!(
                !lower.contains(forbidden),
                "hint must be product-safe (no '{forbidden}'): {hint}"
            );
        }
    }
}
