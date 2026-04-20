//! Help text and help command handlers for yoyo.
//!
//! Contains the detailed per-command help entries, the summary help listing,
//! and the `/help` command handlers. Extracted from `commands.rs` to keep
//! that module focused on command dispatch logic.

use crate::commands::KNOWN_COMMANDS;
use crate::format::*;

/// Return command names (without `/` prefix) for `/help <Tab>` completion.
pub fn help_command_completions(partial_lower: &str) -> Vec<String> {
    KNOWN_COMMANDS
        .iter()
        .map(|c| c.trim_start_matches('/'))
        // /exit is an alias for /quit — skip it for cleaner completion
        .filter(|name| *name != "exit")
        .filter(|name| name.to_lowercase().starts_with(partial_lower))
        .map(|name| name.to_string())
        .collect()
}

/// Return detailed help text for a specific command.
///
/// Accepts the command name without the leading `/` (e.g. `"add"`, `"commit"`).
/// Returns `None` for unknown commands.
pub fn command_help(cmd: &str) -> Option<&'static str> {
    match cmd {
        "add" => Some(
            "/add <path> — Inject file contents into the conversation\n\n\
             Usage:\n\
             \x20 /add <path>              Add entire file\n\
             \x20 /add <path>:<start>-<end> Add specific line range\n\
             \x20 /add src/*.rs            Add files matching a glob pattern\n\
             \x20 /add file1 file2         Add multiple files at once\n\n\
             Examples:\n\
             \x20 /add src/main.rs\n\
             \x20 /add Cargo.toml:1-20\n\
             \x20 /add src/*.rs tests/*.rs",
        ),
        "apply" => Some(
            "/apply [file] — Apply a diff or patch file\n\n\
             Usage:\n\
             \x20 /apply patch.diff          Apply a patch file\n\
             \x20 /apply --check patch.diff  Dry-run: show what would change\n\n\
             Uses `git apply` under the hood. Supports unified diff format.\n\n\
             Examples:\n\
             \x20 /apply fix.patch\n\
             \x20 /apply --check changes.diff",
        ),
        "bg" => Some(
            "/bg — Manage background shell processes\n\n\
             Subcommands:\n\
             \x20 /bg run <command>    Launch a command in the background\n\
             \x20 /bg list             Show all background jobs (default)\n\
             \x20 /bg output <id>      Show output of a job (last 50 lines)\n\
             \x20 /bg output <id> --all Show all output\n\
             \x20 /bg kill <id>        Kill a running job\n\n\
             Examples:\n\
             \x20 /bg run cargo build --release\n\
             \x20 /bg list\n\
             \x20 /bg output 1\n\
             \x20 /bg kill 1",
        ),
        "help" => Some(
            "/help [command] — Show help information\n\n\
             Usage:\n\
             \x20 /help              Show all available commands\n\
             \x20 /help <command>    Show detailed help for a specific command\n\n\
             Examples:\n\
             \x20 /help\n\
             \x20 /help add\n\
             \x20 /help commit",
        ),
        "quit" | "exit" => Some(
            "/quit — Exit yoyo\n\n\
             Aliases: /quit, /exit\n\n\
             Exits the interactive REPL. Unsaved session data will be lost\n\
             unless you /save first.",
        ),
        "clear" => Some(
            "/clear — Clear conversation history\n\n\
             Resets the conversation to a fresh state, removing all messages.\n\
             If the conversation has more than 4 messages, asks for confirmation.\n\
             The system prompt and loaded context are preserved.\n\
             Session cost tracking continues.\n\n\
             See also: /clear! (skip confirmation)",
        ),
        "clear!" => Some(
            "/clear! — Force-clear conversation history\n\n\
             Same as /clear but skips the confirmation prompt.\n\
             Always clears immediately regardless of message count.",
        ),
        "compact" => Some(
            "/compact — Compact conversation to save context space\n\n\
             Asks the AI to summarize the conversation so far into a shorter\n\
             representation, freeing up context window space. Useful when\n\
             approaching token limits on long sessions.",
        ),
        "commit" => Some(
            "/commit [message] — Commit staged changes\n\n\
             Usage:\n\
             \x20 /commit              AI generates a commit message from the diff\n\
             \x20 /commit <message>    Commit with the given message\n\n\
             Stages all changes and commits. If no message is provided, the AI\n\
             analyzes the diff and generates an appropriate commit message.\n\n\
             Examples:\n\
             \x20 /commit\n\
             \x20 /commit fix: resolve off-by-one in parser",
        ),
        "cost" => Some(
            "/cost — Show estimated session cost\n\n\
             Displays the running cost estimate for this session based on\n\
             input/output tokens and the current model's pricing. Supports\n\
             cost tracking across multiple providers.",
        ),
        "docs" => Some(
            "/docs <crate> [item] — Look up docs.rs documentation\n\n\
             Usage:\n\
             \x20 /docs <crate>          Show crate overview\n\
             \x20 /docs <crate> <item>   Look up a specific item\n\n\
             Fetches documentation from docs.rs for Rust crates.\n\n\
             Examples:\n\
             \x20 /docs serde\n\
             \x20 /docs tokio spawn",
        ),
        "doctor" => Some(
            "/doctor — Run environment diagnostics\n\n\
             Checks your development environment and reports what's working,\n\
             what's missing, and what might need attention.\n\n\
             Checks performed:\n\
             \x20 • Version — current yoyo version\n\
             \x20 • Git — whether git is installed and current repo/branch\n\
             \x20 • Provider — configured AI provider\n\
             \x20 • API key — whether the required env var is set\n\
             \x20 • Model — configured model name\n\
             \x20 • Config file — .yoyo.toml or ~/.config/yoyo/config.toml\n\
             \x20 • Project context — YOYO.md, CLAUDE.md, etc.\n\
             \x20 • Curl — needed for /docs and /web\n\
             \x20 • Memory dir — .yoyo/ for persistent memories\n\n\
             Run this when something isn't working to quickly identify the issue.",
        ),
        "find" => Some(
            "/find <pattern> — Fuzzy-search project files by name\n\n\
             Usage:\n\
             \x20 /find <pattern>    Search for files matching the pattern\n\n\
             Searches the project directory for files whose names match\n\
             the given pattern (case-insensitive fuzzy match).\n\n\
             Examples:\n\
             \x20 /find main\n\
             \x20 /find test",
        ),
        "grep" => Some(
            "/grep [-s|--case] <pattern> [path] — Search file contents directly\n\n\
             Usage:\n\
             \x20 /grep <pattern>           Search all files for pattern\n\
             \x20 /grep <pattern> <path>    Search within a specific file or directory\n\
             \x20 /grep -s <pattern>        Case-sensitive search\n\n\
             Fast, direct file content search — no AI, no token cost, instant results.\n\
             Uses git grep in git repos (respects .gitignore), falls back to grep.\n\
             Case-insensitive by default. Limited to 50 results.\n\n\
             Examples:\n\
             \x20 /grep TODO\n\
             \x20 /grep \"fn main\" src/\n\
             \x20 /grep -s MyStruct src/lib.rs",
        ),
        "rename" => Some(
            "/rename <old_name> <new_name> — Cross-file symbol renaming\n\n\
             Usage:\n\
             \x20 /rename <old> <new>    Rename all word-boundary matches across files\n\n\
             Smart find-and-replace that respects word boundaries:\n\
             renaming 'foo' won't change 'foobar' or 'my_foo'.\n\
             Shows a preview of all matches with file:line context,\n\
             then asks for confirmation before applying.\n\n\
             Works on all files tracked by git. Skips binary files.\n\
             Changes are undoable with /undo.\n\n\
             Examples:\n\
             \x20 /rename my_func new_func\n\
             \x20 /rename OldStruct NewStruct\n\
             \x20 /rename CONFIG_KEY NEW_KEY",
        ),
        "extract" => Some(
            "/extract <symbol> <source_file> <target_file> — Move a symbol between files\n\n\
             Usage:\n\
             \x20 /extract <symbol> <source> <target>    Move a top-level item to another file\n\n\
             Finds and moves a function, struct, enum, impl, trait, type alias, const,\n\
             or static from the source file to the target file.\n\
             Includes doc comments and attributes.\n\
             Uses brace-depth tracking to detect the full block.\n\
             Shows a preview and asks for confirmation before moving.\n\
             Creates the target file if it doesn't exist.\n\n\
             Examples:\n\
             \x20 /extract my_func src/lib.rs src/utils.rs\n\
             \x20 /extract MyStruct src/main.rs src/types.rs\n\
             \x20 /extract MyTrait src/old.rs src/new.rs\n\
             \x20 /extract MyResult src/lib.rs src/errors.rs\n\
             \x20 /extract MAX_SIZE src/config.rs src/constants.rs",
        ),
        "explain" => Some(
            "/explain <file>[:<start>-<end>] — Ask the agent to explain code\n\n\
             Usage:\n\
             \x20 /explain <file>               Explain entire file\n\
             \x20 /explain <file>:<start>-<end>  Explain specific lines\n\n\
             Reads the file (or line range), sends it to the agent with a\n\
             clear explanation prompt. Great for understanding unfamiliar code.\n\n\
             Examples:\n\
             \x20 /explain src/main.rs\n\
             \x20 /explain src/main.rs:50-100\n\
             \x20 /explain Cargo.toml:1-20",
        ),
        "move" => Some(
            "/move <SourceType>::<method> [file::]<TargetType> — Relocate a method between impl blocks\n\n\
             Usage:\n\
             \x20 /move Source::method Target           Move method within the same file\n\
             \x20 /move Source::method file.rs::Target   Move method to a different file\n\n\
             Finds the method in `impl SourceType`, extracts it (with doc comments\n\
             and attributes), and inserts it into `impl TargetType`.\n\
             Automatically re-indents to match the target block.\n\
             Shows a preview and asks for confirmation before moving.\n\
             Warns if the method uses `self.` references.\n\n\
             Examples:\n\
             \x20 /move MyStruct::process TargetStruct\n\
             \x20 /move Parser::parse_expr other.rs::Lexer\n\
             \x20 /move Config::validate Settings",
        ),
        "refactor" => Some(
            "/refactor — Refactoring tools overview and dispatch\n\n\
             Usage:\n\
             \x20 /refactor                              Show all refactoring tools\n\
             \x20 /refactor rename <old> <new>            Rename a symbol across files\n\
             \x20 /refactor extract <sym> <src> <dst>     Move a symbol to another file\n\
             \x20 /refactor move <Src>::<method> <Target> Move a method between impl blocks\n\n\
             The umbrella command for all source-code refactoring operations.\n\
             Run /refactor with no arguments to see a summary of all tools\n\
             with examples. Each subcommand dispatches to its standalone\n\
             equivalent (/rename, /extract, /move).\n\n\
             Examples:\n\
             \x20 /refactor\n\
             \x20 /refactor rename MyOldStruct MyNewStruct\n\
             \x20 /refactor extract parse_config src/lib.rs src/config.rs\n\
             \x20 /refactor move Parser::validate Validator",
        ),
        "fix" => Some(
            "/fix — Auto-fix build/lint errors\n\n\
             Runs the project's build and lint checks, captures any errors,\n\
             and sends them to the AI to automatically generate fixes.\n\
             Auto-detects project type (Rust/cargo, Node/npm, Python, etc.).",
        ),
        "forget" => Some(
            "/forget <n> — Remove a project memory by index\n\n\
             Usage:\n\
             \x20 /forget <n>    Delete the memory at the given index\n\n\
             Removes a previously saved project memory. Use /memories to\n\
             see all memories with their indices.\n\n\
             Examples:\n\
             \x20 /forget 0\n\
             \x20 /forget 3",
        ),
        "index" => Some(
            "/index — Build a lightweight index of project source files\n\n\
             Scans the project directory and builds an index of source files,\n\
             their sizes, and structure. Useful for giving the AI awareness\n\
             of the full project layout.",
        ),
        "map" => Some(
            "/map [path] — Show structural map of the codebase\n\n\
             Extracts function signatures, struct/class/trait/enum definitions,\n\
             and other structural symbols from source files.\n\n\
             When ast-grep (sg) is installed, uses it for more accurate AST-based\n\
             extraction. Falls back to regex when ast-grep is not available.\n\n\
             Usage:\n\
             \x20 /map              Map entire project (public symbols)\n\
             \x20 /map src/         Map only files under src/\n\
             \x20 /map --all        Include private symbols\n\
             \x20 /map --all src/   All symbols under src/\n\
             \x20 /map --regex      Force regex backend (skip ast-grep)\n\n\
             Supported languages: Rust, Python, JavaScript, TypeScript, Go, Java.\n\n\
             The repo map is also automatically included in the system prompt\n\
             for structural codebase awareness.",
        ),
        "status" => Some(
            "/status — Show session info\n\n\
             Displays current session information including: working directory,\n\
             active model, message count, git branch (if in a repo), and\n\
             context window usage percentage.",
        ),
        "profile" => Some(
            "/profile — Show unified session statistics\n\n\
             Displays a single-glance summary of the current session:\n\
             model, provider, duration, turns, tokens, cost, and\n\
             context window usage — all in a compact bordered box.\n\n\
             Combines the essentials of /status, /tokens, and /cost.",
        ),
        "tokens" => Some(
            "/tokens — Show token usage and context window\n\n\
             Displays current token usage (input/output), the model's context\n\
             window size, and how much capacity remains. Helps you decide\n\
             when to /compact.",
        ),
        "save" => Some(
            "/save [path] — Save session to file\n\n\
             Usage:\n\
             \x20 /save              Save to yoyo-session.json\n\
             \x20 /save <path>       Save to specified path\n\n\
             Saves the full conversation history to a JSON file so it can\n\
             be resumed later with /load.\n\n\
             Examples:\n\
             \x20 /save\n\
             \x20 /save my-debug-session.json",
        ),
        "load" => Some(
            "/load [path] — Load session from file\n\n\
             Usage:\n\
             \x20 /load              Load from yoyo-session.json\n\
             \x20 /load <path>       Load from specified path\n\n\
             Restores a previously saved session, replacing the current\n\
             conversation history.\n\n\
             Examples:\n\
             \x20 /load\n\
             \x20 /load my-debug-session.json",
        ),
        "diff" => Some(
            "/diff [options] [file] — Show git changes\n\n\
             Usage:\n\
             \x20 /diff                    Show all uncommitted changes\n\
             \x20 /diff --staged           Show only staged changes\n\
             \x20 /diff --name-only        List changed filenames only\n\
             \x20 /diff src/main.rs        Show changes for a specific file\n\
             \x20 /diff --staged main.rs   Staged changes for a specific file\n\n\
             Aliases: --staged, --cached\n\n\
             Displays file summary, change stats, and colored diff output.\n\
             Works in any git repository.",
        ),
        "blame" => Some(
            "/blame <file> [:<start>-<end>] — Show git blame with colored output\n\n\
             Usage:\n\
             \x20 /blame src/main.rs          Blame the entire file\n\
             \x20 /blame src/main.rs:10-20    Blame lines 10 through 20\n\n\
             Colorizes output: commit hash (dim), author (cyan),\n\
             date (dim), line number (yellow), code (default).\n\n\
             Examples:\n\
             \x20 /blame Cargo.toml\n\
             \x20 /blame src/cli.rs:100-150",
        ),
        "undo" => Some(
            "/undo [N] — Undo the last agent turn's file changes\n\n\
             Usage:\n\
             \x20 /undo              Undo the last turn (restore modified files)\n\
             \x20 /undo N            Undo the last N turns\n\
             \x20 /undo --all        Revert ALL uncommitted changes (nuclear option)\n\
             \x20 /undo --last-commit  Revert the most recent git commit (git revert)\n\n\
             Per-turn undo restores files to their state before the agent modified\n\
             them and deletes any files the agent created. Each agent turn is tracked\n\
             as a separate snapshot so you can undo precisely.\n\n\
             --last-commit uses `git revert` to safely undo a committed change while\n\
             preserving history. It also injects context so the agent knows earlier\n\
             conversation may reference code that no longer exists.\n\n\
             Examples:\n\
             \x20 /undo              Undo just the last thing the agent did\n\
             \x20 /undo 3            Undo the last 3 agent turns\n\
             \x20 /undo --all        Git checkout everything (old behavior)\n\
             \x20 /undo --last-commit  Revert the last git commit",
        ),
        "health" => Some(
            "/health — Run project health checks\n\n\
             Auto-detects the project type and runs appropriate health\n\
             checks (build, test, lint). Shows a summary of what passed\n\
             and what failed.",
        ),
        "retry" => Some(
            "/retry — Re-send the last user input\n\n\
             Repeats the most recent user message to the AI. Useful when\n\
             a response was interrupted or you want a different answer.",
        ),
        "history" => Some(
            "/history — Show summary of conversation messages\n\n\
             Displays a compact list of all messages in the current\n\
             conversation: role, length, and a preview of each message.\n\
             Useful for understanding conversation flow.",
        ),
        "hooks" => Some(
            "/hooks — Show active hooks (pre/post tool execution)\n\n\
             Lists all shell hooks configured in .yoyo.toml.\n\
             Shows each hook's phase (pre/post), tool pattern, and command.\n\n\
             Configuration (.yoyo.toml):\n\n\
             \x20 # Pre-hook: runs before bash tool calls\n\
             \x20 hooks.pre.bash = \"echo 'About to run bash'\"\n\n\
             \x20 # Post-hook: runs after every tool call (wildcard)\n\
             \x20 hooks.post.* = \"echo 'Tool finished'\"\n\n\
             Pre-hooks that exit non-zero block the tool from executing.\n\
             Post-hooks always pass through the original tool output.\n\
             All hooks have a 5-second timeout to prevent hanging.\n\n\
             Environment variables available to hooks:\n\
             \x20 TOOL_NAME   — the tool being executed\n\
             \x20 TOOL_PARAMS — JSON string of tool parameters\n\
             \x20 TOOL_OUTPUT — (post-hooks only) tool output, truncated to 1000 chars",
        ),
        "permissions" => Some(
            "/permissions — Show active security and permission configuration\n\n\
             Displays the full security posture of the current session:\n\n\
             \x20 • Auto-approve mode (--yes flag)\n\
             \x20 • Bash command allow/deny patterns\n\
             \x20 • Directory access restrictions\n\n\
             Configure permissions via CLI flags:\n\
             \x20 --allow <pattern>     Auto-approve matching bash commands\n\
             \x20 --deny <pattern>      Block matching bash commands\n\
             \x20 --allow-dir <path>    Restrict file access to these directories\n\
             \x20 --deny-dir <path>     Block file access to these directories\n\n\
             Or in .yoyo.toml:\n\
             \x20 allow = [\"cargo *\", \"git *\"]\n\
             \x20 deny = [\"rm -rf *\"]\n\
             \x20 allow_dir = [\"/home/user/project\"]\n\
             \x20 deny_dir = [\"/etc\", \"/usr\"]",
        ),
        "search" => Some(
            "/search <query> — Search conversation history\n\n\
             Usage:\n\
             \x20 /search <query>    Find messages containing the query\n\n\
             Searches through all conversation messages for matching text\n\
             (case-insensitive). Shows matching messages with context.\n\n\
             Examples:\n\
             \x20 /search error handling\n\
             \x20 /search TODO",
        ),
        "skill" => Some(
            "/skill [subcommand] — List and inspect loaded skills\n\n\
             Usage:\n\
             \x20 /skill              List all loaded skills (same as /skill list)\n\
             \x20 /skill list         List loaded skills with name and description\n\
             \x20 /skill show <name>  Show the full content of a skill\n\
             \x20 /skill path         Show the skills directory path(s)\n\n\
             Skills are loaded from directories specified with --skills <dir>.\n\
             Each skill is a directory containing a SKILL.md file with YAML\n\
             frontmatter (name + description) and markdown instructions.\n\n\
             Examples:\n\
             \x20 /skill\n\
             \x20 /skill list\n\
             \x20 /skill show evolve\n\
             \x20 /skill path",
        ),
        "model" => Some(
            "/model <name> — Switch the AI model\n\n\
             Usage:\n\
             \x20 /model <name>    Switch to the specified model\n\n\
             Changes the active model while preserving the conversation.\n\
             Tab-completion is available for known model names.\n\n\
             Examples:\n\
             \x20 /model claude-sonnet-4-20250514\n\
             \x20 /model gpt-4o\n\
             \x20 /model gemini-2.5-pro",
        ),
        "think" => Some(
            "/think [level] — Show or change thinking level\n\n\
             Usage:\n\
             \x20 /think             Show current thinking level\n\
             \x20 /think <level>     Set thinking level\n\n\
             Levels: off, minimal, low, medium, high\n\n\
             Higher levels give the AI more internal reasoning tokens\n\
             before responding, improving quality for complex tasks.\n\n\
             Examples:\n\
             \x20 /think\n\
             \x20 /think high\n\
             \x20 /think off",
        ),
        "config" => Some(
            "/config — Show all current settings\n\n\
             Displays the current configuration including: model, provider,\n\
             thinking level, system prompt preview, permission settings,\n\
             and other active options.\n\n\
             Subcommands:\n\
               /config show — Show which config file was loaded (if any) and\n\
                              the merged key-value pairs it contributed. Any\n\
                              key matching /key|token|secret|password/i is\n\
                              masked as *** so secrets never print. Useful for\n\
                              debugging 'why isn't my override being picked up?'\n\
                              questions at runtime.\n\
               /config edit — Open the config file in $EDITOR (or $VISUAL, vi).\n\
                              Opens project-level .yoyo.toml if it exists,\n\
                              otherwise falls back to ~/.config/yoyo/config.toml.",
        ),
        "context" => Some(
            "/context — Show loaded project context files\n\n\
             Lists the project context files that were loaded at startup\n\
             (e.g. YOYO.md, CLAUDE.md). These files give the AI awareness\n\
             of project conventions and architecture.\n\n\
             Subcommands:\n\
               /context system — Show system prompt sections with token estimates\n\
                                 Displays each section of the assembled system prompt\n\
                                 with line counts, approximate token estimates, and a\n\
                                 preview of each section's content.\n\
               /context tokens — Show context token budget breakdown\n\
                                 System prompt size, conversation messages, total\n\
                                 context used vs limit, and remaining capacity.",
        ),
        "init" => Some(
            "/init — Scan project and generate a YOYO.md context file\n\n\
             Analyzes the project structure, detects the tech stack, and\n\
             creates a YOYO.md file with context information. This file\n\
             is automatically loaded in future sessions to give the AI\n\
             project awareness.",
        ),
        "version" => Some(
            "/version — Show yoyo version\n\n\
             Displays the current yoyo version number.",
        ),
        "update" => Some(
            "/update — Check for and install the latest version\n\n\
             Checks for the latest release on GitHub and downloads the appropriate\n\
             binary for your platform. Creates a backup of the current binary and\n\
             replaces it with the new version. Requires confirmation before proceeding.\n\n\
             Note: You'll need to restart yoyo to use the new version.\n\n\
             Use --no-update-check at startup to disable the update notification.",
        ),
        "run" => Some(
            "/run <cmd> — Run a shell command directly\n\n\
             Usage:\n\
             \x20 /run <command>     Execute a shell command\n\
             \x20 !<command>         Shortcut for /run\n\n\
             Runs the command directly in the shell without using AI tokens.\n\
             Output is displayed but not added to the conversation.\n\n\
             Examples:\n\
             \x20 /run cargo test\n\
             \x20 !ls -la\n\
             \x20 /run git log --oneline -5",
        ),
        "tree" => Some(
            "/tree [depth] — Show project directory tree\n\n\
             Usage:\n\
             \x20 /tree              Show tree with default depth (3)\n\
             \x20 /tree <depth>      Show tree with specified depth\n\n\
             Displays the project directory structure, respecting .gitignore.\n\n\
             Examples:\n\
             \x20 /tree\n\
             \x20 /tree 5",
        ),
        "pr" => Some(
            "/pr [subcommand] — Pull request management\n\n\
             Usage:\n\
             \x20 /pr                     List open PRs\n\
             \x20 /pr list                List open PRs\n\
             \x20 /pr view <n>            View PR details\n\
             \x20 /pr diff <n>            Show PR diff\n\
             \x20 /pr comment <n> <text>  Comment on a PR\n\
             \x20 /pr create [--draft]    Create a PR from current branch\n\
             \x20 /pr checkout <n>        Checkout a PR's branch\n\n\
             Requires the `gh` CLI to be installed and authenticated.\n\n\
             Examples:\n\
             \x20 /pr\n\
             \x20 /pr create --draft\n\
             \x20 /pr diff 42",
        ),
        "git" => Some(
            "/git <subcmd> — Quick git commands\n\n\
             Usage:\n\
             \x20 /git status          Show working tree status\n\
             \x20 /git log             Show recent commit log\n\
             \x20 /git add             Stage all changes\n\
             \x20 /git diff            Show unstaged changes\n\
             \x20 /git branch          List branches\n\
             \x20 /git stash           Stash current changes\n\
             \x20 /git stash pop       Restore stashed changes\n\
             \x20 /git stash list      List all stash entries\n\
             \x20 /git stash show [n]  Show diff of stash entry\n\
             \x20 /git stash drop [n]  Drop a stash entry\n\n\
             Shortcut for common git operations without leaving yoyo.\n\n\
             Examples:\n\
             \x20 /git status\n\
             \x20 /git log\n\
             \x20 /git stash list",
        ),
        "test" => Some(
            "/test — Auto-detect and run project tests\n\n\
             Detects the project type and runs the appropriate test command:\n\
             \x20 • Rust: cargo test\n\
             \x20 • Node: npm test\n\
             \x20 • Python: pytest / python -m pytest\n\
             \x20 • Go: go test ./...\n\n\
             Output is displayed directly in the terminal.",
        ),
        "lint" => Some(
            "/lint — Auto-detect and run project linter\n\n\
             Detects the project type and runs the appropriate linter:\n\
             \x20 • Rust: cargo clippy\n\
             \x20 • Node: npm run lint / eslint\n\
             \x20 • Python: ruff / flake8\n\
             \x20 • Go: golangci-lint\n\n\
             When lint fails, the error output is automatically fed into\n\
             the agent context so you can ask the AI to help fix issues.\n\n\
             Subcommands:\n\
             \x20 /lint              Run with default strictness (-D warnings)\n\
             \x20 /lint pedantic     Run with pedantic clippy lints (Rust only)\n\
             \x20 /lint strict       Run with pedantic + nursery clippy lints (Rust only)\n\
             \x20 /lint fix          Run linter and auto-send failures to AI for fixing\n\
             \x20 /lint unsafe       Scan for unsafe code blocks and suggest safety attributes\n\n\
             Strictness levels only affect Rust projects (clippy). Other languages\n\
             use their default linter regardless of strictness level.\n\n\
             Output is displayed directly in the terminal.",
        ),
        "spawn" => Some(
            "/spawn <task> — Spawn a subagent to handle a task\n\n\
             Usage:\n\
             \x20 /spawn <task description>\n\n\
             Creates a new AI agent with a separate context window to\n\
             handle the given task. The subagent has access to the same\n\
             tools but operates independently.\n\n\
             Examples:\n\
             \x20 /spawn write unit tests for the parser module\n\
             \x20 /spawn refactor the error handling in src/lib.rs",
        ),
        "review" => Some(
            "/review [path] — AI code review\n\n\
             Usage:\n\
             \x20 /review            Review staged/uncommitted changes\n\
             \x20 /review <path>     Review a specific file\n\n\
             Sends the diff or file to the AI for a code review, looking\n\
             for bugs, style issues, and improvement opportunities.\n\n\
             Examples:\n\
             \x20 /review\n\
             \x20 /review src/main.rs",
        ),
        "mark" => Some(
            "/mark <name> — Bookmark current conversation state\n\n\
             Usage:\n\
             \x20 /mark <name>    Save a named bookmark at this point\n\n\
             Creates a bookmark of the current conversation state that\n\
             can be restored later with /jump. Useful for branching\n\
             explorations.\n\n\
             Examples:\n\
             \x20 /mark before-refactor\n\
             \x20 /mark checkpoint1",
        ),
        "jump" => Some(
            "/jump <name> — Restore conversation to a bookmark\n\n\
             Usage:\n\
             \x20 /jump <name>    Restore to the named bookmark\n\n\
             Restores the conversation to a previously saved bookmark.\n\
             ⚠️  Messages after the bookmark are discarded.\n\n\
             Examples:\n\
             \x20 /jump before-refactor\n\
             \x20 /jump checkpoint1",
        ),
        "marks" => Some(
            "/marks — List all saved bookmarks\n\n\
             Shows all conversation bookmarks created with /mark,\n\
             including their names and the message count at each point.",
        ),
        "plan" => Some(
            "/plan <task> — Plan a task step-by-step (architect mode)\n\n\
             Usage:\n\
             \x20 /plan <task description>\n\n\
             Asks the AI to create a detailed plan for the given task\n\
             without executing any tools. The AI analyzes the codebase\n\
             and produces a step-by-step implementation plan.\n\n\
             Examples:\n\
             \x20 /plan add authentication to the API\n\
             \x20 /plan migrate database from SQLite to PostgreSQL",
        ),
        "remember" => Some(
            "/remember <note> — Save a project-specific memory\n\n\
             Usage:\n\
             \x20 /remember <note>    Save a memory for this project\n\n\
             Saves a note that persists across sessions for the current\n\
             project directory. Memories are loaded automatically when\n\
             you start yoyo in the same directory.\n\n\
             Examples:\n\
             \x20 /remember always run migrations before testing\n\
             \x20 /remember the auth module uses JWT with RS256",
        ),
        "memories" => Some(
            "/memories [query] — List or search project memories\n\n\
             Usage:\n\
             \x20 /memories            List all saved memories\n\
             \x20 /memories <query>    Search memories by keyword (case-insensitive)\n\n\
             Shows saved memories for the current project directory.\n\
             Each memory is displayed with its index (for use with /forget)\n\
             and the saved text.\n\n\
             Examples:\n\
             \x20 /memories\n\
             \x20 /memories docker\n\
             \x20 /memories sqlx",
        ),
        "provider" => Some(
            "/provider <name> — Switch AI provider\n\n\
             Usage:\n\
             \x20 /provider <name>    Switch to the specified provider\n\n\
             Changes the active AI provider and resets the model to that\n\
             provider's default. Tab-completion is available.\n\n\
             Providers: anthropic, openai, google, deepseek, openrouter, local\n\n\
             Examples:\n\
             \x20 /provider openai\n\
             \x20 /provider google",
        ),
        "changes" => Some(
            "/changes — Show files modified during this session\n\n\
             Lists all files that were written or edited by the AI during\n\
             the current session. Useful for reviewing what the AI touched\n\
             before committing.\n\n\
             Flags:\n\
             \x20 --diff    Show colorized git diff for each modified file\n\n\
             Examples:\n\
             \x20 /changes          List modified files\n\
             \x20 /changes --diff   List files and show diffs",
        ),
        "changelog" => Some(
            "/changelog [count] — Show recent git commit history\n\n\
             Usage:\n\
             \x20 /changelog        Show the last 15 commits\n\
             \x20 /changelog <N>    Show the last N commits (max 100)\n\n\
             Displays a compact log of recent commits with hash, message,\n\
             and relative time. Useful for reviewing evolution history\n\
             without leaving the REPL.\n\n\
             Examples:\n\
             \x20 /changelog\n\
             \x20 /changelog 30",
        ),
        "web" => Some(
            "/web <url> — Fetch and display web page content\n\n\
             Usage:\n\
             \x20 /web <url>    Fetch a URL and display readable text\n\n\
             Downloads the web page and extracts clean readable text,\n\
             stripping HTML tags and scripts.\n\n\
             Examples:\n\
             \x20 /web https://docs.rs/serde/latest\n\
             \x20 /web https://rust-lang.org",
        ),
        "export" => Some(
            "/export [path] — Export conversation as readable markdown\n\n\
             Usage:\n\
             \x20 /export              Export to conversation.md\n\
             \x20 /export <path>       Export to specified path\n\n\
             Saves the current conversation as a formatted markdown file.\n\
             User messages, assistant responses, thinking blocks, and tool\n\
             results are all included in a readable format.\n\n\
             Examples:\n\
             \x20 /export\n\
             \x20 /export chat-log.md\n\
             \x20 /export output/session.md",
        ),
        "watch" => Some(
            "/watch [command|off|status] — Auto-run tests after agent edits\n\n\
             Usage:\n\
             \x20 /watch              Auto-detect and enable test watching\n\
             \x20 /watch cargo test   Watch with a specific command\n\
             \x20 /watch off          Disable watching\n\
             \x20 /watch status       Show current watch state\n\n\
             When enabled, yoyo automatically runs the test command after every\n\
             agent turn that modifies files. On success, a brief pass message is\n\
             shown. On failure, the last 20 lines of output are displayed.\n\n\
             Examples:\n\
             \x20 /watch\n\
             \x20 /watch npm test\n\
             \x20 /watch pytest -x\n\
             \x20 /watch off",
        ),
        "ast" => Some(
            "/ast <pattern> [--lang <lang>] [--in <path>] — Structural code search using ast-grep\n\n\
             Searches for AST patterns using ast-grep (sg). Requires `sg` to be installed.\n\
             Pattern syntax: use $VAR for wildcards. E.g. $X.unwrap() matches any .unwrap() call.\n\n\
             Install: https://ast-grep.github.io/\n\n\
             Examples:\n\
             \x20 /ast $X.unwrap()\n\
             \x20 /ast $X.unwrap() --lang rust\n\
             \x20 /ast fn $NAME($$$ARGS) --lang rust --in src/",
        ),
        "stash" => Some(
            "/stash — Save and restore conversation context\n\n\
             Usage:\n\
             \x20 /stash [desc]        Push current conversation and start fresh\n\
             \x20 /stash push [desc]   Same as above\n\
             \x20 /stash pop           Restore the most recent stashed conversation\n\
             \x20 /stash list          Show all stashed conversations\n\
             \x20 /stash drop [N]      Remove stash entry N (default: most recent)\n\n\
             Like git stash, but for your conversation. Useful when you need to\n\
             quickly switch tasks and come back later.",
        ),
        "todo" => Some(
            "/todo — Track tasks during complex operations\n\n\
             Usage:\n\
             \x20 /todo                    Show all tasks\n\
             \x20 /todo add <description>  Add a new task\n\
             \x20 /todo done <id>          Mark task as done\n\
             \x20 /todo wip <id>           Mark as in-progress\n\
             \x20 /todo remove <id>        Remove a task\n\
             \x20 /todo clear              Clear all tasks\n\n\
             Keep track of multi-step plans without losing context.\n\
             Tasks persist for the duration of the session.\n\n\
             The AI agent can also manage tasks via the todo tool during\n\
             agentic runs, helping it stay organized on multi-step operations.",
        ),
        "teach" => Some(
            "/teach — Toggle teach mode\n\n\
             Usage:\n\
             \x20 /teach       Toggle teach mode on/off\n\
             \x20 /teach on    Enable teach mode\n\
             \x20 /teach off   Disable teach mode\n\n\
             When teach mode is active, yoyo explains its reasoning as it works:\n\
             \x20 • Explains WHY before showing code\n\
             \x20 • Uses clear, readable patterns over cleverness\n\
             \x20 • Adds comments on non-obvious lines\n\
             \x20 • Summarizes what you should learn after each task\n\n\
             Great for learning while the agent codes. Session-only — resets when you exit.",
        ),
        "mcp" => Some(
            "/mcp — List and manage MCP server connections\n\n\
             Usage:\n\
             \x20 /mcp         List configured MCP servers\n\
             \x20 /mcp list    List configured MCP servers\n\
             \x20 /mcp help    Show configuration guide\n\n\
             MCP (Model Context Protocol) lets you connect external tool servers.\n\
             Configure servers in .yoyo.toml:\n\n\
             \x20 [mcp_servers.filesystem]\n\
             \x20 command = \"npx\"\n\
             \x20 args = [\"-y\", \"@modelcontextprotocol/server-filesystem\", \"/path\"]\n\n\
             Or pass via CLI:\n\
             \x20 yoyo --mcp \"npx -y @modelcontextprotocol/server-filesystem /path\"",
        ),
        _ => None,
    }
}

/// Build help text as a String so it's testable.
pub fn help_text() -> String {
    let mut out = String::new();

    // ── Session ──
    out.push_str("  ── Session ──\n");
    out.push_str("  /help              Show this help\n");
    out.push_str("  /quit, /exit       Exit yoyo\n");
    out.push_str("  /clear             Clear conversation history (confirms if >4 messages)\n");
    out.push_str("  /clear!            Force-clear without confirmation\n");
    out.push_str("  /compact           Compact conversation to save context space\n");
    out.push_str("  /save [path]       Save session to file (default: yoyo-session.json)\n");
    out.push_str("  /load [path]       Load session from file\n");
    out.push_str("  /retry             Re-send the last user input\n");
    out.push_str("  /status            Show session info\n");
    out.push_str("  /tokens            Show token usage and context window\n");
    out.push_str("  /cost              Show estimated session cost\n");
    out.push_str(
        "  /profile           Show unified session statistics (model, tokens, cost, time)\n",
    );
    out.push_str("  /config            Show all current settings\n");
    out.push_str(
        "  /config show       Show loaded config file path and merged key-value pairs (secrets masked)\n",
    );
    out.push_str("  /config edit       Open config file in $EDITOR\n");
    out.push_str("  /hooks             Show active hooks (pre/post tool execution)\n");
    out.push_str("  /permissions       Show active security and permission configuration\n");
    out.push_str("  /version           Show yoyo version\n");
    out.push_str("  /update            Check for and install the latest version\n");
    out.push_str("  /history           Show summary of conversation messages\n");
    out.push_str("  /search <query>    Search conversation history for matching messages\n");
    out.push_str("  /mark <name>       Bookmark current conversation state\n");
    out.push_str(
        "  /jump <name>       Restore conversation to a bookmark (discards messages after it)\n",
    );
    out.push_str("  /marks             List all saved bookmarks\n");
    out.push_str("  /changes [--diff]  Show files modified (written/edited) during this session\n");
    out.push_str("  /changelog [N]     Show recent git commit history (default: 15, max: 100)\n");
    out.push_str(
        "  /export [path]     Export conversation as readable markdown (default: conversation.md)\n",
    );
    out.push_str(
        "  /stash [desc]      Stash conversation and start fresh (like git stash for chat)\n",
    );
    out.push_str(
        "  /todo [subcmd]     Track tasks: add, done, wip, remove, clear (in-session checklist)\n",
    );
    out.push('\n');

    // ── Git ──
    out.push_str("  ── Git ──\n");
    out.push_str("  /git <subcmd>      Quick git: status, log, add, diff, branch, stash\n");
    out.push_str("  /diff [opts] [file] Show git changes (--staged, --name-only, file filter)\n");
    out.push_str("  /blame <file>      Show git blame with colored output (/blame file:10-20)\n");
    out.push_str("  /undo [N|--all|--last-commit] Undo changes (turn, all, or last commit)\n");
    out.push_str("  /commit [msg]      Commit staged changes (AI-generates message if no msg)\n");
    out.push_str("  /pr [number]       List open PRs, view, diff, comment, or checkout a PR\n");
    out.push_str(
        "                     /pr create [--draft] | /pr <n> diff | /pr <n> comment <text>\n",
    );
    out.push_str(
        "  /review [path]     AI code review: staged changes (default) or a specific file\n",
    );
    out.push('\n');

    // ── Project ──
    out.push_str("  ── Project ──\n");
    out.push_str(
        "  /add <path>        Add file contents to conversation (like @file in Claude Code)\n",
    );
    out.push_str(
        "                     /add <path>:<start>-<end> for line ranges, /add src/*.rs for globs\n",
    );
    out.push_str("  /explain <file>    Ask the agent to explain code from a file\n");
    out.push_str("                     /explain <path>:<start>-<end> for specific line ranges\n");
    out.push_str("  /apply <file>      Apply a diff or patch file (--check for dry-run)\n");
    out.push_str("  /context [system|tokens]  Show loaded project context files\n");
    out.push_str("  /doctor            Run environment diagnostics (git, API key, config, etc.)\n");
    out.push_str("  /init              Scan project and generate a YOYO.md context file\n");
    out.push_str("  /health            Run project health checks (auto-detects project type)\n");
    out.push_str(
        "  /fix               Auto-fix build/lint errors (runs checks, sends failures to AI)\n",
    );
    out.push_str(
        "  /test              Auto-detect and run project tests (cargo test, npm test, etc.)\n",
    );
    out.push_str(
        "  /lint [pedantic|strict|fix|unsafe]  Run project linter (clippy, eslint, ruff, etc.)\n",
    );
    out.push_str("  /run <cmd>         Run a shell command directly (no AI, no tokens)\n");
    out.push_str("  !<cmd>             Shortcut for /run\n");
    out.push_str("  /bg <sub>          Manage background shell processes (run/list/output/kill)\n");
    out.push_str("  /docs <crate> [item] Look up docs.rs documentation for a Rust crate\n");
    out.push_str("  /find <pattern>    Fuzzy-search project files by name\n");
    out.push_str("  /grep <pattern> [path] Search file contents directly (no AI, instant)\n");
    out.push_str("  /rename <old> <new> Cross-file symbol renaming with word boundaries\n");
    out.push_str("  /extract <sym> <src> <dst> Move a symbol (fn/struct/enum/type/const/...) to another file\n");
    out.push_str("  /move <Src>::<method> [file::]<Dst> Move a method between impl blocks\n");
    out.push_str("  /refactor              Show all refactoring tools (rename, extract, move)\n");
    out.push_str("  /index             Build a lightweight index of project source files\n");
    out.push_str(
        "  /map [path]        Show structural map of the codebase (functions, types, etc.)\n",
    );
    out.push_str("  /tree [depth]      Show project directory tree (default depth: 3)\n");
    out.push_str("  /web <url>         Fetch a web page and display clean readable text content\n");
    out.push_str("  /watch [cmd]       Auto-run tests after agent edits (off/status to control)\n");
    out.push_str(
        "  /ast <pattern>     Structural code search using ast-grep (--lang, --in flags)\n",
    );
    out.push_str("  /skill [subcmd]    List and inspect loaded skills (list/show/path)\n");
    out.push('\n');

    // ── AI ──
    out.push_str("  ── AI ──\n");
    out.push_str("  /model <name>      Switch model (preserves conversation)\n");
    out.push_str("  /provider <name>   Switch provider (resets model to provider default)\n");
    out.push_str("  /think [level]     Show or change thinking level (off/low/medium/high)\n");
    out.push_str(
        "  /plan <task>       Plan a task step-by-step without executing (architect mode)\n",
    );
    out.push_str("  /spawn <task>      Spawn a subagent to handle a task (separate context)\n");
    out.push_str(
        "                     The model can also delegate subtasks to sub-agents automatically.\n",
    );
    out.push_str(
        "                     The model can ask you questions mid-task using the ask_user tool.\n",
    );
    out.push_str("  /teach [on|off]    Toggle teach mode — explains reasoning as it works\n");
    out.push_str(
        "  /remember <note>   Save a project-specific memory (persists across sessions)\n",
    );
    out.push_str("  /memories          List project-specific memories for this directory\n");
    out.push_str("  /forget <n>        Remove a project memory by index\n");
    out.push_str("  /mcp [list|help]   List and manage MCP server connections\n");
    out.push('\n');

    // ── Input ──
    out.push_str("  ── Input ──\n");
    out.push_str("  End a line with \\ to continue on the next line\n");
    out.push_str("  Start with ``` to enter a fenced code block\n");

    out
}

pub fn handle_help() {
    println!("{DIM}{}{RESET}", help_text());
}

/// Handle `/help <command>` — show detailed help for a specific command.
/// Returns `true` if a command was looked up (found or not), `false` if no argument.
pub fn handle_help_command(input: &str) -> bool {
    let arg = input
        .strip_prefix("/help")
        .unwrap_or("")
        .trim()
        .trim_start_matches('/');
    if arg.is_empty() {
        return false;
    }
    match command_help(arg) {
        Some(text) => {
            println!("{DIM}{text}{RESET}");
        }
        None => {
            println!("{DIM}  Unknown command: /{arg}\n  Type /help for available commands.{RESET}");
        }
    }
    true
}

/// Returns a short one-line description for a command (used for inline hints).
pub fn command_short_description(cmd: &str) -> Option<&'static str> {
    match cmd {
        "add" => Some("Add file contents to conversation"),
        "apply" => Some("Apply a diff or patch file"),
        "ast" => Some("Structural code search via ast-grep"),
        "bg" => Some("Manage background shell processes"),
        "blame" => Some("Show git blame with colored output"),
        "changes" => Some("Show files modified during this session"),
        "changelog" => Some("Show recent git commit history"),
        "clear" => Some("Clear conversation history"),
        "clear!" => Some("Force-clear without confirmation"),
        "commit" => Some("Commit staged changes"),
        "compact" => Some("Compact conversation to save context"),
        "config" => Some("Show current settings"),
        "context" => Some("Show project context, system prompt sections, or token budget"),
        "cost" => Some("Show estimated session cost"),
        "diff" => Some("Show git changes"),
        "doctor" => Some("Run environment diagnostics"),
        "docs" => Some("Look up crate documentation"),
        "exit" => Some("Exit yoyo"),
        "export" => Some("Export conversation as markdown"),
        "explain" => Some("Ask the agent to explain code from a file"),
        "extract" => Some("Extract a function/block to a new file"),
        "find" => Some("Find files by name pattern"),
        "fix" => Some("Auto-fix build/lint errors"),
        "forget" => Some("Remove a saved memory"),
        "git" => Some("Quick git commands"),
        "grep" => Some("Search file contents"),
        "health" => Some("Run project health checks"),
        "help" => Some("Show help for commands"),
        "history" => Some("Show conversation message summary"),
        "hooks" => Some("Show active hooks (pre/post tool execution)"),
        "index" => Some("Show project file index"),
        "init" => Some("Generate a YOYO.md context file"),
        "jump" => Some("Restore conversation to a bookmark"),
        "lint" => Some("Run project linter (pedantic/strict/fix subcommands)"),
        "load" => Some("Load session from file"),
        "map" => Some("Show project symbol map"),
        "mcp" => Some("List and manage MCP server connections"),
        "mark" => Some("Bookmark current conversation state"),
        "marks" => Some("List saved bookmarks"),
        "memories" => Some("List or search project memories"),
        "model" => Some("Switch or show current model"),
        "move" => Some("Move a method between files"),
        "plan" => Some("AI-generate a task plan"),
        "permissions" => Some("Show active security and permission configuration"),
        "pr" => Some("List, view, or create pull requests"),
        "profile" => Some("Show session statistics (tokens, cost, time, turns)"),
        "provider" => Some("Switch or show current provider"),
        "quit" => Some("Exit yoyo"),
        "refactor" => Some("Refactoring tools (extract, rename, move)"),
        "remember" => Some("Save a memory note"),
        "rename" => Some("Rename a symbol across the project"),
        "retry" => Some("Re-send the last input"),
        "review" => Some("AI code review"),
        "run" => Some("Run a shell command"),
        "save" => Some("Save session to file"),
        "search" => Some("Search conversation history"),
        "skill" => Some("List and inspect loaded skills"),
        "spawn" => Some("Run a task in a sub-agent"),
        "stash" => Some("Stash conversation and start fresh"),
        "status" => Some("Show session info"),
        "teach" => Some("Toggle teach mode — explains reasoning as it works"),
        "test" => Some("Run project tests"),
        "think" => Some("Set thinking level"),
        "todo" => Some("Track tasks (add, done, remove, clear)"),
        "tokens" => Some("Show token usage and context window"),
        "tree" => Some("Show project directory tree"),
        "undo" => Some("Undo last turn's changes, all uncommitted, or last commit"),
        "update" => Some("Check for and install the latest version"),
        "version" => Some("Show yoyo version"),
        "watch" => Some("Auto-run command after file changes"),
        "web" => Some("Fetch a web page"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{command_arg_completions, KNOWN_COMMANDS};

    // ── help_text categorization tests ────────────────────────────────────

    #[test]
    fn test_help_text_contains_all_commands() {
        let text = help_text();
        let expected = [
            "/help",
            "/quit",
            "/exit",
            "/clear",
            "/compact",
            "/save",
            "/load",
            "/retry",
            "/status",
            "/tokens",
            "/cost",
            "/config",
            "/version",
            "/update",
            "/history",
            "/search",
            "/mark",
            "/jump",
            "/marks",
            "/git",
            "/diff",
            "/undo",
            "/commit",
            "/pr",
            "/review",
            "/context",
            "/init",
            "/health",
            "/fix",
            "/test",
            "/lint",
            "/run",
            "/docs",
            "/find",
            "/index",
            "/tree",
            "/model",
            "/think",
            "/spawn",
            "/remember",
            "/memories",
            "/forget",
            "/provider",
            "/changes",
            "/stash",
            "/todo",
            "/profile",
        ];
        for cmd in &expected {
            assert!(text.contains(cmd), "help text should contain {cmd}");
        }
    }

    #[test]
    fn test_help_text_has_category_headers() {
        let text = help_text();
        let categories = [
            "── Session ──",
            "── Git ──",
            "── Project ──",
            "── AI ──",
            "── Input ──",
        ];
        for cat in &categories {
            assert!(
                text.contains(cat),
                "help text should contain category header '{cat}'"
            );
        }
    }

    #[test]
    fn test_help_text_session_commands_under_session_header() {
        let text = help_text();
        let session_start = text.find("── Session ──").expect("Session header missing");
        let git_start = text.find("── Git ──").expect("Git header missing");
        // Session commands should appear between Session and Git headers
        let session_section = &text[session_start..git_start];
        for cmd in &[
            "/help",
            "/quit",
            "/clear",
            "/compact",
            "/save",
            "/load",
            "/retry",
            "/status",
            "/tokens",
            "/cost",
            "/config",
            "/version",
            "/history",
            "/search",
            "/mark",
            "/jump",
            "/marks",
            "/changes",
            "/stash",
            "/todo",
            "/permissions",
            "/profile",
        ] {
            assert!(
                session_section.contains(cmd),
                "{cmd} should be in the Session section"
            );
        }
    }

    #[test]
    fn test_help_text_git_commands_under_git_header() {
        let text = help_text();
        let git_start = text.find("── Git ──").expect("Git header missing");
        let project_start = text.find("── Project ──").expect("Project header missing");
        let git_section = &text[git_start..project_start];
        for cmd in &[
            "/git", "/diff", "/blame", "/undo", "/commit", "/pr", "/review",
        ] {
            assert!(
                git_section.contains(cmd),
                "{cmd} should be in the Git section"
            );
        }
    }

    #[test]
    fn test_help_text_project_commands_under_project_header() {
        let text = help_text();
        let project_start = text.find("── Project ──").expect("Project header missing");
        let ai_start = text.find("── AI ──").expect("AI header missing");
        let project_section = &text[project_start..ai_start];
        for cmd in &[
            "/context", "/init", "/health", "/fix", "/test", "/lint", "/run", "/docs", "/find",
            "/index", "/tree",
        ] {
            assert!(
                project_section.contains(cmd),
                "{cmd} should be in the Project section"
            );
        }
    }

    #[test]
    fn test_help_text_ai_commands_under_ai_header() {
        let text = help_text();
        let ai_start = text.find("── AI ──").expect("AI header missing");
        let input_start = text.find("── Input ──").expect("Input header missing");
        let ai_section = &text[ai_start..input_start];
        for cmd in &[
            "/model",
            "/think",
            "/spawn",
            "/remember",
            "/memories",
            "/forget",
            "/provider",
        ] {
            assert!(
                ai_section.contains(cmd),
                "{cmd} should be in the AI section"
            );
        }
    }

    #[test]
    fn test_help_text_input_section() {
        let text = help_text();
        let input_start = text.find("── Input ──").expect("Input header missing");
        let input_section = &text[input_start..];
        assert!(
            input_section.contains("\\"),
            "Input section should mention backslash continuation"
        );
        assert!(
            input_section.contains("```"),
            "Input section should mention fenced code blocks"
        );
    }
    // ── /help <command> per-command detailed help tests ──────────────────

    #[test]
    fn test_command_help_add_returns_some() {
        let help = command_help("add");
        assert!(help.is_some(), "command_help(\"add\") should return Some");
        let text = help.unwrap();
        assert!(
            text.contains("add"),
            "Help for /add should mention file injection"
        );
    }

    #[test]
    fn test_command_help_nonexistent_returns_none() {
        assert!(
            command_help("nonexistent").is_none(),
            "Nonexistent command should return None"
        );
        assert!(
            command_help("").is_none(),
            "Empty string should return None"
        );
    }

    #[test]
    fn test_command_help_exhaustive_for_known_commands() {
        // Every command in KNOWN_COMMANDS should have a detailed help entry
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            // /exit is an alias for /quit, skip it
            if name == "exit" {
                continue;
            }
            assert!(
                command_help(name).is_some(),
                "Missing detailed help for command: {cmd}"
            );
        }
    }

    #[test]
    fn test_command_help_strips_leading_slash() {
        // command_help should work with or without leading slash
        assert!(command_help("add").is_some());
        assert!(command_help("commit").is_some());
        assert!(command_help("model").is_some());
    }

    #[test]
    fn test_help_still_in_known_commands() {
        assert!(
            KNOWN_COMMANDS.contains(&"/help"),
            "/help should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_arg_completions_help_returns_command_names() {
        let candidates = command_arg_completions("/help", "");
        assert!(
            !candidates.is_empty(),
            "/help should offer command name completions"
        );
        assert!(
            candidates.contains(&"add".to_string()),
            "Should include 'add'"
        );
        assert!(
            candidates.contains(&"commit".to_string()),
            "Should include 'commit'"
        );
    }

    #[test]
    fn test_arg_completions_help_filters_by_prefix() {
        let candidates = command_arg_completions("/help", "co");
        assert!(
            candidates.contains(&"commit".to_string()),
            "Should include 'commit' for prefix 'co'"
        );
        assert!(
            candidates.contains(&"compact".to_string()),
            "Should include 'compact' for prefix 'co'"
        );
        assert!(
            !candidates.contains(&"add".to_string()),
            "Should not include 'add' for prefix 'co'"
        );
    }

    #[test]
    fn test_diff_help_mentions_staged() {
        let help = command_help("diff").expect("diff should have help text");
        assert!(
            help.contains("--staged"),
            "diff help should mention --staged"
        );
        assert!(
            help.contains("--name-only"),
            "diff help should mention --name-only"
        );
        assert!(
            help.contains("--cached"),
            "diff help should mention --cached alias"
        );
    }

    #[test]
    fn test_command_short_description_coverage() {
        // Every KNOWN_COMMAND should have a short description
        for cmd in KNOWN_COMMANDS {
            let name = &cmd[1..]; // strip /
            assert!(
                command_short_description(name).is_some(),
                "Missing short description for command: {cmd}"
            );
        }
    }

    #[test]
    fn test_command_short_description_unknown_returns_none() {
        assert!(command_short_description("nonexistent").is_none());
        assert!(command_short_description("").is_none());
    }
}
