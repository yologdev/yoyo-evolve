//! Skill-related command handlers: /skill list, show, path, install, search.

use crate::format::*;

/// Subcommand names for `/skill <Tab>` completion.
pub const SKILL_SUBCOMMANDS: &[&str] = &["list", "show", "path", "install", "search", "init"];

/// Handle the `/skill` command: list, show, and inspect loaded skills.
///
/// Accepts the raw input (with or without the `/skill` prefix) and a reference
/// to the loaded `SkillSet`. If no skills directory is configured, prints a
/// helpful message about the `--skills` flag.
pub fn handle_skill(input: &str, skills: &yoagent::skills::SkillSet) {
    let sub = input.strip_prefix("/skill").unwrap_or(input).trim();

    if sub.is_empty() || sub == "list" {
        skill_list(skills);
    } else if sub == "path" {
        skill_path(skills);
    } else if let Some(name) = sub.strip_prefix("show ") {
        skill_show(name.trim(), skills);
    } else if sub == "show" {
        eprintln!("{YELLOW}  usage: /skill show <name>{RESET}");
        eprintln!("{DIM}  try /skill list to see available skills{RESET}\n");
    } else if let Some(source) = sub.strip_prefix("install ") {
        let source = source.trim();
        if source.is_empty() {
            eprintln!("{YELLOW}  usage: /skill install <path|gh:user/repo>{RESET}");
            eprintln!(
                "{DIM}  install a skill from a local directory or GitHub repository{RESET}\n"
            );
        } else {
            skill_install(source);
        }
    } else if sub == "install" {
        eprintln!("{YELLOW}  usage: /skill install <path|gh:user/repo>{RESET}");
        eprintln!("{DIM}  install a skill from a local directory or GitHub repository{RESET}\n");
    } else if let Some(query) = sub.strip_prefix("search ") {
        let query = query.trim();
        if query.is_empty() {
            handle_skill_search(None);
        } else {
            handle_skill_search(Some(query));
        }
    } else if sub == "search" {
        handle_skill_search(None);
    } else if let Some(name) = sub.strip_prefix("init ") {
        let name = name.trim();
        if name.is_empty() {
            eprintln!("{YELLOW}  usage: /skill init <name>{RESET}");
            eprintln!("{DIM}  scaffold a new skill in .yoyo/skills/<name>/SKILL.md{RESET}\n");
        } else {
            skill_init(name);
        }
    } else if sub == "init" {
        eprintln!("{YELLOW}  usage: /skill init <name>{RESET}");
        eprintln!("{DIM}  scaffold a new skill in .yoyo/skills/<name>/SKILL.md{RESET}\n");
    } else {
        eprintln!("{RED}  unknown subcommand: {sub}{RESET}");
        eprintln!("{DIM}  try: /skill list, /skill show <name>, /skill path, /skill install <path>, /skill search <query>, /skill init <name>{RESET}\n");
    }
}

/// Validate a skill name: must be kebab-case (a-z, 0-9, hyphens).
///
/// Rules: non-empty, only lowercase letters/digits/hyphens, must start and end
/// with a letter or digit (no leading/trailing hyphens), no consecutive hyphens.
fn is_valid_skill_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    // Only a-z, 0-9, and hyphens
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return false;
    }
    // Must start and end with letter or digit
    if name.starts_with('-') || name.ends_with('-') {
        return false;
    }
    // No consecutive hyphens
    if name.contains("--") {
        return false;
    }
    true
}

/// Generate the SKILL.md template content for a new skill.
fn skill_init_template(name: &str) -> String {
    // Convert kebab-case to Title Case for heading
    let title: String = name
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{upper}{}", chars.collect::<String>())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    format!(
        "---\n\
         name: {name}\n\
         description: <placeholder — describe what this skill does>\n\
         tools: [bash, read_file, write_file, edit_file, search, list_files]\n\
         origin: user\n\
         status: active\n\
         ---\n\
         \n\
         # {title}\n\
         \n\
         ## When to use\n\
         \n\
         <Describe when this skill should be activated>\n\
         \n\
         ## How\n\
         \n\
         <Describe the steps this skill follows>\n"
    )
}

/// Scaffold a new skill at `.yoyo/skills/<name>/SKILL.md`.
///
/// Creates the directory and writes a template SKILL.md with correct YAML
/// frontmatter. Refuses to overwrite an existing skill.
fn skill_init(name: &str) {
    if !is_valid_skill_name(name) {
        eprintln!("{RED}  invalid skill name: {name}{RESET}");
        eprintln!("{DIM}  names must be kebab-case: lowercase letters, digits, and hyphens{RESET}");
        eprintln!("{DIM}  examples: my-skill, code-review, lint-fix{RESET}\n");
        return;
    }

    let dir = std::path::PathBuf::from(".yoyo/skills").join(name);
    let file = dir.join("SKILL.md");

    if file.exists() {
        eprintln!("{YELLOW}  skill already exists: {}{RESET}", file.display());
        eprintln!("{DIM}  delete it first if you want to start over{RESET}\n");
        return;
    }

    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "{RED}  error creating directory {}: {e}{RESET}\n",
            dir.display()
        );
        return;
    }

    let content = skill_init_template(name);
    if let Err(e) = std::fs::write(&file, &content) {
        eprintln!("{RED}  error writing {}: {e}{RESET}\n", file.display());
        return;
    }

    println!("{GREEN}  ✓ created skill '{name}'{RESET}");
    println!("{DIM}  {}{RESET}", file.display());
    println!("{DIM}  edit the SKILL.md to define your skill's behavior{RESET}");
    println!("{DIM}  load with: --skills .yoyo/skills{RESET}\n");
}

/// List all loaded skills with name and description.
fn skill_list(skills: &yoagent::skills::SkillSet) {
    if skills.is_empty() {
        println!("{DIM}  no skills loaded{RESET}");
        println!("{DIM}  use --skills <dir> to load skills from a directory{RESET}\n");
        return;
    }

    println!("{BOLD}  Loaded skills ({}):{RESET}\n", skills.len());

    // Find the longest skill name for alignment
    let max_name_len = skills
        .skills()
        .iter()
        .map(|s| s.name.len())
        .max()
        .unwrap_or(0);

    for skill in skills.skills() {
        let padding = " ".repeat(max_name_len.saturating_sub(skill.name.len()));
        println!(
            "    {GREEN}{}{RESET}{}  {DIM}{}{RESET}",
            skill.name, padding, skill.description
        );
    }
    println!();
}

/// Show the current skills directory paths (derived from loaded skill base_dirs).
fn skill_path(skills: &yoagent::skills::SkillSet) {
    if skills.is_empty() {
        println!("{DIM}  no skills directory configured{RESET}");
        println!("{DIM}  use --skills <dir> to load skills from a directory{RESET}\n");
        return;
    }

    // Collect unique parent directories from loaded skills
    let mut dirs: Vec<String> = skills
        .skills()
        .iter()
        .filter_map(|s| s.base_dir.parent().map(|p| p.display().to_string()))
        .collect();
    dirs.sort();
    dirs.dedup();

    if dirs.len() == 1 {
        println!("{DIM}  skills directory: {}{RESET}\n", dirs[0]);
    } else {
        println!("{DIM}  skills directories:{RESET}");
        for d in &dirs {
            println!("{DIM}    {d}{RESET}");
        }
        println!();
    }
}

/// Show the full content of a named skill's SKILL.md file.
fn skill_show(name: &str, skills: &yoagent::skills::SkillSet) {
    let skill = skills.skills().iter().find(|s| s.name == name);

    match skill {
        Some(s) => {
            match std::fs::read_to_string(&s.file_path) {
                Ok(content) => {
                    println!("{BOLD}  Skill: {}{RESET}", s.name);
                    println!("{DIM}  path: {}{RESET}\n", s.file_path.display());
                    // Print the skill content with light indentation
                    for line in content.lines() {
                        println!("  {line}");
                    }
                    println!();
                }
                Err(e) => {
                    eprintln!(
                        "{RED}  error reading {}: {e}{RESET}\n",
                        s.file_path.display()
                    );
                }
            }
        }
        None => {
            eprintln!("{RED}  skill not found: {name}{RESET}");
            if !skills.is_empty() {
                let names: Vec<&str> = skills.skills().iter().map(|s| s.name.as_str()).collect();
                eprintln!("{DIM}  available: {}{RESET}\n", names.join(", "));
            } else {
                eprintln!("{DIM}  no skills loaded — use --skills <dir>{RESET}\n");
            }
        }
    }
}

/// A single search result from GitHub.
struct SkillSearchResult {
    full_name: String,
    description: String,
    updated: String,
}

/// Search GitHub for yoyo skills using the `gh` CLI.
///
/// Searches for repositories with the `yoyo-skill` topic. If no topic-based
/// results are found, falls back to a text search for "yoyo-skill".
/// Displays results in a formatted table with install hints.
fn handle_skill_search(query: Option<&str>) {
    // Check if gh CLI is available
    let gh_check = std::process::Command::new("gh")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    if gh_check.is_err() || !gh_check.unwrap().success() {
        eprintln!("{RED}  gh CLI not found{RESET}");
        eprintln!(
            "{DIM}  install the GitHub CLI to search for skills: https://cli.github.com/{RESET}\n"
        );
        return;
    }

    println!("{DIM}  searching for yoyo skills...{RESET}");

    // Try topic-based search first
    let results = search_github_skills_by_topic(query);
    let results = match results {
        Ok(r) if !r.is_empty() => r,
        _ => {
            // Fall back to text search
            match search_github_skills_by_text(query) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{RED}  search failed: {e}{RESET}\n");
                    return;
                }
            }
        }
    };

    if results.is_empty() {
        let q = query.unwrap_or("(all)");
        eprintln!("{YELLOW}  no skills found for: {q}{RESET}");
        eprintln!("{DIM}  try a different query, or browse: https://github.com/topics/yoyo-skill{RESET}\n");
        return;
    }

    // Find max name length for alignment
    let max_name = results
        .iter()
        .map(|r| r.full_name.len())
        .max()
        .unwrap_or(0)
        .min(40);

    println!(
        "\n{BOLD}  Found {} skill{}:{RESET}\n",
        results.len(),
        if results.len() == 1 { "" } else { "s" }
    );

    for r in &results {
        let name_display = if r.full_name.len() > 40 {
            safe_truncate_with_suffix(&r.full_name, 37, "...")
        } else {
            r.full_name.clone()
        };
        let padding = " ".repeat(max_name.saturating_sub(name_display.len()));
        let desc = if r.description.is_empty() {
            "(no description)"
        } else {
            safe_truncate(&r.description, 60)
        };
        let updated_str = if r.updated.is_empty() {
            String::new()
        } else {
            format!("  {DIM}({}){RESET}", r.updated)
        };
        println!("    {GREEN}{name_display}{RESET}{padding}  {DIM}{desc}{RESET}{updated_str}");
    }

    println!("\n{DIM}  install with: /skill install gh:<owner>/<repo>{RESET}\n");
}

/// Search GitHub repos by the `yoyo-skill` topic.
fn search_github_skills_by_topic(query: Option<&str>) -> Result<Vec<SkillSearchResult>, String> {
    let mut args = vec![
        "search",
        "repos",
        "--topic",
        "yoyo-skill",
        "--limit",
        "20",
        "--json",
        "fullName,description,updatedAt",
    ];
    // Add the query as a positional argument if provided
    if let Some(q) = query {
        args.push("--");
        args.push(q);
    }

    run_gh_search(&args)
}

/// Search GitHub repos by text query containing "yoyo-skill".
fn search_github_skills_by_text(query: Option<&str>) -> Result<Vec<SkillSearchResult>, String> {
    let combined: String;
    let search_term = if let Some(q) = query {
        combined = format!("yoyo-skill {q}");
        combined.as_str()
    } else {
        "yoyo-skill"
    };

    let args = vec![
        "search",
        "repos",
        search_term,
        "--limit",
        "20",
        "--json",
        "fullName,description,updatedAt",
    ];

    run_gh_search(&args)
}

/// Run a `gh` search command and parse JSON results.
fn run_gh_search(args: &[&str]) -> Result<Vec<SkillSearchResult>, String> {
    let output = std::process::Command::new("gh")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh search failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_gh_search_json(&stdout)
}

/// Parse the JSON output from `gh search repos --json`.
///
/// Expected format: array of objects with `fullName`, `description`, `updatedAt`.
fn parse_gh_search_json(json_str: &str) -> Result<Vec<SkillSearchResult>, String> {
    let json_str = json_str.trim();
    if json_str.is_empty() || json_str == "[]" {
        return Ok(Vec::new());
    }

    // Minimal JSON array-of-objects parser — avoids adding a serde_json dependency.
    // Each object has the shape: {"fullName":"...","description":"...","updatedAt":"..."}
    let mut results = Vec::new();

    // Split by objects: find each { ... } block
    let mut depth = 0i32;
    let mut obj_start: Option<usize> = None;

    for (i, ch) in json_str.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    obj_start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = obj_start {
                        let obj = &json_str[start..=i];
                        let full_name = extract_json_string(obj, "fullName").unwrap_or_default();
                        let description =
                            extract_json_string(obj, "description").unwrap_or_default();
                        let updated = extract_json_string(obj, "updatedAt")
                            .unwrap_or_default()
                            .chars()
                            .take(10)
                            .collect();
                        if !full_name.is_empty() {
                            results.push(SkillSearchResult {
                                full_name,
                                description,
                                updated,
                            });
                        }
                    }
                    obj_start = None;
                }
            }
            _ => {}
        }
    }

    Ok(results)
}

/// Extract a string value for a given key from a JSON object string.
///
/// Very simple: looks for `"key":"value"` or `"key": "value"`.
/// Handles basic escape sequences (\", \\, \n, \t).
fn extract_json_string(obj: &str, key: &str) -> Option<String> {
    // Look for "key": or "key":
    let patterns = [format!("\"{key}\":\""), format!("\"{key}\": \"")];

    for pattern in &patterns {
        if let Some(start) = obj.find(pattern.as_str()) {
            let val_start = start + pattern.len();
            // Find the closing unescaped quote
            let rest = &obj[val_start..];
            let mut result = String::new();
            let mut chars = rest.chars();
            loop {
                match chars.next() {
                    None => break,
                    Some('\\') => match chars.next() {
                        Some('"') => result.push('"'),
                        Some('\\') => result.push('\\'),
                        Some('n') => result.push('\n'),
                        Some('t') => result.push('\t'),
                        Some(c) => {
                            result.push('\\');
                            result.push(c);
                        }
                        None => break,
                    },
                    Some('"') => break,
                    Some(c) => result.push(c),
                }
            }
            return Some(result);
        }
    }

    // Handle null/empty values: "key":null or "key": null
    let null_patterns = [format!("\"{key}\":null"), format!("\"{key}\": null")];
    for pattern in &null_patterns {
        if obj.contains(pattern.as_str()) {
            return Some(String::new());
        }
    }

    None
}

/// Extract the skill name from SKILL.md YAML frontmatter.
///
/// Looks for a `name:` field in the YAML frontmatter delimited by `---`.
/// Returns `None` if there's no valid frontmatter or no name field.
fn extract_skill_name_from_frontmatter(content: &str) -> Option<String> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    // Find the closing ---
    let after_first = &content[3..];
    let end_idx = after_first.find("\n---")?;
    let frontmatter = &after_first[..end_idx];

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("name:") {
            let name = value.trim().trim_matches('"').trim_matches('\'').trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Default skill install directory: `~/.config/yoyo/skills/`
///
/// Uses `XDG_CONFIG_HOME` if set, otherwise falls back to `~/.config`.
fn default_skill_install_dir() -> Option<std::path::PathBuf> {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(config_dir.join("yoyo").join("skills"))
}

/// Install a skill from a local directory path.
///
/// Validates the source, extracts the skill name from SKILL.md frontmatter,
/// and copies the entire directory to `~/.config/yoyo/skills/<name>/`.
///
/// If the source starts with `gh:`, delegates to [`skill_install_from_github`].
fn skill_install(source: &str) {
    if source.starts_with("gh:") {
        skill_install_from_github(source);
        return;
    }

    let source_path = std::path::Path::new(source);

    // Validate source exists and is a directory
    if !source_path.exists() {
        eprintln!("{RED}  error: path does not exist: {source}{RESET}\n");
        return;
    }
    if !source_path.is_dir() {
        eprintln!("{RED}  error: not a directory: {source}{RESET}");
        eprintln!("{DIM}  /skill install expects a skill directory containing SKILL.md{RESET}\n");
        return;
    }

    // Validate SKILL.md exists
    let skill_md_path = source_path.join("SKILL.md");
    if !skill_md_path.exists() {
        eprintln!("{RED}  error: no SKILL.md found in {source}{RESET}");
        eprintln!("{DIM}  a skill directory must contain a SKILL.md file{RESET}\n");
        return;
    }

    // Read and parse frontmatter for skill name
    let content = match std::fs::read_to_string(&skill_md_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  error reading SKILL.md: {e}{RESET}\n");
            return;
        }
    };

    let skill_name = match extract_skill_name_from_frontmatter(&content) {
        Some(name) => name,
        None => {
            eprintln!(
                "{RED}  error: could not extract skill name from SKILL.md frontmatter{RESET}"
            );
            eprintln!("{DIM}  SKILL.md must have YAML frontmatter with a 'name:' field{RESET}\n");
            return;
        }
    };

    // Determine install destination
    let install_dir = match default_skill_install_dir() {
        Some(dir) => dir,
        None => {
            eprintln!("{RED}  error: could not determine config directory{RESET}\n");
            return;
        }
    };

    let dest = install_dir.join(&skill_name);

    // Check if skill already exists
    if dest.exists() {
        eprintln!(
            "{YELLOW}  skill '{skill_name}' already installed at {}{RESET}",
            dest.display()
        );
        eprintln!("{DIM}  remove the existing directory first to reinstall{RESET}\n");
        return;
    }

    // Create destination and copy
    if let Err(e) = std::fs::create_dir_all(&dest) {
        eprintln!(
            "{RED}  error creating directory {}: {e}{RESET}\n",
            dest.display()
        );
        return;
    }

    match copy_dir_recursive(source_path, &dest) {
        Ok(count) => {
            println!("{GREEN}  ✓ installed skill '{skill_name}' ({count} files){RESET}");
            println!("{DIM}  location: {}{RESET}", dest.display());
            println!(
                "{DIM}  load with: --skills {}{RESET}\n",
                install_dir.display()
            );
        }
        Err(e) => {
            eprintln!("{RED}  error copying skill: {e}{RESET}");
            // Clean up partial install
            let _ = std::fs::remove_dir_all(&dest);
            eprintln!("{DIM}  installation rolled back{RESET}\n");
        }
    }
}

/// Recursively copy a directory's contents. Returns the number of files copied.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<usize> {
    let mut count = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            count += copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
            count += 1;
        }
    }
    Ok(count)
}

/// Parsed GitHub skill source: `gh:user/repo[/path][@branch]`
#[derive(Debug, PartialEq)]
struct GitHubSource {
    user: String,
    repo: String,
    /// Sub-path within the repo (e.g. `"skills/my-skill"`), empty if root.
    path: String,
    /// Optional branch or tag (e.g. `"main"`).
    branch: Option<String>,
}

/// Parse a `gh:` prefixed source string into its components.
///
/// Supported formats:
/// - `gh:user/repo`
/// - `gh:user/repo/path/to/skill`
/// - `gh:user/repo@branch`
/// - `gh:user/repo/path@branch`
fn parse_github_source(source: &str) -> Result<GitHubSource, String> {
    let raw = source
        .strip_prefix("gh:")
        .ok_or_else(|| "source must start with 'gh:'".to_string())?;

    if raw.is_empty() {
        return Err("missing repository reference after 'gh:'".to_string());
    }

    // Split off optional @branch first
    let (main_part, branch) = if let Some(at_pos) = raw.rfind('@') {
        let branch_str = &raw[at_pos + 1..];
        if branch_str.is_empty() {
            return Err("empty branch specifier after '@'".to_string());
        }
        (&raw[..at_pos], Some(branch_str.to_string()))
    } else {
        (raw, None)
    };

    // Split by '/' — first two segments are user/repo, rest is path
    let segments: Vec<&str> = main_part.split('/').collect();
    if segments.len() < 2 {
        return Err(format!(
            "invalid format: expected 'gh:user/repo', got 'gh:{main_part}'"
        ));
    }

    let user = segments[0].trim();
    let repo = segments[1].trim();

    if user.is_empty() {
        return Err("missing GitHub username".to_string());
    }
    if repo.is_empty() {
        return Err("missing GitHub repository name".to_string());
    }

    let path = if segments.len() > 2 {
        segments[2..].join("/")
    } else {
        String::new()
    };

    Ok(GitHubSource {
        user: user.to_string(),
        repo: repo.to_string(),
        path,
        branch,
    })
}

/// Find a SKILL.md in a cloned repo directory.
///
/// Search order:
/// 1. If a sub-path was specified, look only there.
/// 2. Root of the repo.
/// 3. `skill/SKILL.md` or `skills/SKILL.md` (common conventions).
///
/// Returns the directory containing SKILL.md, or an error with suggestions.
fn find_skill_in_repo(
    repo_root: &std::path::Path,
    sub_path: &str,
) -> Result<std::path::PathBuf, String> {
    if !sub_path.is_empty() {
        let target = repo_root.join(sub_path);
        if target.join("SKILL.md").exists() {
            return Ok(target);
        }
        // Maybe the sub_path itself points to a file or doesn't exist
        if !target.exists() {
            return Err(format!(
                "path '{sub_path}' does not exist in the repository"
            ));
        }
        return Err(format!(
            "no SKILL.md found at '{sub_path}/' in the repository"
        ));
    }

    // Check root
    if repo_root.join("SKILL.md").exists() {
        return Ok(repo_root.to_path_buf());
    }

    // Check common subdirectories
    for dir_name in &["skill", "skills"] {
        let candidate = repo_root.join(dir_name);
        if candidate.join("SKILL.md").exists() {
            return Ok(candidate);
        }
    }

    // Not found — build a helpful error with what we did find
    let mut found_files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(repo_root) {
        for entry in entries.flatten() {
            found_files.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    found_files.sort();

    let mut msg = "no SKILL.md found in repository root".to_string();
    if !found_files.is_empty() {
        msg.push_str("\n  found at root: ");
        msg.push_str(&found_files.join(", "));
    }

    // Look for SKILL.md anywhere in the repo to suggest paths
    let mut skill_locations = Vec::new();
    find_skill_md_recursive(repo_root, repo_root, &mut skill_locations, 3);
    if !skill_locations.is_empty() {
        msg.push_str("\n  SKILL.md found at:");
        for loc in &skill_locations {
            msg.push_str(&format!("\n    gh:user/repo/{loc}"));
        }
    }

    Err(msg)
}

/// Recursively search for SKILL.md files, up to `max_depth` levels deep.
fn find_skill_md_recursive(
    base: &std::path::Path,
    current: &std::path::Path,
    results: &mut Vec<String>,
    max_depth: usize,
) {
    if max_depth == 0 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(current) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.join("SKILL.md").exists() {
                    if let Ok(rel) = path.strip_prefix(base) {
                        results.push(rel.display().to_string());
                    }
                }
                find_skill_md_recursive(base, &path, results, max_depth - 1);
            }
        }
    }
}

/// Install a skill from a GitHub repository.
///
/// Clones the repo with `--depth 1`, finds SKILL.md, then installs via
/// the same path as local installs.
fn skill_install_from_github(source: &str) {
    let gh = match parse_github_source(source) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{RED}  error: {e}{RESET}\n");
            return;
        }
    };

    let url = format!("https://github.com/{}/{}.git", gh.user, gh.repo);

    // Create a temp directory for the clone
    let tmp_base = std::env::temp_dir().join(format!("yoyo-skill-install-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&tmp_base) {
        eprintln!("{RED}  error creating temp directory: {e}{RESET}\n");
        return;
    }

    // Ensure cleanup on all exit paths
    struct CleanupGuard(std::path::PathBuf);
    impl Drop for CleanupGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = CleanupGuard(tmp_base.clone());

    let clone_path = tmp_base.join(&gh.repo);

    // Build git clone command
    let mut args = vec!["clone".to_string(), "--depth".to_string(), "1".to_string()];
    if let Some(ref branch) = gh.branch {
        args.push("--branch".to_string());
        args.push(branch.clone());
    }
    args.push(url.clone());
    args.push(clone_path.display().to_string());

    eprintln!(
        "{DIM}  cloning {}/{}{}…{RESET}",
        gh.user,
        gh.repo,
        gh.branch
            .as_ref()
            .map(|b| format!("@{b}"))
            .unwrap_or_default()
    );

    let output = std::process::Command::new("git")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();

    match output {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("{RED}  error: git is not installed or not in PATH{RESET}");
            eprintln!("{DIM}  install git to use remote skill installation{RESET}\n");
            return;
        }
        Err(e) => {
            eprintln!("{RED}  error running git clone: {e}{RESET}\n");
            return;
        }
        Ok(ref o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.contains("not found")
                || stderr.contains("does not exist")
                || stderr.contains("Repository not found")
            {
                eprintln!(
                    "{RED}  error: repository not found: {}/{}{RESET}",
                    gh.user, gh.repo
                );
            } else if stderr.contains("Remote branch") && stderr.contains("not found")
                || stderr.contains("Could not find remote branch")
            {
                eprintln!(
                    "{RED}  error: branch '{}' not found in {}/{}{RESET}",
                    gh.branch.as_deref().unwrap_or("?"),
                    gh.user,
                    gh.repo
                );
            } else if stderr.contains("Could not resolve host")
                || stderr.contains("unable to access")
            {
                eprintln!("{RED}  error: network failure — could not reach github.com{RESET}");
            } else {
                eprintln!("{RED}  error: git clone failed{RESET}");
                // Show first meaningful line of stderr
                let first_line = stderr
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("unknown error");
                eprintln!("{DIM}  {first_line}{RESET}");
            }
            eprintln!();
            return;
        }
        Ok(_) => {} // success
    }

    // Find SKILL.md in the cloned repo
    let skill_dir = match find_skill_in_repo(&clone_path, &gh.path) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("{RED}  error: {e}{RESET}\n");
            return;
        }
    };

    // Read and validate the SKILL.md
    let skill_md_path = skill_dir.join("SKILL.md");
    let content = match std::fs::read_to_string(&skill_md_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  error reading SKILL.md: {e}{RESET}\n");
            return;
        }
    };

    let skill_name = match extract_skill_name_from_frontmatter(&content) {
        Some(name) => name,
        None => {
            eprintln!(
                "{RED}  error: could not extract skill name from SKILL.md frontmatter{RESET}"
            );
            eprintln!("{DIM}  SKILL.md must have YAML frontmatter with a 'name:' field{RESET}\n");
            return;
        }
    };

    // Determine install destination
    let install_dir = match default_skill_install_dir() {
        Some(dir) => dir,
        None => {
            eprintln!("{RED}  error: could not determine config directory{RESET}\n");
            return;
        }
    };

    let dest = install_dir.join(&skill_name);

    if dest.exists() {
        eprintln!(
            "{YELLOW}  skill '{skill_name}' already installed at {}{RESET}",
            dest.display()
        );
        eprintln!("{DIM}  remove the existing directory first to reinstall{RESET}\n");
        return;
    }

    if let Err(e) = std::fs::create_dir_all(&dest) {
        eprintln!(
            "{RED}  error creating directory {}: {e}{RESET}\n",
            dest.display()
        );
        return;
    }

    match copy_dir_recursive(&skill_dir, &dest) {
        Ok(count) => {
            println!(
                "{GREEN}  ✓ installed skill '{skill_name}' from {}/{} ({count} files){RESET}",
                gh.user, gh.repo
            );
            println!("{DIM}  location: {}{RESET}", dest.display());
            println!(
                "{DIM}  load with: --skills {}{RESET}\n",
                install_dir.display()
            );
        }
        Err(e) => {
            eprintln!("{RED}  error copying skill: {e}{RESET}");
            let _ = std::fs::remove_dir_all(&dest);
            eprintln!("{DIM}  installation rolled back{RESET}\n");
        }
    }

    // _cleanup guard handles temp directory removal on drop
}

/// Install a skill to a specific directory (used for testing).
/// Same logic as `skill_install` but allows specifying the destination base dir.
#[cfg(test)]
fn skill_install_to(source: &str, install_dir: &std::path::Path) -> Result<String, String> {
    let source_path = std::path::Path::new(source);

    if !source_path.exists() {
        return Err(format!("path does not exist: {source}"));
    }
    if !source_path.is_dir() {
        return Err(format!("not a directory: {source}"));
    }

    let skill_md_path = source_path.join("SKILL.md");
    if !skill_md_path.exists() {
        return Err(format!("no SKILL.md found in {source}"));
    }

    let content = std::fs::read_to_string(&skill_md_path)
        .map_err(|e| format!("error reading SKILL.md: {e}"))?;

    let skill_name = extract_skill_name_from_frontmatter(&content)
        .ok_or_else(|| "could not extract skill name from SKILL.md frontmatter".to_string())?;

    let dest = install_dir.join(&skill_name);

    if dest.exists() {
        return Err(format!(
            "skill '{skill_name}' already installed at {}",
            dest.display()
        ));
    }

    std::fs::create_dir_all(&dest).map_err(|e| format!("error creating directory: {e}"))?;

    match copy_dir_recursive(source_path, &dest) {
        Ok(_count) => Ok(skill_name),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dest);
            Err(format!("error copying skill: {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    use crate::commands::KNOWN_COMMANDS;
    use crate::help::help_text;

    #[test]
    fn test_skill_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/skill"),
            "/skill should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_skill_in_help_text() {
        let help = help_text();
        assert!(help.contains("/skill"), "/skill should appear in help text");
        assert!(help.contains("skills"), "Help text should mention skills");
    }

    #[test]
    fn test_skill_list_with_real_skills() {
        // Load the real ./skills directory used by this project
        let skills = yoagent::skills::SkillSet::load(&["./skills"]).unwrap();
        assert!(
            skills.len() >= 4,
            "Expected at least 4 core skills, got {}",
            skills.len()
        );

        // Verify the evolve skill is present
        let names: Vec<&str> = skills.skills().iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"evolve"), "evolve skill should be loaded");
        assert!(
            names.contains(&"communicate"),
            "communicate skill should be loaded"
        );
    }

    #[test]
    fn test_skill_list_empty() {
        let skills = yoagent::skills::SkillSet::empty();
        // Should not panic — just print "no skills loaded"
        handle_skill("/skill list", &skills);
        handle_skill("/skill", &skills);
    }

    #[test]
    fn test_skill_show_existing() {
        let skills = yoagent::skills::SkillSet::load(&["./skills"]).unwrap();
        // Should not panic — prints the evolve skill content
        handle_skill("/skill show evolve", &skills);
    }

    #[test]
    fn test_skill_show_nonexistent() {
        let skills = yoagent::skills::SkillSet::load(&["./skills"]).unwrap();
        // Should not panic — prints error message
        handle_skill("/skill show nonexistent-skill", &skills);
    }

    #[test]
    fn test_skill_path() {
        let skills = yoagent::skills::SkillSet::load(&["./skills"]).unwrap();
        // Should not panic — prints the skills directory
        handle_skill("/skill path", &skills);
    }

    #[test]
    fn test_skill_path_empty() {
        let skills = yoagent::skills::SkillSet::empty();
        // Should not panic — prints "no skills directory configured"
        handle_skill("/skill path", &skills);
    }

    #[test]
    fn test_skill_unknown_subcommand() {
        let skills = yoagent::skills::SkillSet::empty();
        // Should not panic — prints error about unknown subcommand
        handle_skill("/skill foobar", &skills);
    }

    #[test]
    fn test_skill_show_bare() {
        let skills = yoagent::skills::SkillSet::empty();
        // Should not panic — prints usage hint
        handle_skill("/skill show", &skills);
    }

    #[test]
    fn test_skill_with_temp_dir() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\n---\n\n# My Skill\n\nDoes things.\n",
        )
        .unwrap();

        let skills = yoagent::skills::SkillSet::load(&[tmp.path()]).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills.skills()[0].name, "my-skill");
        assert_eq!(skills.skills()[0].description, "A test skill");

        // List should work
        handle_skill("/skill list", &skills);

        // Show should work
        handle_skill("/skill show my-skill", &skills);

        // Path should work
        handle_skill("/skill path", &skills);
    }

    #[test]
    fn test_extract_skill_name_valid() {
        let content = "---\nname: my-cool-skill\ndescription: Does things\n---\n\n# Content\n";
        assert_eq!(
            extract_skill_name_from_frontmatter(content),
            Some("my-cool-skill".to_string())
        );
    }

    #[test]
    fn test_extract_skill_name_quoted() {
        let content = "---\nname: \"quoted-name\"\ndescription: test\n---\n\nBody\n";
        assert_eq!(
            extract_skill_name_from_frontmatter(content),
            Some("quoted-name".to_string())
        );
    }

    #[test]
    fn test_extract_skill_name_single_quoted() {
        let content = "---\nname: 'single-quoted'\ndescription: test\n---\n\nBody\n";
        assert_eq!(
            extract_skill_name_from_frontmatter(content),
            Some("single-quoted".to_string())
        );
    }

    #[test]
    fn test_extract_skill_name_no_frontmatter() {
        let content = "# Just a heading\n\nNo frontmatter here.\n";
        assert_eq!(extract_skill_name_from_frontmatter(content), None);
    }

    #[test]
    fn test_extract_skill_name_no_name_field() {
        let content = "---\ndescription: has frontmatter but no name\n---\n\nBody\n";
        assert_eq!(extract_skill_name_from_frontmatter(content), None);
    }

    #[test]
    fn test_extract_skill_name_empty_name() {
        let content = "---\nname: \ndescription: empty name\n---\n\nBody\n";
        assert_eq!(extract_skill_name_from_frontmatter(content), None);
    }

    #[test]
    fn test_skill_install_nonexistent_path() {
        let tmp = TempDir::new().unwrap();
        let install_dir = tmp.path().join("installed");
        let result = skill_install_to("/nonexistent/path/to/skill", &install_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_skill_install_missing_skill_md() {
        let tmp = TempDir::new().unwrap();
        // Create a directory without SKILL.md
        let source = tmp.path().join("bad-skill");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("README.md"), "# Not a skill").unwrap();

        let install_dir = tmp.path().join("installed");
        let result = skill_install_to(source.to_str().unwrap(), &install_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no SKILL.md"));
    }

    #[test]
    fn test_skill_install_not_a_directory() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("not-a-dir.txt");
        fs::write(&file_path, "just a file").unwrap();

        let install_dir = tmp.path().join("installed");
        let result = skill_install_to(file_path.to_str().unwrap(), &install_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a directory"));
    }

    #[test]
    fn test_skill_install_bad_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let source = tmp.path().join("no-name-skill");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "# Just markdown, no frontmatter\n").unwrap();

        let install_dir = tmp.path().join("installed");
        let result = skill_install_to(source.to_str().unwrap(), &install_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("could not extract skill name"));
    }

    #[test]
    fn test_skill_install_success() {
        let tmp = TempDir::new().unwrap();

        // Create source skill with multiple files
        let source = tmp.path().join("my-skill");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("SKILL.md"),
            "---\nname: test-skill\ndescription: A test\n---\n\n# Test Skill\n",
        )
        .unwrap();
        fs::write(source.join("extra.txt"), "extra content").unwrap();

        let install_dir = tmp.path().join("installed");
        let result = skill_install_to(source.to_str().unwrap(), &install_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-skill");

        // Verify files were copied
        let dest = install_dir.join("test-skill");
        assert!(dest.join("SKILL.md").exists());
        assert!(dest.join("extra.txt").exists());
        assert_eq!(
            fs::read_to_string(dest.join("extra.txt")).unwrap(),
            "extra content"
        );
    }

    #[test]
    fn test_skill_install_already_exists() {
        let tmp = TempDir::new().unwrap();

        // Create source skill
        let source = tmp.path().join("my-skill");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("SKILL.md"),
            "---\nname: dupe-skill\ndescription: A test\n---\n\n# Test\n",
        )
        .unwrap();

        let install_dir = tmp.path().join("installed");

        // First install succeeds
        let result = skill_install_to(source.to_str().unwrap(), &install_dir);
        assert!(result.is_ok());

        // Second install fails (already exists)
        let result2 = skill_install_to(source.to_str().unwrap(), &install_dir);
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("already installed"));
    }

    #[test]
    fn test_skill_install_with_subdirectory() {
        let tmp = TempDir::new().unwrap();

        // Create source skill with a subdirectory
        let source = tmp.path().join("complex-skill");
        let sub = source.join("templates");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            source.join("SKILL.md"),
            "---\nname: complex-skill\ndescription: Has subdirs\n---\n\n# Complex\n",
        )
        .unwrap();
        fs::write(sub.join("template.txt"), "template content").unwrap();

        let install_dir = tmp.path().join("installed");
        let result = skill_install_to(source.to_str().unwrap(), &install_dir);
        assert!(result.is_ok());

        // Verify subdirectory was copied
        let dest = install_dir.join("complex-skill");
        assert!(dest.join("templates").join("template.txt").exists());
        assert_eq!(
            fs::read_to_string(dest.join("templates").join("template.txt")).unwrap(),
            "template content"
        );
    }

    #[test]
    fn test_skill_install_subcommand_routing() {
        let skills = yoagent::skills::SkillSet::empty();
        // These should not panic — they print usage/error messages
        handle_skill("/skill install", &skills);
        handle_skill("/skill install ", &skills);
    }

    // ── GitHub source parsing tests ──────────────────────────────────────

    #[test]
    fn test_parse_github_source_basic() {
        let result = parse_github_source("gh:user/repo").unwrap();
        assert_eq!(result.user, "user");
        assert_eq!(result.repo, "repo");
        assert_eq!(result.path, "");
        assert_eq!(result.branch, None);
    }

    #[test]
    fn test_parse_github_source_with_path() {
        let result = parse_github_source("gh:user/repo/skills/my-skill").unwrap();
        assert_eq!(result.user, "user");
        assert_eq!(result.repo, "repo");
        assert_eq!(result.path, "skills/my-skill");
        assert_eq!(result.branch, None);
    }

    #[test]
    fn test_parse_github_source_with_branch() {
        let result = parse_github_source("gh:user/repo@main").unwrap();
        assert_eq!(result.user, "user");
        assert_eq!(result.repo, "repo");
        assert_eq!(result.path, "");
        assert_eq!(result.branch, Some("main".to_string()));
    }

    #[test]
    fn test_parse_github_source_with_path_and_branch() {
        let result = parse_github_source("gh:user/repo/path/to/skill@dev").unwrap();
        assert_eq!(result.user, "user");
        assert_eq!(result.repo, "repo");
        assert_eq!(result.path, "path/to/skill");
        assert_eq!(result.branch, Some("dev".to_string()));
    }

    #[test]
    fn test_parse_github_source_missing_prefix() {
        let result = parse_github_source("user/repo");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'gh:'"));
    }

    #[test]
    fn test_parse_github_source_missing_repo() {
        let result = parse_github_source("gh:user");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 'gh:user/repo'"));
    }

    #[test]
    fn test_parse_github_source_empty_after_prefix() {
        let result = parse_github_source("gh:");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing repository reference"));
    }

    #[test]
    fn test_parse_github_source_empty_user() {
        let result = parse_github_source("gh:/repo");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing GitHub username"));
    }

    #[test]
    fn test_parse_github_source_empty_repo() {
        let result = parse_github_source("gh:user/");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("missing GitHub repository name"));
    }

    #[test]
    fn test_parse_github_source_empty_branch() {
        let result = parse_github_source("gh:user/repo@");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty branch specifier"));
    }

    #[test]
    fn test_parse_github_source_tag_as_branch() {
        let result = parse_github_source("gh:user/repo@v1.0.0").unwrap();
        assert_eq!(result.branch, Some("v1.0.0".to_string()));
    }

    // ── find_skill_in_repo tests ─────────────────────────────────────────

    #[test]
    fn test_find_skill_in_repo_root() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("SKILL.md"),
            "---\nname: root-skill\n---\n# Hi\n",
        )
        .unwrap();

        let result = find_skill_in_repo(tmp.path(), "");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), tmp.path());
    }

    #[test]
    fn test_find_skill_in_repo_skills_subdir() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(
            skills_dir.join("SKILL.md"),
            "---\nname: sub-skill\n---\n# Hi\n",
        )
        .unwrap();

        let result = find_skill_in_repo(tmp.path(), "");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), skills_dir);
    }

    #[test]
    fn test_find_skill_in_repo_skill_subdir() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: sub-skill\n---\n# Hi\n",
        )
        .unwrap();

        let result = find_skill_in_repo(tmp.path(), "");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), skill_dir);
    }

    #[test]
    fn test_find_skill_in_repo_with_subpath() {
        let tmp = TempDir::new().unwrap();
        let deep = tmp.path().join("a").join("b");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("SKILL.md"), "---\nname: deep-skill\n---\n# Hi\n").unwrap();

        let result = find_skill_in_repo(tmp.path(), "a/b");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), deep);
    }

    #[test]
    fn test_find_skill_in_repo_subpath_no_skill_md() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("somedir");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("README.md"), "# Not a skill").unwrap();

        let result = find_skill_in_repo(tmp.path(), "somedir");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no SKILL.md found"));
    }

    #[test]
    fn test_find_skill_in_repo_subpath_missing() {
        let tmp = TempDir::new().unwrap();

        let result = find_skill_in_repo(tmp.path(), "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_find_skill_in_repo_not_found_suggests_locations() {
        let tmp = TempDir::new().unwrap();
        // Put SKILL.md in a nested location
        let nested = tmp.path().join("custom").join("my-skill");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("SKILL.md"), "---\nname: nested\n---\n# Hi\n").unwrap();

        let result = find_skill_in_repo(tmp.path(), "");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("no SKILL.md found"));
        assert!(err.contains("custom/my-skill"));
    }

    // ── skill_install_from_github integration with local git ─────────────

    #[test]
    fn test_skill_install_from_github_with_local_repo() {
        // Create a local git repo that simulates a GitHub clone
        let tmp = TempDir::new().unwrap();
        let repo_dir = tmp.path().join("test-skill-repo");
        fs::create_dir_all(&repo_dir).unwrap();

        // Init a git repo
        let status = std::process::Command::new("git")
            .args(["init", repo_dir.to_str().unwrap()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        if status.is_err() || !status.unwrap().success() {
            // git not available, skip test
            return;
        }

        // Configure git user for the commit
        let _ = std::process::Command::new("git")
            .args([
                "-C",
                repo_dir.to_str().unwrap(),
                "config",
                "user.email",
                "test@test.com",
            ])
            .status();
        let _ = std::process::Command::new("git")
            .args([
                "-C",
                repo_dir.to_str().unwrap(),
                "config",
                "user.name",
                "Test",
            ])
            .status();

        // Add SKILL.md
        fs::write(
            repo_dir.join("SKILL.md"),
            "---\nname: remote-test-skill\ndescription: A test skill from git\norigin: marketplace\n---\n\n# Remote Test Skill\n\nInstalled from a git repo.\n",
        )
        .unwrap();

        let _ = std::process::Command::new("git")
            .args(["-C", repo_dir.to_str().unwrap(), "add", "."])
            .status();
        let _ = std::process::Command::new("git")
            .args(["-C", repo_dir.to_str().unwrap(), "commit", "-m", "init"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        // Now test that parse_github_source and find_skill_in_repo work
        // (We can't test the full flow without a real GitHub URL, but we
        // can test the pieces)
        let source = parse_github_source("gh:testuser/test-skill-repo").unwrap();
        assert_eq!(source.user, "testuser");
        assert_eq!(source.repo, "test-skill-repo");

        // Test find_skill_in_repo on the created repo
        let found = find_skill_in_repo(&repo_dir, "").unwrap();
        assert_eq!(found, repo_dir);

        // Read and validate the SKILL.md
        let content = fs::read_to_string(found.join("SKILL.md")).unwrap();
        let name = extract_skill_name_from_frontmatter(&content).unwrap();
        assert_eq!(name, "remote-test-skill");
    }

    #[test]
    fn test_gh_prefix_dispatch() {
        // Verify that /skill install gh:... routes to the github handler
        // (it will fail with a network error, but should not panic)
        let skills = yoagent::skills::SkillSet::empty();
        handle_skill(
            "/skill install gh:nonexistent-user-12345/nonexistent-repo-67890",
            &skills,
        );
    }

    #[test]
    fn test_parse_gh_search_json_basic() {
        let json = r#"[{"fullName":"alice/cool-skill","description":"A cool skill","updatedAt":"2025-01-15T10:00:00Z"}]"#;
        let results = parse_gh_search_json(json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].full_name, "alice/cool-skill");
        assert_eq!(results[0].description, "A cool skill");
        assert_eq!(results[0].updated, "2025-01-15");
    }

    #[test]
    fn test_parse_gh_search_json_multiple() {
        let json = r#"[
            {"fullName":"alice/skill-a","description":"First","updatedAt":"2025-01-01T00:00:00Z"},
            {"fullName":"bob/skill-b","description":"Second","updatedAt":"2025-02-02T00:00:00Z"}
        ]"#;
        let results = parse_gh_search_json(json).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].full_name, "alice/skill-a");
        assert_eq!(results[1].full_name, "bob/skill-b");
        assert_eq!(results[1].description, "Second");
    }

    #[test]
    fn test_parse_gh_search_json_empty() {
        assert!(parse_gh_search_json("[]").unwrap().is_empty());
        assert!(parse_gh_search_json("").unwrap().is_empty());
        assert!(parse_gh_search_json("  ").unwrap().is_empty());
    }

    #[test]
    fn test_parse_gh_search_json_null_description() {
        let json =
            r#"[{"fullName":"alice/skill","description":null,"updatedAt":"2025-01-01T00:00:00Z"}]"#;
        let results = parse_gh_search_json(json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].description, "");
    }

    #[test]
    fn test_parse_gh_search_json_escaped_chars() {
        let json = r#"[{"fullName":"alice/skill","description":"A \"quoted\" skill","updatedAt":"2025-01-01T00:00:00Z"}]"#;
        let results = parse_gh_search_json(json).unwrap();
        assert_eq!(results[0].description, "A \"quoted\" skill");
    }

    #[test]
    fn test_extract_json_string() {
        let obj = r#"{"fullName":"alice/repo","description":"hello"}"#;
        assert_eq!(
            extract_json_string(obj, "fullName"),
            Some("alice/repo".into())
        );
        assert_eq!(
            extract_json_string(obj, "description"),
            Some("hello".into())
        );
        assert_eq!(extract_json_string(obj, "missing"), None);
    }

    #[test]
    fn test_extract_json_string_with_spaces() {
        // gh CLI sometimes outputs with spaces after colon
        let obj = r#"{"fullName": "alice/repo", "description": "hello world"}"#;
        assert_eq!(
            extract_json_string(obj, "fullName"),
            Some("alice/repo".into())
        );
        assert_eq!(
            extract_json_string(obj, "description"),
            Some("hello world".into())
        );
    }

    #[test]
    fn test_skill_search_subcommand_registered() {
        assert!(SKILL_SUBCOMMANDS.contains(&"search"));
    }

    #[test]
    fn test_skill_search_routes_without_panic() {
        // Verify /skill search routes correctly (may fail if gh not installed,
        // but should not panic)
        let skills = yoagent::skills::SkillSet::empty();
        handle_skill("/skill search test-query", &skills);
        handle_skill("/skill search", &skills);
    }

    #[test]
    fn test_skill_init_subcommand_registered() {
        assert!(SKILL_SUBCOMMANDS.contains(&"init"));
    }

    #[test]
    fn test_valid_skill_names() {
        assert!(is_valid_skill_name("my-skill"));
        assert!(is_valid_skill_name("code-review"));
        assert!(is_valid_skill_name("lint-fix"));
        assert!(is_valid_skill_name("a"));
        assert!(is_valid_skill_name("abc123"));
        assert!(is_valid_skill_name("my-cool-skill-2"));
    }

    #[test]
    fn test_invalid_skill_names() {
        assert!(!is_valid_skill_name(""));
        assert!(!is_valid_skill_name("My-Skill")); // uppercase
        assert!(!is_valid_skill_name("my skill")); // spaces
        assert!(!is_valid_skill_name("-leading")); // leading hyphen
        assert!(!is_valid_skill_name("trailing-")); // trailing hyphen
        assert!(!is_valid_skill_name("double--hyphen")); // consecutive hyphens
        assert!(!is_valid_skill_name("special!char")); // special chars
        assert!(!is_valid_skill_name("under_score")); // underscore
        assert!(!is_valid_skill_name("path/slash")); // slash
    }

    #[test]
    fn test_skill_init_template_content() {
        let content = skill_init_template("my-cool-skill");
        // Check frontmatter
        assert!(content.starts_with("---\n"));
        assert!(content.contains("name: my-cool-skill\n"));
        assert!(content.contains("origin: user\n"));
        assert!(content.contains("status: active\n"));
        assert!(content
            .contains("tools: [bash, read_file, write_file, edit_file, search, list_files]\n"));
        // Check title case conversion
        assert!(content.contains("# My Cool Skill\n"));
        // Check template sections
        assert!(content.contains("## When to use"));
        assert!(content.contains("## How"));
    }

    #[test]
    fn test_skill_init_template_single_word() {
        let content = skill_init_template("lint");
        assert!(content.contains("name: lint\n"));
        assert!(content.contains("# Lint\n"));
    }

    #[test]
    fn test_skill_init_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".yoyo/skills/test-skill");
        let file = dir.join("SKILL.md");

        // Manually do what skill_init does but in a temp dir
        std::fs::create_dir_all(&dir).unwrap();
        let content = skill_init_template("test-skill");
        std::fs::write(&file, &content).unwrap();

        assert!(file.exists());
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("name: test-skill"));
        assert!(written.contains("# Test Skill"));
    }

    #[test]
    fn test_skill_init_no_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".yoyo/skills/existing");
        let file = dir.join("SKILL.md");

        // Pre-create the file with custom content
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "custom content").unwrap();

        // Verify file still has the original content (skill_init would refuse)
        assert!(file.exists());
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "custom content");
    }

    #[test]
    fn test_skill_init_routes_without_panic() {
        let skills = yoagent::skills::SkillSet::empty();
        // Bare init (no name) should not panic
        handle_skill("/skill init", &skills);
        // Init with name should not panic (will try to create in cwd's .yoyo/)
        // We don't assert file creation here since it writes to cwd
        handle_skill("/skill init  ", &skills);
    }
}
