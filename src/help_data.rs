//! Static command help text and short descriptions.
//!
//! Pure data — no logic, no imports. Extracted from `help.rs` to separate
//! "what data exists" from "how to use it."

/// Return detailed help text for a specific command.
///
/// Accepts the command name without the leading `/` (e.g. `"add"`, `"commit"`).
/// Returns `None` for unknown commands.
pub fn command_help(cmd: &str) -> Option<&'static str> {
    match cmd {
        "add" => Some(
            "/add <path|url> — Inject file or web contents into the conversation\n\n\
             Usage:\n\
             \x20 /add <path>              Add entire file\n\
             \x20 /add <path>:<start>-<end> Add specific line range\n\
             \x20 /add src/*.rs            Add files matching a glob pattern\n\
             \x20 /add file1 file2         Add multiple files at once\n\
             \x20 /add <url>               Fetch and add web page content\n\n\
             Examples:\n\
             \x20 /add src/main.rs\n\
             \x20 /add Cargo.toml:1-20\n\
             \x20 /add src/*.rs tests/*.rs\n\
             \x20 /add https://docs.rs/some-crate",
        ),
        "architect" => Some(
            "/architect [on|off|<model>] — Toggle architect mode (dual-model plan+implement)\n\n\
             Architect mode uses a strong model to plan changes (text-only, no tools),\n\
             then a cheaper model to implement the plan (with full tools).\n\
             Saves 60-80% on costs for complex tasks.\n\n\
             Usage:\n\
             \x20 /architect          Toggle on/off\n\
             \x20 /architect on       Enable with current model as architect\n\
             \x20 /architect off      Disable architect mode\n\
             \x20 /architect <model>  Enable with specific architect model\n\n\
             When enabled, each prompt goes through two phases:\n\
             \x20 1. Architect (planning): Describes what changes to make, no tools\n\
             \x20 2. Editor (implementation): Implements the plan with full tool access\n\n\
             The editor model is auto-selected based on the architect model:\n\
             \x20 opus → sonnet, sonnet → haiku, gpt-4o → gpt-4o-mini, etc.\n\n\
             Examples:\n\
             \x20 /architect\n\
             \x20 /architect claude-opus-4-6\n\
             \x20 /architect off",
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
            "/help [command|search <keyword>] — Show help information\n\n\
             Usage:\n\
             \x20 /help              Show all available commands\n\
             \x20 /help <command>    Show detailed help for a specific command\n\
             \x20 /help search <kw>  Search commands by keyword\n\n\
             The search looks across command names, descriptions, and detailed\n\
             help text. Results are ranked by relevance (name match > description\n\
             match > help text match).\n\n\
             Examples:\n\
             \x20 /help\n\
             \x20 /help add\n\
             \x20 /help commit\n\
             \x20 /help search git\n\
             \x20 /help search test",
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
            "/compact [N|all|--preview] — Compact conversation to save context space\n\n\
             Summarizes older turns to free context window space.\n\n\
             Usage:\n\
             \x20 /compact              Default — keep last 10 messages at full fidelity\n\
             \x20 /compact 4            Keep last 4 messages, summarize everything before\n\
             \x20 /compact all          Summarize everything except the last 2 messages\n\
             \x20 /compact --preview    Show what compaction would do without performing it\n\n\
             The number controls how many recent messages survive compaction at\n\
             full detail. Lower numbers free more space but lose more context.\n\
             Minimum value is 2 (always keeps at least the last exchange).\n\n\
             The --preview flag shows estimated token savings, message counts,\n\
             files touched, and topics in the conversation — without changing anything.",
        ),
        "commit" => Some(
            "/commit [message] — Commit staged changes\n\n\
             Usage:\n\
             \x20 /commit              Generate a heuristic commit message from the diff\n\
             \x20 /commit --ai         Use AI to generate a descriptive commit message\n\
             \x20 /commit <message>    Commit with the given message\n\n\
             Without arguments, a heuristic message is generated from the diff.\n\
             With --ai (or --generate), a side agent analyzes the diff and writes\n\
             a conventional-commit-style message describing the actual changes.\n\
             Both modes show the suggestion and let you accept (y), reject (n),\n\
             or edit (e) before committing.\n\n\
             Examples:\n\
             \x20 /commit\n\
             \x20 /commit --ai\n\
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
        "fork" => Some(
            "/fork — Conversation branching\n\n\
             Usage:\n\
             \x20 /fork <name>             Create a named branch from current conversation\n\
             \x20 /fork switch <name>      Switch to a named branch (auto-saves current)\n\
             \x20 /fork list               List all branches\n\
             \x20 /fork delete <name>      Delete a branch (cannot delete current)\n\
             \x20 /fork rename <old> <new> Rename a branch\n\n\
             Like git branches for your conversation. Explore different directions\n\
             and switch between them freely. When you switch, the current branch\n\
             is auto-saved so you never lose work.\n\n\
             Examples:\n\
             \x20 /fork refactor-approach\n\
             \x20 /fork switch main-idea\n\
             \x20 /fork list\n\
             \x20 /fork rename old-idea better-name",
        ),
        "grep" => Some(
            "/grep [-s|--case] [-c|--count] [-C N] [-B N] [-A N] [--include <glob>] [--exclude <glob>] <pattern> [path] — Search file contents directly\n\n\
             Usage:\n\
             \x20 /grep <pattern>           Search all files for pattern\n\
             \x20 /grep <pattern> <path>    Search within a specific file or directory\n\
             \x20 /grep -s <pattern>        Case-sensitive search\n\
             \x20 /grep -C N <pattern>      Show N lines of context around each match\n\
             \x20 /grep -B N <pattern>      Show N lines before each match\n\
             \x20 /grep -A N <pattern>      Show N lines after each match\n\
             \x20 /grep --include \"*.rs\" <pattern>  Only search files matching glob\n\
             \x20 /grep --exclude \"*.md\" <pattern>  Skip files matching glob\n\
             \x20 /grep -c <pattern>        Show match counts per file\n\n\
             Fast, direct file content search — no AI, no token cost, instant results.\n\
             Uses git grep in git repos (respects .gitignore), falls back to grep.\n\
             Case-insensitive by default. Limited to 50 results.\n\n\
             Context flags can be combined: /grep -B 2 -A 1 TODO shows 2 lines\n\
             before and 1 line after each match.\n\n\
             The --include flag filters by file glob (e.g. *.rs, *.toml, *.md).\n\
             The --exclude flag skips files matching the glob.\n\
             For git grep these are pathspecs; for plain grep they use --include/--exclude.\n\n\
             The -c/--count flag shows match counts per file instead of individual lines,\n\
             with a total summary at the end.\n\n\
             Examples:\n\
             \x20 /grep TODO\n\
             \x20 /grep \"fn main\" src/\n\
             \x20 /grep -s MyStruct src/lib.rs\n\
             \x20 /grep -C 3 \"fn main\" src/\n\
             \x20 /grep -B 2 -A 1 TODO\n\
             \x20 /grep --include \"*.rs\" fn main\n\
             \x20 /grep --exclude \"*.md\" TODO\n\
             \x20 /grep -c fn src/\n\
             \x20 /grep -C 3 --include \"*.toml\" version",
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
        "extended" => Some(
            "/extended <task> [--turns N] [--budget N] — Run the agent autonomously on a long task\n\n\
             Usage:\n\
             \x20 /extended <task description>\n\
             \x20 /extended <task description> --turns 30\n\
             \x20 /extended <task description> --budget 15\n\n\
             Enters extended autonomous mode: the agent works step by step on\n\
             the given task without asking questions. It will run tests after\n\
             making changes and summarize results when done.\n\n\
             Options:\n\
             \x20 --turns N     Maximum turns (default: 20)\n\
             \x20 --budget N    Wall-clock time limit in minutes\n\n\
             Examples:\n\
             \x20 /extended add error handling to the parser module\n\
             \x20 /extended refactor the auth system --turns 30\n\
             \x20 /extended rebuild the test suite --budget 15\n\
             \x20 /extended build a REST API for the todo app",
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
             Supported languages: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, Ruby, Shell.\n\n\
             The repo map is also automatically included in the system prompt\n\
             for structural codebase awareness.",
        ),
        "outline" => Some(
            "/outline <query|filepath> [--all] — Search for symbols or show file structure\n\n\
             Finds functions, structs, enums, traits, and other symbols whose names\n\
             match the query. Like VS Code's \"Go to Symbol in Workspace\" (Ctrl+T).\n\n\
             When given a file path, shows all symbols in that file sorted by line number.\n\n\
             Results are ranked by relevance: exact match > prefix > substring.\n\
             Shows up to 30 results by default.\n\n\
             Usage:\n\
             \x20 /outline parse            Find symbols containing \"parse\"\n\
             \x20 /outline Config           Find symbols containing \"Config\"\n\
             \x20 /outline src/main.rs      Show all symbols in src/main.rs\n\
             \x20 /outline handle --all     Show all matches (no limit)\n\n\
             Uses the same symbol extraction as /map (regex or ast-grep).",
        ),
        "status" => Some(
            "/status — Show session dashboard\n\n\
             Displays a comprehensive session overview: model, git branch,\n\
             working directory, active modes (teach, architect, plan, read-only),\n\
             project goal (if set), watch command (if active), session duration\n\
             and turns, uncommitted file changes, token usage, and context\n\
             window usage percentage.",
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
            "/diff [options] [file|ref] — Show git changes\n\n\
             Usage:\n\
             \x20 /diff                    Show all uncommitted changes\n\
             \x20 /diff --staged           Show only staged changes\n\
             \x20 /diff --name-only        List changed filenames only\n\
             \x20 /diff --stat             Show compact diffstat summary only\n\
             \x20 /diff --explain          AI-powered explanation of current changes\n\
             \x20 /diff src/main.rs        Show changes for a specific file\n\
             \x20 /diff --staged main.rs   Staged changes for a specific file\n\
             \x20 /diff --stat --staged    Diffstat for staged changes only\n\
             \x20 /diff --stat HEAD~3      Diffstat of last 3 commits\n\
             \x20 /diff --stat main        Diffstat vs another branch\n\
             \x20 /diff --explain --staged Explain only staged changes\n\n\
             Flags:\n\
             \x20 --staged, --cached  Show only staged (index) changes\n\
             \x20 --name-only         List changed filenames without diff content\n\
             \x20 --stat              Show compact per-file change summary (visual bar)\n\
             \x20 --explain           Send diff to AI for natural-language explanation\n\n\
             --stat accepts a git ref (branch, tag, HEAD~N) to compare against.\n\
             --stat is mutually exclusive with --explain (--stat wins if both given).\n\n\
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
             a response was interrupted or you want a different answer.\n\n\
             Modifiers:\n\
             \x20 /retry --with \"...\"   Append additional instructions to the retry\n\n\
             Examples:\n\
             \x20 /retry                        Re-run as-is\n\
             \x20 /retry --with \"use async\"     Re-run with extra guidance\n\
             \x20 /retry --with \"make it shorter\"",
        ),
        "history" => Some(
            "/history — Show summary of conversation messages\n\n\
             Displays a compact list of all messages in the current\n\
             conversation: role, length, and a preview of each message.\n\
             Useful for understanding conversation flow.\n\n\
             Subcommands:\n\n\
             \x20 /history detail — Per-turn breakdown with tools used and token counts",
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
             \x20 --deny-dir <path>     Block file access to these directories\n\
             \x20 --disallowed-tools <names>  Comma-separated tool names to disable\n\
             \x20 --no-tools                  Disable all tools (chat-only mode)\n\n\
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
        "side" => Some(
            "/side <question> — Ask a quick question without affecting the main conversation\n\n\
             Usage:\n\
             \x20 /side <question>    Ask a quick side question\n\n\
             Opens a disposable one-shot conversation with the same model.\n\
             The side question and answer are NOT added to the main conversation\n\
             history, so they won't consume your main context window.\n\n\
             Side conversations have no tool access — they're pure text Q&A\n\
             for quick lookups, syntax checks, or concept clarifications.\n\n\
             Examples:\n\
             \x20 /side what's the syntax for a match guard in Rust?\n\
             \x20 /side explain the difference between clone and copy\n\
             \x20 /side how do I convert a Vec<u8> to a String?",
        ),
        "quick" => Some(
            "/quick <question> — Fast single-turn answer without tools or agent loop\n\n\
             Usage:\n\
             \x20 /quick <question>    Get a fast answer to a simple question\n\n\
             Sends your question directly to the model without tool access.\n\
             The response is streamed back immediately — no agent loop, no tools.\n\
             Great for quick lookups, error explanations, and syntax help.\n\n\
             Like /side, the exchange is NOT added to the main conversation.\n\n\
             Examples:\n\
             \x20 /quick what does this error mean: borrow of moved value?\n\
             \x20 /quick how do I use sed to replace X with Y?\n\
             \x20 /quick explain the difference between async and threading",
        ),
        "skill" => Some(
            "/skill [subcommand] — List, inspect, install, and search for skills\n\n\
             Usage:\n\
             \x20 /skill              List all loaded skills (same as /skill list)\n\
             \x20 /skill list         List loaded skills with name and description\n\
             \x20 /skill show <name>  Show the full content of a skill\n\
             \x20 /skill path         Show the skills directory path(s)\n\
             \x20 /skill search [query]  Search GitHub for community skills\n\
             \x20 /skill install <path>           Install a skill from a local directory\n\
             \x20 /skill install gh:user/repo     Install a skill from a GitHub repository\n\
             \x20 /skill install gh:user/repo/path  Install from a subdirectory of a repo\n\
             \x20 /skill install gh:user/repo@branch  Install from a specific branch\n\n\
             Skills are loaded from directories specified with --skills <dir>.\n\
             Each skill is a directory containing a SKILL.md file with YAML\n\
             frontmatter (name + description) and markdown instructions.\n\n\
             The search subcommand finds skills on GitHub tagged with the\n\
             yoyo-skill topic. Requires the gh CLI (https://cli.github.com/).\n\n\
             The install subcommand copies a skill directory into\n\
             ~/.config/yoyo/skills/<name>/ for permanent availability.\n\
             Remote install uses 'git clone --depth 1' and cleans up after.\n\n\
             Examples:\n\
             \x20 /skill\n\
             \x20 /skill list\n\
             \x20 /skill show evolve\n\
             \x20 /skill path\n\
             \x20 /skill search research\n\
             \x20 /skill search\n\
             \x20 /skill install ./my-skill/\n\
             \x20 /skill install gh:user/awesome-skill\n\
             \x20 /skill install gh:user/skill-collection/skills/my-skill\n\
             \x20 /skill install gh:user/repo@dev",
        ),
        "model" => Some(
            "/model <name> — Switch the AI model\n\n\
             Usage:\n\
             \x20 /model <name>       Switch to the specified model\n\
             \x20 /model list         Show all available models by provider\n\
             \x20 /model list <prov>  Show models for a specific provider\n\
             \x20 /model info [name]  Show details (pricing, context, provider)\n\n\
             Changes the active model while preserving the conversation.\n\
             Tab-completion is available for known model names.\n\n\
             Examples:\n\
             \x20 /model claude-sonnet-4-20250514\n\
             \x20 /model gpt-4o\n\
             \x20 /model list\n\
             \x20 /model list anthropic\n\
             \x20 /model info gpt-4o",
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
                              otherwise falls back to ~/.config/yoyo/config.toml.\n\
               /config set <key> <value> [--global]\n\
                              Persist a config value to .yoyo.toml (project-local\n\
                              by default) or ~/.yoyo.toml (with --global). Also\n\
                              applies the change to the current session immediately.\n\
                              Keys: model, provider, thinking, temperature,\n\
                              max_tokens, max_turns.\n\
               /config get <key>\n\
                              Show the on-disk value for a single config key.",
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
                                 context used vs limit, and remaining capacity.\n\
               /context files  — Show files referenced in this conversation\n\
                                 Lists all files the agent has read, edited, written,\n\
                                 listed, or searched during the current session,\n\
                                 grouped by action type and deduplicated.",
        ),
        "copy" => Some(
            "/copy [last|code|<text>] — Copy text to the system clipboard\n\n\
             Usage:\n\
             \x20 /copy              Copy the last assistant message (plain text)\n\
             \x20 /copy last         Same as /copy with no arguments\n\
             \x20 /copy code         Copy the last code block from the last response\n\
             \x20 /copy <text>       Copy the literal text argument\n\n\
             Platform support:\n\
             \x20 • macOS: uses pbcopy\n\
             \x20 • Linux: tries wl-copy (Wayland), xclip, then xsel\n\
             \x20 • Windows: uses clip.exe\n\n\
             Examples:\n\
             \x20 /copy\n\
             \x20 /copy code\n\
             \x20 /copy hello world",
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
             \x20 /pr review <n>          AI-powered code review of a PR\n\
             \x20 /pr review <n> --post   Review and post inline comments to GitHub\n\
             \x20 /pr comment <n> <text>  Comment on a PR\n\
             \x20 /pr create [--draft]    Create a PR from current branch\n\
             \x20 /pr checkout <n>        Checkout a PR's branch\n\n\
             The review subcommand fetches the PR diff and description,\n\
             then sends them to the AI for a detailed code review.\n\n\
             With --post, the review is also posted to GitHub as an inline\n\
             PR review with file-specific comments. Without --post, the\n\
             review is only displayed in the terminal.\n\n\
             Requires the `gh` CLI to be installed and authenticated.\n\n\
             Examples:\n\
             \x20 /pr\n\
             \x20 /pr create --draft\n\
             \x20 /pr review 42\n\
             \x20 /pr review 42 --post\n\
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
        "goal" => Some(
            "/goal — Set, view, or check progress on a session goal\n\n\
             Usage:\n\
             \x20 /goal              Show current goal\n\
             \x20 /goal show         Show current goal\n\
             \x20 /goal set <desc>   Set a new goal\n\
             \x20 /goal clear        Remove current goal\n\
             \x20 /goal check        Ask AI to evaluate progress\n\n\
             Goals are stored in .yoyo/goal.md — human-readable, version-controllable.\n\
             Persists across sessions so you can pick up where you left off.\n\n\
             Your goal is automatically included in the AI's context, so it stays aware\n\
             of what you're working toward across the entire conversation.\n\n\
             /goal check sends the goal to the AI, which reviews conversation history\n\
             and project state to evaluate progress, remaining work, and next steps.\n\n\
             Examples:\n\
             \x20 /goal set Refactor auth module to use JWT\n\
             \x20 /goal check\n\
             \x20 /goal clear",
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
        "security" => Some(
            "/security — Run dependency vulnerability scan\n\n\
             Detects the project type and runs the appropriate security audit tool:\n\
             \x20 • Rust: cargo audit\n\
             \x20 • Node: npm audit (with JSON summary)\n\
             \x20 • Python: pip-audit / safety check\n\
             \x20 • Go: govulncheck\n\
             \x20 • Ruby: bundle-audit\n\n\
             If the audit tool isn't installed, prints a helpful install command.\n\
             Findings are grouped by severity (critical/high/medium/low) with\n\
             colored output.\n\n\
             This is a read-only scan — it reports vulnerabilities but does not\n\
             attempt to fix them automatically.\n\n\
             Examples:\n\
             \x20 /security          Scan current project for known vulnerabilities",
        ),
        "loop" => Some(
            "/loop <N|until-pass> <prompt> — Repeat a prompt in a polling loop\n\n\
             Usage:\n\
             \x20 /loop <N> <prompt>          Run the prompt exactly N times (1-100)\n\
             \x20 /loop until-pass <prompt>   Run until the last tool call succeeds (max 20)\n\n\
             Each iteration runs the prompt through the normal agent path with auto-retry.\n\
             A 1-second pause between iterations gives you time to Ctrl+C.\n\n\
             In until-pass mode, the loop stops as soon as the last tool call exits\n\
             without error (e.g. a bash command that returns exit code 0).\n\n\
             Examples:\n\
             \x20 /loop 5 run the tests and fix any failures\n\
             \x20 /loop until-pass run cargo test\n\
             \x20 /loop 3 check if the server is responding",
        ),
        "spawn" => Some(
            "/spawn <task> — Spawn a subagent to handle a task\n\n\
             Usage:\n\
             \x20 /spawn <task description>\n\
             \x20 /spawn --bg <task>              Run in background (returns immediately)\n\
             \x20 /spawn -o <file> <task>         Capture output to a file\n\
             \x20 /spawn --model <name> <task>    Use a specific model for the subagent\n\
             \x20 /spawn --system <prompt> <task> Custom system prompt for the subagent\n\
             \x20 /spawn --bg -o <f> <task>       Background with output capture\n\
             \x20 /spawn collect <id>             Collect a finished background spawn\n\
             \x20 /spawn status                   Show all tracked spawns\n\n\
             Creates a new AI agent with a separate context window to\n\
             handle the given task. The subagent has access to the same\n\
             tools but operates independently.\n\n\
             Use --model to run the subagent with a different model than\n\
             your main session (e.g. a cheaper model for simple tasks,\n\
             or a more powerful one for complex analysis).\n\n\
             Use --system to give the subagent a custom persona or instruction\n\
             set. Quote multi-word prompts. The custom prompt is prepended to\n\
             the standard subagent context (project info + conversation summary).\n\n\
             Background spawns (--bg) return control immediately so you can\n\
             keep working while the subagent runs in parallel. Use\n\
             /spawn collect <id> to retrieve the result when ready.\n\n\
             Examples:\n\
             \x20 /spawn write unit tests for the parser module\n\
             \x20 /spawn --model claude-haiku-4-5 summarize this file\n\
             \x20 /spawn --system \"You are a security auditor\" review src/safety.rs\n\
             \x20 /spawn --bg analyze test coverage for src/\n\
             \x20 /spawn --bg --model gpt-4o -o report.md review error handling\n\
             \x20 /spawn collect 1\n\
             \x20 /spawn status",
        ),
        "review" => Some(
            "/review [--quick|--thorough] [target] — AI code review\n\n\
             Usage:\n\
             \x20 /review                   Review staged/uncommitted changes\n\
             \x20 /review <path>            Review a specific file\n\
             \x20 /review HEAD~3..HEAD      Review a commit range\n\
             \x20 /review --pr 42           Review a GitHub PR\n\
             \x20 /review --quick           Quick review: bugs & security only\n\
             \x20 /review --thorough        Deep review: all dimensions\n\n\
             Effort levels:\n\
             \x20 --quick     Focus on bugs and security only. Skip style nits. Terse output.\n\
             \x20 (default)   Bugs, security, style, performance, and suggestions.\n\
             \x20 --thorough  Exhaustive review: also checks error handling, edge cases,\n\
             \x20             API contracts, test coverage, docs, and concurrency.\n\n\
             Sends the diff or file to the AI for a code review.\n\n\
             Also works as a CLI subcommand (non-interactive):\n\
             \x20 yoyo review               Review from the command line\n\
             \x20 yoyo review --quick       Quick review from CLI\n\
             \x20 yoyo review HEAD~1 > r.md Pipe review to a file\n\n\
             Examples:\n\
             \x20 /review\n\
             \x20 /review --quick src/main.rs\n\
             \x20 /review --thorough HEAD~3..HEAD",
        ),
        "revisit" => Some(
            "/revisit [subcommand] — Review closed/shelved issues that may now be feasible\n\n\
             Subcommands:\n\
             \x20 /revisit              Scan recently closed issues (default)\n\
             \x20 /revisit scan         Same as above\n\
             \x20 /revisit check #N     Inspect a specific closed issue\n\
             \x20 /revisit list         Show tracked revisit candidates\n\
             \x20 /revisit add #N <reason>  Mark an issue for future review\n\
             \x20 /revisit remove #N    Remove an issue from revisit list\n\n\
             Candidates are stored in .yoyo/revisit.json.\n\n\
             Examples:\n\
             \x20 /revisit\n\
             \x20 /revisit check #42\n\
             \x20 /revisit add #100 Too complex before, now have better infra\n\
             \x20 /revisit remove #100",
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
            "/plan — Plan mode toggle, one-shot planning, and plan-then-apply workflow\n\n\
             Usage:\n\
             \x20 /plan on|open        Enter plan mode (read-only, agent thinks but won't modify)\n\
             \x20 /plan off|close      Exit plan mode (return to normal operation)\n\
             \x20 /plan                Show current plan mode status\n\
             \x20 /plan <task>         One-shot plan: create a step-by-step plan without tools\n\
             \x20 /plan show           Display the last generated plan\n\
             \x20 /plan apply          Execute the last generated plan (agent runs it with tools)\n\
             \x20 /plan clear          Discard the stored plan\n\n\
             Plan mode restricts the agent to read-only operations — it can read files,\n\
             search, and analyze, but will not modify files or run destructive commands.\n\
             Useful for understanding a codebase before making changes.\n\n\
             Plan-then-apply workflow:\n\
             \x20 1. /plan <task>  — generate a structured plan (stored automatically)\n\
             \x20 2. /plan show    — review the plan\n\
             \x20 3. /plan apply   — agent executes the plan with full tool access\n\n\
             Examples:\n\
             \x20 /plan on\n\
             \x20 /plan add authentication to the API\n\
             \x20 /plan show\n\
             \x20 /plan apply",
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
        "checkpoint" => Some(
            "/checkpoint — Named file-state snapshots within a session\n\n\
             Usage:\n\
             \x20 /checkpoint <name>         Save a named checkpoint\n\
             \x20 /checkpoint save <name>    Save a named checkpoint\n\
             \x20 /checkpoint list           List all checkpoints\n\
             \x20 /checkpoint restore <name> Restore files to checkpoint state\n\
             \x20 /checkpoint diff <name>    Show changes since checkpoint\n\
             \x20 /checkpoint delete <name>  Delete a checkpoint\n\n\
             Creates named snapshots of all modified files so you can\n\
             return to a known-good state. Session-scoped (not persisted).\n\
             Names must use only letters, numbers, hyphens, underscores.\n\n\
             Examples:\n\
             \x20 /checkpoint before-refactor\n\
             \x20 /checkpoint list\n\
             \x20 /checkpoint restore before-refactor\n\
             \x20 /checkpoint diff before-refactor",
        ),
        "changes" => Some(
            "/changes — Show files modified during this session\n\n\
             Lists all files that were written or edited by the AI during\n\
             the current session. Useful for reviewing what the AI touched\n\
             before committing.\n\n\
             Subcommands:\n\
             \x20 summary   Generate an AI-written natural-language summary of all\n\
             \x20           session changes (suitable for PR descriptions or commit messages)\n\n\
             Flags:\n\
             \x20 --diff    Show colorized git diff for each modified file\n\n\
             Examples:\n\
             \x20 /changes              List modified files\n\
             \x20 /changes --diff       List files and show diffs\n\
             \x20 /changes summary      AI-generated summary of what changed and why",
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
            "/web <url> | /web search <query> — Fetch a web page or search the web\n\n\
             Usage:\n\
             \x20 /web <url>           Fetch a URL and display readable text\n\
             \x20 /web search <query>  Search the web via DuckDuckGo\n\n\
             Downloads the web page and extracts clean readable text,\n\
             stripping HTML tags and scripts. Search returns titles,\n\
             URLs, and snippets for each result.\n\n\
             Examples:\n\
             \x20 /web https://docs.rs/serde/latest\n\
             \x20 /web https://rust-lang.org\n\
             \x20 /web search rust async tutorial\n\
             \x20 /web search \"tokio vs async-std\"",
        ),
        "open" => Some(
            "/open <file>[:<line>] — Open a file in your editor\n\n\
             Usage:\n\
             \x20 /open <file>          Open file in $VISUAL/$EDITOR/fallback\n\
             \x20 /open <file>:<line>   Open file at a specific line number\n\
             \x20 /open <file> <line>   Alternative line number syntax\n\n\
             Editor resolution order:\n\
             \x20 1. $VISUAL environment variable\n\
             \x20 2. $EDITOR environment variable\n\
             \x20 3. First found in PATH: code, vim, vi, nano\n\n\
             Line numbers use +N syntax (works with vim, nano, VS Code, emacs).\n\
             If the file doesn't exist, the editor is still launched (it may create it).\n\n\
             Examples:\n\
             \x20 /open src/main.rs\n\
             \x20 /open src/main.rs:42\n\
             \x20 /open Cargo.toml 10",
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
        "evolution" => Some(
            "/evolution [count] — Show evolution history, session stats, and CI run status\n\n\
             Usage:\n\
             \x20 /evolution           Show last 10 sessions (default)\n\
             \x20 /evolution 20        Show last 20 sessions\n\
             \x20 /evolution 100       Show up to 100 sessions\n\n\
             Reads DAY_COUNT and git tags (dayNN-HH-MM format) to show\n\
             the evolution timeline. Matches journal entries from\n\
             journals/JOURNAL.md to display session titles.\n\n\
             Output includes current day, total sessions, tests count,\n\
             average sessions/day, peak day, current streak, and recent\n\
             CI workflow runs (via gh CLI, if available).",
        ),
        "watch" => Some(
            "/watch [command|all|lint|off|status] — Auto-run lint+test after agent edits\n\n\
             Usage:\n\
             \x20 /watch              Auto-detect lint+test as separate phases\n\
             \x20 /watch all          Same as /watch (two-phase: lint → test)\n\
             \x20 /watch lint         Watch with lint only (no tests)\n\
             \x20 /watch cargo test   Watch with a specific command (single phase)\n\
             \x20 /watch off          Disable watching\n\
             \x20 /watch status       Show current watch state and phases\n\n\
             When enabled, yoyo automatically runs the watch command after every\n\
             agent turn that modifies files. On failure, yoyo auto-fixes up to 3 times.\n\n\
             By default, `/watch` and `/watch all` detect both the lint and test commands\n\
             for your project and run them as **separate phases** — lint is fixed first,\n\
             then tests run. This is more efficient than chaining with `&&` because lint\n\
             fixes are usually mechanical while test fixes require understanding behavior.\n\n\
             Fix prompts are command-type-aware: lint failures get targeted mechanical fix\n\
             hints, test failures get behavioral understanding hints.\n\n\
             Use `/watch lint` for lint-only or `/watch <cmd>` for any custom command.\n\n\
             Examples:\n\
             \x20 /watch\n\
             \x20 /watch lint\n\
             \x20 /watch all\n\
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
             agentic runs, helping it stay organized on multi-step operations.\n\n\
             Board (Kanban view of session_plan/ tasks):\n\
             \x20 /todo board              Show board (reads session_plan/task_*.md)\n\
             \x20 /todo board init [goal]  Create session_plan/ with optional goal\n\
             \x20 /todo board add <title>  Add task (creates task_NN.md, status: backlog)\n\
             \x20 /todo board move <task_id> <status>  Update status in task file\n\
             \x20   e.g. /todo board move task_02 active\n\
             \x20 /todo board done <task_id>  Set task status to done\n\
             \x20 /todo board goal <text>  Write/update session_plan/goal.md\n\
             \x20 /todo board evidence <text>  Append to session_plan/evidence.md\n\n\
             Task IDs are file-based (task_01, task_02, etc.) derived from filenames.\n\
             Valid statuses: backlog, active, done.",
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
        "read" => Some(
            "/read — Toggle read-only oracle mode\n\n\
             Usage:\n\
             \x20 /read       Toggle read-only mode on/off\n\
             \x20 /read on    Enable read-only mode\n\
             \x20 /read off   Disable read-only mode\n\n\
             When read-only mode is active, the agent can only:\n\
             \x20 • Read files (read_file)\n\
             \x20 • Search code (search, grep, find)\n\
             \x20 • List files (list_files)\n\
             \x20 • Run non-destructive bash commands for analysis\n\n\
             The agent will NOT write, edit, or run destructive commands.\n\
             Use for code understanding, architecture exploration, and Q&A.\n\
             Session-only — resets when you exit.",
        ),
        "tips" => Some(
            "/tips — Context-sensitive feature suggestions\n\n\
             Shows helpful tips based on your current session and project:\n\
             \x20 • Project-type tips (Rust, Node, Python, Go)\n\
             \x20 • Session-state tips (watch, goal, context usage)\n\
             \x20 • Feature-discovery tips (random sample of lesser-known commands)\n\n\
             Tips are generated fresh each time — run it again for new suggestions.",
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

/// Returns a short one-line description for a command (used for inline hints).
pub fn command_short_description(cmd: &str) -> Option<&'static str> {
    match cmd {
        "add" => Some("Add file or URL contents to conversation"),
        "architect" => {
            Some("Toggle architect mode — plan with strong model, implement with cheap model")
        }
        "apply" => Some("Apply a diff or patch file"),
        "ast" => Some("Structural code search via ast-grep"),
        "bg" => Some("Manage background shell processes"),
        "blame" => Some("Show git blame with colored output"),
        "changes" => Some("Show files modified during this session"),
        "changelog" => Some("Show recent git commit history"),
        "checkpoint" => Some("Named file-state snapshots (save, list, restore, diff, delete)"),
        "clear" => Some("Clear conversation history"),
        "clear!" => Some("Force-clear without confirmation"),
        "commit" => Some("Commit staged changes"),
        "compact" => Some("Compact conversation to save context"),
        "config" => Some("Show current settings"),
        "context" => Some("Show project context, system prompt sections, or token budget"),
        "copy" => Some("Copy text to the system clipboard"),
        "cost" => Some("Show estimated session cost"),
        "diff" => Some("Show git changes"),
        "doctor" => Some("Run environment diagnostics"),
        "docs" => Some("Look up crate documentation"),
        "exit" => Some("Exit yoyo"),
        "evolution" => Some("Show evolution history, session stats, and CI runs"),
        "export" => Some("Export conversation as markdown"),
        "explain" => Some("Ask the agent to explain code from a file"),
        "extended" => Some("Run the agent autonomously on a long task"),
        "extract" => Some("Extract a function/block to a new file"),
        "find" => Some("Find files by name pattern"),
        "fix" => Some("Auto-fix build/lint errors"),
        "forget" => Some("Remove a saved memory"),
        "fork" => Some("Branch conversations and switch between them"),
        "git" => Some("Quick git commands"),
        "goal" => Some("Set, view, or check progress on a session goal"),
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
        "loop" => Some("Repeat a prompt in a polling loop"),
        "map" => Some("Show project symbol map"),
        "mcp" => Some("List and manage MCP server connections"),
        "mark" => Some("Bookmark current conversation state"),
        "marks" => Some("List saved bookmarks"),
        "memories" => Some("List or search project memories"),
        "model" => Some("Switch, list, or inspect models"),
        "move" => Some("Move a method between files"),
        "outline" => Some("Search for symbols or show file structure"),
        "plan" => Some("Plan mode toggle, one-shot plan, show/apply/clear"),
        "permissions" => Some("Show active security and permission configuration"),
        "pr" => Some("List, view, or create pull requests"),
        "profile" => Some("Show session statistics (tokens, cost, time, turns)"),
        "provider" => Some("Switch or show current provider"),
        "quick" => Some("Fast answer without tools (single-turn, no agent loop)"),
        "quit" => Some("Exit yoyo"),
        "read" => Some("Toggle read-only oracle mode — analyze but not modify"),
        "refactor" => Some("Refactoring tools (extract, rename, move)"),
        "remember" => Some("Save a memory note"),
        "rename" => Some("Rename a symbol across the project"),
        "retry" => Some("Re-send the last input (--with \"...\" to refine)"),
        "review" => Some("AI code review (--quick, --thorough)"),
        "revisit" => Some("Review closed/shelved issues that may now be feasible"),
        "run" => Some("Run a shell command"),
        "save" => Some("Save session to file"),
        "search" => Some("Search conversation history"),
        "security" => Some("Run dependency vulnerability scan"),
        "side" => Some("Ask a quick question without affecting conversation"),
        "skill" => Some("List, inspect, install, and search for skills"),
        "spawn" => Some("Run a task in a sub-agent"),
        "stash" => Some("Stash conversation and start fresh"),
        "status" => Some("Show session dashboard"),
        "teach" => Some("Toggle teach mode — explains reasoning as it works"),
        "test" => Some("Run project tests"),
        "think" => Some("Set thinking level"),
        "tips" => Some("Context-sensitive feature suggestions"),
        "todo" => Some("Track tasks (add, done, remove, clear, board)"),
        "tokens" => Some("Show token usage and context window"),
        "tree" => Some("Show project directory tree"),
        "undo" => Some("Undo last turn's changes, all uncommitted, or last commit"),
        "update" => Some("Check for and install the latest version"),
        "version" => Some("Show yoyo version"),
        "watch" => Some("Auto-run lint+test after file changes"),
        "web" => Some("Fetch a web page or search the web"),
        "open" => Some("Open a file in your editor"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::KNOWN_COMMANDS;
    use std::collections::HashSet;

    // ── Completeness tests ──

    #[test]
    fn test_every_known_command_has_help() {
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            // /exit is an alias for /quit — no dedicated help entry
            if name == "exit" {
                continue;
            }
            assert!(
                command_help(name).is_some(),
                "KNOWN_COMMAND {cmd} has no command_help entry"
            );
        }
    }

    #[test]
    fn test_every_known_command_has_short_description() {
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            assert!(
                command_short_description(name).is_some(),
                "KNOWN_COMMAND {cmd} has no command_short_description entry"
            );
        }
    }

    #[test]
    fn test_no_orphan_help_entries() {
        // Verify a fake command returns None (no catch-all that leaks)
        assert!(
            command_help("zzz_nonexistent").is_none(),
            "Fake command should not have a help entry"
        );
        assert!(
            command_short_description("zzz_nonexistent").is_none(),
            "Fake command should not have a short description"
        );
    }

    // ── Content quality tests ──

    #[test]
    fn test_short_descriptions_are_actually_short() {
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            if let Some(desc) = command_short_description(name) {
                assert!(
                    desc.len() <= 80,
                    "Short description for {cmd} is too long ({} chars): {desc}",
                    desc.len()
                );
            }
        }
    }

    #[test]
    fn test_help_entries_are_non_empty() {
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            if name == "exit" {
                continue;
            }
            if let Some(help) = command_help(name) {
                assert!(
                    help.len() >= 20,
                    "Help entry for {cmd} is suspiciously short ({} chars): {help}",
                    help.len()
                );
            }
        }
    }

    #[test]
    fn test_help_uses_bare_names_not_slashes() {
        // command_help matches on bare names like "add", not "/add"
        // Verify that passing a slash-prefixed name returns None
        // (callers are expected to strip the slash before calling)
        assert!(
            command_help("/add").is_none(),
            "command_help should not match slash-prefixed names"
        );
        assert!(
            command_short_description("/add").is_none(),
            "command_short_description should not match slash-prefixed names"
        );
        // But bare name works
        assert!(command_help("add").is_some());
        assert!(command_short_description("add").is_some());
    }

    // ── Edge case tests ──

    #[test]
    fn test_command_help_returns_none_for_empty() {
        assert!(command_help("").is_none());
    }

    #[test]
    fn test_command_short_description_returns_none_for_empty() {
        assert!(command_short_description("").is_none());
    }

    #[test]
    fn test_command_short_description_returns_none_for_unknown() {
        assert!(command_short_description("zzz_nonexistent").is_none());
    }

    #[test]
    fn test_no_duplicate_short_descriptions() {
        // Deduplicate KNOWN_COMMANDS (e.g. /quick appears twice)
        let unique_cmds: HashSet<&str> = KNOWN_COMMANDS.iter().copied().collect();

        let mut seen: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        let mut duplicates: Vec<(&str, &str)> = Vec::new();

        for cmd in &unique_cmds {
            let name = cmd.trim_start_matches('/');
            if let Some(desc) = command_short_description(name) {
                if let Some(first) = seen.get(desc) {
                    duplicates.push((cmd, first));
                } else {
                    seen.insert(desc, cmd);
                }
            }
        }

        // Allow known aliases: /exit and /quit share a description
        duplicates.retain(|(a, b)| {
            let pair = [a.trim_start_matches('/'), b.trim_start_matches('/')];
            !(pair.contains(&"exit") && pair.contains(&"quit"))
        });

        assert!(
            duplicates.is_empty(),
            "Duplicate short descriptions found: {duplicates:?}"
        );
    }

    // ── Consistency tests ──

    #[test]
    fn test_help_entries_start_with_command_name() {
        // Help text should mention the command (usually starts with /cmd)
        for cmd in KNOWN_COMMANDS {
            let name = cmd.trim_start_matches('/');
            if name == "exit" {
                continue;
            }
            if let Some(help) = command_help(name) {
                assert!(
                    help.contains(name) || help.contains(cmd),
                    "Help entry for {cmd} doesn't mention the command name anywhere"
                );
            }
        }
    }

    #[test]
    fn test_quit_and_exit_share_short_description() {
        let quit_desc = command_short_description("quit");
        let exit_desc = command_short_description("exit");
        assert!(quit_desc.is_some(), "/quit should have a short description");
        assert!(exit_desc.is_some(), "/exit should have a short description");
        assert_eq!(
            quit_desc, exit_desc,
            "/quit and /exit should share the same short description"
        );
    }
}
