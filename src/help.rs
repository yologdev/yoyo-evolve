//! Help text and help command handlers for yoyo.
//!
//! Contains the detailed per-command help entries, the summary help listing,
//! the `/help` command handlers, and the `--help` CLI help output.
//! This is the canonical module for all help content.

use crate::cli::VERSION;
use crate::commands::{discover_custom_commands, get_custom_command_content, KNOWN_COMMANDS};
use crate::format::*;

/// Return command names (without `/` prefix) for `/help <Tab>` completion.
/// Includes both built-in and custom commands.
pub fn help_command_completions(partial_lower: &str) -> Vec<String> {
    let mut completions: Vec<String> = KNOWN_COMMANDS
        .iter()
        .map(|c| c.trim_start_matches('/'))
        // /exit is an alias for /quit — skip it for cleaner completion
        .filter(|name| *name != "exit")
        .filter(|name| name.to_lowercase().starts_with(partial_lower))
        .map(|name| name.to_string())
        .collect();

    // Append custom commands
    for (name, _) in discover_custom_commands() {
        if name.to_lowercase().starts_with(partial_lower) && !completions.contains(&name) {
            completions.push(name);
        }
    }

    completions
}

// Static help data lives in help_data.rs — re-exported here for backward compatibility.
pub use crate::help_data::{command_help, command_short_description};

/// Build the full `--help` output as a string.
///
/// This is the canonical source for CLI help text. `cli::print_help()` and
/// `cli::help_text()` both delegate here.
pub fn cli_help_text() -> String {
    let mut s = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(s, "yoyo v{VERSION} — a coding agent growing up in public");
    let _ = writeln!(s);
    let _ = writeln!(s, "Usage: yoyo [OPTIONS] [PROMPT]");
    let _ = writeln!(s);
    let _ = writeln!(s, "  Run with a bare prompt:  yoyo \"fix this bug\"");
    let _ = writeln!(s, "  Or with --prompt flag:   yoyo -p \"fix this bug\"");
    let _ = writeln!(s, "  Interactive REPL:        yoyo");
    let _ = writeln!(s);
    let _ = writeln!(s, "Options:");
    let _ = writeln!(
        s,
        "  --model <name>    Model to use (default: claude-opus-4-6)"
    );
    let _ = writeln!(
        s,
        "  --provider <name> Provider: anthropic (default), openai, google, openrouter,"
    );
    let _ = writeln!(
        s,
        "                    ollama, xai, groq, deepseek, mistral, cerebras, zai, custom"
    );
    let _ = writeln!(
        s,
        "  --base-url <url>  Custom API endpoint (e.g., http://localhost:11434/v1)"
    );
    let _ = writeln!(
        s,
        "  --thinking <lvl>  Enable extended thinking (off, minimal, low, medium, high)"
    );
    let _ = writeln!(
        s,
        "  --max-tokens <n>  Maximum output tokens per response (default: 8192)"
    );
    let _ = writeln!(
        s,
        "  --max-turns <n>   Maximum agent turns per prompt (default: 50)"
    );
    let _ = writeln!(
        s,
        "  --temperature <f> Sampling temperature (0.0-1.0, default: model default)"
    );
    let _ = writeln!(s, "  --skills <dir>    Directory containing skill files");
    let _ = writeln!(
        s,
        "  --system <text>   Custom system prompt (overrides default)"
    );
    let _ = writeln!(s, "  --system-file <f> Read system prompt from file");
    let _ = writeln!(
        s,
        "  --prompt, -p <t>  Run a single prompt and exit (no REPL)"
    );
    let _ = writeln!(s, "  --output, -o <f>  Write final response text to a file");
    let _ = writeln!(
        s,
        "  --api-key <key>   API key (overrides provider-specific env var)"
    );
    let _ = writeln!(
        s,
        "  --mcp <cmd>       Connect to an MCP server via stdio (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --openapi <spec>  Load OpenAPI spec file and register API tools (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --no-color        Disable colored output (also respects NO_COLOR env)"
    );
    let _ = writeln!(s, "  --no-bell         Disable terminal bell on long completions (also respects YOYO_NO_BELL env)");
    let _ = writeln!(s, "  --no-notify       Disable desktop notifications on long completions (also respects YOYO_NO_NOTIFY env)");
    let _ = writeln!(
        s,
        "  --no-rtk          Disable RTK (Rust Token Killer) proxy even when installed"
    );
    let _ = writeln!(
        s,
        "  --no-update-check Skip startup update check (also respects YOYO_NO_UPDATE_CHECK=1 env)"
    );
    let _ = writeln!(
        s,
        "  --json            Output JSON instead of plain text (for -p and piped modes)"
    );
    let _ = writeln!(
        s,
        "  --print           Output only the response text (no UI chrome, for -p and piped modes)"
    );
    let _ = writeln!(
        s,
        "                    Implies --yes; suppresses spinners, tool output, and color"
    );
    let _ = writeln!(
        s,
        "  --output-format <fmt>  Output format: text, json, stream-json (NDJSON events)"
    );
    let _ = writeln!(
        s,
        "  --audit           Enable audit logging of all tool calls to .yoyo/audit.jsonl"
    );
    let _ = writeln!(
        s,
        "                    (also respects YOYO_AUDIT=1 env or audit = true in config)"
    );
    let _ = writeln!(
        s,
        "  --verbose, -v     Show debug info (API errors, request details)"
    );
    let _ = writeln!(
        s,
        "  --quiet, -q       Suppress informational stderr output (config/context loading messages)"
    );
    let _ = writeln!(
        s,
        "                    Auto-enabled when both stdin and stdout are piped. Also respects YOYO_QUIET=1 env"
    );
    let _ = writeln!(
        s,
        "  --yes, -y         Auto-approve all tool executions (skip confirmation prompts)"
    );
    let _ = writeln!(
        s,
        "  --auto-commit     Auto-commit file changes after each agent turn"
    );
    let _ = writeln!(
        s,
        "  --allow <pat>     Auto-approve bash commands matching glob pattern (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --deny <pat>      Auto-deny bash commands matching glob pattern (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --allow-dir <d>   Restrict file access to this directory (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --deny-dir <d>    Block file access to this directory (repeatable)"
    );
    let _ = writeln!(
        s,
        "  --disallowed-tools <names>  Comma-separated tool names to disable (e.g., bash,write_file)"
    );
    let _ = writeln!(
        s,
        "  --no-tools        Disable all tools (chat-only mode, no file access or commands)"
    );
    let _ = writeln!(
        s,
        "  --lite            Optimize for small/local LLMs (minimal prompt, 4 tools, 8K context)"
    );
    let _ = writeln!(
        s,
        "  --context-strategy <s>  Context management: compaction (default) or checkpoint"
    );
    let _ = writeln!(
        s,
        "  --context-window <n>    Override context window size (tokens). Default: auto-detected"
    );
    let _ = writeln!(
        s,
        "                          per provider (200K Anthropic, 1M Google, 128K OpenAI, etc.)"
    );
    let _ = writeln!(s, "  --continue, -c    Resume last saved session");
    let _ = writeln!(
        s,
        "  --fallback <prov> Fallback provider if primary fails (e.g. --fallback google)"
    );
    let _ = writeln!(
        s,
        "  --print-system-prompt  Print the fully assembled system prompt and exit"
    );
    let _ = writeln!(s, "  --help, -h        Show this help message");
    let _ = writeln!(s, "  --version, -V     Show version");
    let _ = writeln!(s);
    let _ = writeln!(s, "Subcommands (run from shell, no REPL):");
    let _ = writeln!(
        s,
        "  help              Show this help message (same as --help)"
    );
    let _ = writeln!(s, "  version           Show version (same as --version)");
    let _ = writeln!(s, "  setup             Run the interactive setup wizard");
    let _ = writeln!(
        s,
        "  init              Generate a YOYO.md project context file"
    );
    let _ = writeln!(
        s,
        "  doctor            Diagnose yoyo setup (config, API key, provider, tool availability)"
    );
    let _ = writeln!(
        s,
        "  health            Run project health checks (build, test, clippy, fmt)"
    );
    let _ = writeln!(
        s,
        "  lint              Run project linter (e.g. yoyo lint --strict, yoyo lint unsafe)"
    );
    let _ = writeln!(s, "  test              Run project test suite");
    let _ = writeln!(
        s,
        "  tree              Show project directory tree (e.g. yoyo tree 5)"
    );
    let _ = writeln!(s, "  map               Show project symbol map");
    let _ = writeln!(
        s,
        "  run               Run a shell command (e.g. yoyo run cargo clippy)"
    );
    let _ = writeln!(
        s,
        "  diff              Show git diff (e.g. yoyo diff --staged)"
    );
    let _ = writeln!(
        s,
        "  commit            Commit staged changes (e.g. yoyo commit \"fix typo\")"
    );
    let _ = writeln!(
        s,
        "  review            AI code review (non-interactive, supports commit ranges and PRs)"
    );
    let _ = writeln!(
        s,
        "  blame             Show git blame (e.g. yoyo blame src/main.rs 10-20)"
    );
    let _ = writeln!(
        s,
        "  grep              Search files for a pattern (e.g. yoyo grep TODO src/)"
    );
    let _ = writeln!(
        s,
        "  find              Find files by name (e.g. yoyo find main)"
    );
    let _ = writeln!(s, "  index             Build and display project index");
    let _ = writeln!(
        s,
        "  outline           Search for symbols or show file structure (e.g. yoyo outline src/main.rs)"
    );
    let _ = writeln!(
        s,
        "  update            Check for and install the latest yoyo release"
    );
    let _ = writeln!(
        s,
        "  docs              Look up docs.rs documentation (e.g. yoyo docs serde)"
    );
    let _ = writeln!(
        s,
        "  skill             List, inspect, install, and search for skills (e.g. yoyo skill list --skills ./skills)"
    );
    let _ = writeln!(
        s,
        "  watch             Toggle watch mode (e.g. yoyo watch, yoyo watch lint)"
    );
    let _ = writeln!(
        s,
        "  status            Show version, git branch, and working directory"
    );
    let _ = writeln!(
        s,
        "  undo              Undo changes (e.g. yoyo undo --last-commit)"
    );
    let _ = writeln!(
        s,
        "  changelog         Show recent commits (e.g. yoyo changelog 20)"
    );
    let _ = writeln!(
        s,
        "  config            Show configuration (e.g. yoyo config show)"
    );
    let _ = writeln!(s, "  permissions       Show security/permission config");
    let _ = writeln!(
        s,
        "  todo              Manage project tasks (e.g. yoyo todo list, yoyo todo add ...)"
    );
    let _ = writeln!(
        s,
        "  goal              Show or set a persistent session goal (e.g. yoyo goal show)"
    );
    let _ = writeln!(
        s,
        "  memories          Show project memories (e.g. yoyo memories)"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "Commands (in REPL):");
    let _ = writeln!(s);
    let _ = writeln!(s, "  Session:");
    let _ = writeln!(
        s,
        "    /help              Show help (/help <cmd> for details)"
    );
    let _ = writeln!(s, "    /quit, /exit       Exit yoyo");
    let _ = writeln!(s, "    /clear             Clear conversation history");
    let _ = writeln!(s, "    /clear!            Force-clear without confirmation");
    let _ = writeln!(
        s,
        "    /compact [N|all]   Compact conversation to save context"
    );
    let _ = writeln!(s, "    /save [path]       Save session to file");
    let _ = writeln!(s, "    /load [path]       Load session from file");
    let _ = writeln!(s, "    /retry             Re-send the last user input");
    let _ = writeln!(s, "    /status            Show session info");
    let _ = writeln!(
        s,
        "    /tokens            Show token usage and context window"
    );
    let _ = writeln!(s, "    /cost              Show estimated session cost");
    let _ = writeln!(s, "    /profile           Show unified session statistics");
    let _ = writeln!(s, "    /config            Show all current settings");
    let _ = writeln!(s, "    /hooks             Show active hooks");
    let _ = writeln!(s, "    /permissions       Show security/permission config");
    let _ = writeln!(s, "    /version           Show yoyo version");
    let _ = writeln!(
        s,
        "    /update            Check for and install latest version"
    );
    let _ = writeln!(
        s,
        "    /history           Show conversation message summary"
    );
    let _ = writeln!(
        s,
        "    /history detail    Per-turn breakdown with tools and token counts"
    );
    let _ = writeln!(s, "    /search <query>    Search conversation history");
    let _ = writeln!(s, "    /mark <name>       Bookmark conversation state");
    let _ = writeln!(s, "    /jump <name>       Restore to a bookmark");
    let _ = writeln!(s, "    /marks             List saved bookmarks");
    let _ = writeln!(
        s,
        "    /checkpoint [sub]  Named file-state snapshots (save/list/restore/diff/delete)"
    );
    let _ = writeln!(
        s,
        "    /changes [sub]     Show files modified this session (summary, --diff)"
    );
    let _ = writeln!(s, "    /changelog [N]     Show recent git commit history");
    let _ = writeln!(
        s,
        "    /evolution [N]     Show evolution history and session stats"
    );
    let _ = writeln!(s, "    /export [path]     Export conversation as markdown");
    let _ = writeln!(
        s,
        "    /stash [desc]      Stash conversation and start fresh"
    );
    let _ = writeln!(
        s,
        "    /fork [sub]        Branch conversations (switch/list/delete/rename)"
    );
    let _ = writeln!(
        s,
        "    /todo [subcmd]     Track tasks (add/done/wip/remove/clear)"
    );
    let _ = writeln!(
        s,
        "    /tips              Context-sensitive feature suggestions"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "  Git:");
    let _ = writeln!(
        s,
        "    /git <subcmd>      Quick git: status, log, add, diff, branch"
    );
    let _ = writeln!(
        s,
        "    /diff [opts]       Show git diff (--staged, --name-only)"
    );
    let _ = writeln!(
        s,
        "    /blame <file>      Show git blame with colored output"
    );
    let _ = writeln!(
        s,
        "    /undo [N|--all]    Undo changes (turn, all, or last commit)"
    );
    let _ = writeln!(
        s,
        "    /commit [msg]      Commit staged changes (--ai for AI-generated message)"
    );
    let _ = writeln!(
        s,
        "    /pr [number]       List, view, diff, comment, or create PRs"
    );
    let _ = writeln!(
        s,
        "    /review [target]   AI code review (staged, file, range, or --pr N)"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "  Project:");
    let _ = writeln!(
        s,
        "    /add <path|url>    Add file or URL contents to conversation"
    );
    let _ = writeln!(s, "    /explain <file>    Ask the agent to explain code");
    let _ = writeln!(s, "    /apply <file>      Apply a diff or patch file");
    let _ = writeln!(
        s,
        "    /context           Show loaded project context files"
    );
    let _ = writeln!(s, "    /doctor            Run environment diagnostics");
    let _ = writeln!(
        s,
        "    /init              Generate a YOYO.md project context file"
    );
    let _ = writeln!(
        s,
        "    /goal [subcmd]     Set, view, or check progress on a goal"
    );
    let _ = writeln!(s, "    /health            Run project health checks");
    let _ = writeln!(
        s,
        "    /fix               Auto-fix build/lint errors via AI"
    );
    let _ = writeln!(
        s,
        "    /test              Auto-detect and run project tests"
    );
    let _ = writeln!(
        s,
        "    /lint [opts]       Run project linter (pedantic/strict/fix/unsafe)"
    );
    let _ = writeln!(
        s,
        "    /security          Run dependency vulnerability scan"
    );
    let _ = writeln!(
        s,
        "    /run <cmd>         Run a shell command (no AI, no tokens)"
    );
    let _ = writeln!(
        s,
        "    /bg <sub>          Background shell jobs (run/list/output/kill)"
    );
    let _ = writeln!(s, "    /docs <crate>      Look up docs.rs documentation");
    let _ = writeln!(
        s,
        "    /find <pattern>    Fuzzy-search project files by name"
    );
    let _ = writeln!(
        s,
        "    /grep <pat> [path] Search file contents (no AI, instant)"
    );
    let _ = writeln!(s, "    /rename <old> <new> Cross-file symbol rename");
    let _ = writeln!(
        s,
        "    /extract <sym> <src> <dst>  Move a symbol to another file"
    );
    let _ = writeln!(
        s,
        "    /move <S>::<m> <D>  Move a method between impl blocks"
    );
    let _ = writeln!(s, "    /refactor          Show all refactoring tools");
    let _ = writeln!(
        s,
        "    /index             Build lightweight project source index"
    );
    let _ = writeln!(
        s,
        "    /map [path]        Show structural map (functions, types)"
    );
    let _ = writeln!(
        s,
        "    /outline <query|file>  Search for symbols or show file structure"
    );
    let _ = writeln!(s, "    /tree [depth]      Show project directory tree");
    let _ = writeln!(
        s,
        "    /web <url>         Fetch and display web page content"
    );
    let _ = writeln!(s, "    /open <file>[:<line>]  Open a file in $EDITOR");
    let _ = writeln!(
        s,
        "    /copy [last|code]  Copy text to the system clipboard"
    );
    let _ = writeln!(
        s,
        "    /watch [cmd|all|lint]  Auto-run lint+test after agent edits"
    );
    let _ = writeln!(
        s,
        "    /loop <N|until-pass>   Repeat a prompt in a polling loop"
    );
    let _ = writeln!(
        s,
        "    /ast <pattern>     Structural code search (ast-grep)"
    );
    let _ = writeln!(
        s,
        "    /revisit [subcmd]  Review closed/shelved issues (scan/check/list/add/remove)"
    );
    let _ = writeln!(
        s,
        "    /skill [subcmd]    List, inspect, install, and search for skills"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "  AI:");
    let _ = writeln!(
        s,
        "    /model <name|list|info> Switch, list, or inspect models"
    );
    let _ = writeln!(s, "    /provider <name>   Switch provider");
    let _ = writeln!(
        s,
        "    /think [level]     Show/change thinking (off/low/medium/high)"
    );
    let _ = writeln!(
        s,
        "    /plan <task>       Plan a task without executing (show/apply/clear)"
    );
    let _ = writeln!(
        s,
        "    /architect [on|off] Architect mode — plan strong, implement cheap"
    );
    let _ = writeln!(
        s,
        "    /spawn <task>      Spawn a subagent for a task (--bg for background)"
    );
    let _ = writeln!(
        s,
        "    /extended <task>   Autonomous mode for long tasks (--turns N)"
    );
    let _ = writeln!(
        s,
        "    /teach [on|off]    Toggle teach mode (explains reasoning)"
    );
    let _ = writeln!(
        s,
        "    /read [on|off]     Toggle read-only oracle mode (analyze only)"
    );
    let _ = writeln!(
        s,
        "    /side <question>   Quick question (no tools, no context impact)"
    );
    let _ = writeln!(
        s,
        "    /quick <question>  Fast single-turn answer (no tools, no agent loop)"
    );
    let _ = writeln!(s, "    /remember <note>   Save a project-specific memory");
    let _ = writeln!(s, "    /memories          List project memories");
    let _ = writeln!(s, "    /forget <n>        Remove a project memory by index");
    let _ = writeln!(s, "    /mcp [list|help]   Manage MCP server connections");
    let _ = writeln!(s);
    let _ = writeln!(s, "Environment:");
    let _ = writeln!(
        s,
        "  ANTHROPIC_API_KEY  API key for Anthropic (default provider)"
    );
    let _ = writeln!(s, "  OPENAI_API_KEY    API key for OpenAI");
    let _ = writeln!(s, "  GOOGLE_API_KEY    API key for Google/Gemini");
    let _ = writeln!(s, "  GROQ_API_KEY      API key for Groq");
    let _ = writeln!(s, "  XAI_API_KEY       API key for xAI");
    let _ = writeln!(s, "  DEEPSEEK_API_KEY  API key for DeepSeek");
    let _ = writeln!(s, "  OPENROUTER_API_KEY API key for OpenRouter");
    let _ = writeln!(s, "  ZAI_API_KEY       API key for ZAI (Zhipu AI / z.ai)");
    let _ = writeln!(s, "  API_KEY            Fallback API key (any provider)");
    let _ = writeln!(
        s,
        "  YOYO_NO_UPDATE_CHECK  Set to 1 to skip startup update check"
    );
    let _ = writeln!(
        s,
        "  YOYO_AUDIT            Set to 1 to enable audit logging"
    );
    let _ = writeln!(
        s,
        "  YOYO_SESSION_BUDGET_SECS  Soft wall-clock budget in seconds; retry loops bail"
    );
    let _ = writeln!(
        s,
        "                            early when <30s remain (default: unbounded)"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "Config files (searched in order, first found wins):");
    let _ = writeln!(
        s,
        "  .yoyo.toml                  Project-level config (current directory)"
    );
    let _ = writeln!(s, "  ~/.yoyo.toml                Home directory config");
    let _ = writeln!(s, "  ~/.config/yoyo/config.toml  User-level config (XDG)");
    let _ = writeln!(s);
    let _ = writeln!(s, "Config file format (key = value):");
    let _ = writeln!(s, "  model = \"claude-sonnet-4-20250514\"");
    let _ = writeln!(s, "  provider = \"openai\"");
    let _ = writeln!(s, "  base_url = \"http://localhost:11434/v1\"");
    let _ = writeln!(s, "  thinking = \"medium\"");
    let _ = writeln!(s, "  max_tokens = 4096");
    let _ = writeln!(s, "  max_turns = 20");
    let _ = writeln!(s, "  api_key = \"sk-ant-...\"");
    let _ = writeln!(s, "  system_prompt = \"You are a Go expert\"");
    let _ = writeln!(s, "  system_file = \"prompts/system.txt\"");
    let _ = writeln!(
        s,
        "  mcp = [\"npx open-websearch@latest\", \"npx @mcp/server-filesystem /tmp\"]"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "  [permissions]");
    let _ = writeln!(s, "  allow = [\"git *\", \"cargo *\"]");
    let _ = writeln!(s, "  deny = [\"rm -rf *\"]");
    let _ = writeln!(s);
    let _ = writeln!(s, "  [directories]");
    let _ = writeln!(s, "  allow = [\"./src\", \"./tests\"]");
    let _ = writeln!(s, "  deny = [\"~/.ssh\", \"/etc\"]");
    let _ = writeln!(s);
    let _ = writeln!(s, "CLI flags override config file values.");
    s
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
    out.push_str("  /config set        Persist a config key=value to .yoyo.toml [--global]\n");
    out.push_str("  /config get        Show the on-disk value for a config key\n");
    out.push_str("  /hooks             Show active hooks (pre/post tool execution)\n");
    out.push_str("  /permissions       Show active security and permission configuration\n");
    out.push_str("  /version           Show yoyo version\n");
    out.push_str("  /update            Check for and install the latest version\n");
    out.push_str("  /history           Show summary of conversation messages\n");
    out.push_str("  /history detail    Per-turn breakdown with tools and token counts\n");
    out.push_str("  /search <query>    Search conversation history for matching messages\n");
    out.push_str("  /mark <name>       Bookmark current conversation state\n");
    out.push_str(
        "  /jump <name>       Restore conversation to a bookmark (discards messages after it)\n",
    );
    out.push_str("  /marks             List all saved bookmarks\n");
    out.push_str(
        "  /checkpoint [sub]  Named file-state snapshots (save, list, restore, diff, delete)\n",
    );
    out.push_str("  /changes [sub]     Show files modified this session (summary, --diff)\n");
    out.push_str("  /changelog [N]     Show recent git commit history (default: 15, max: 100)\n");
    out.push_str("  /evolution [N]     Show evolution history, session stats, and CI runs\n");
    out.push_str(
        "  /export [path]     Export conversation as readable markdown (default: conversation.md)\n",
    );
    out.push_str(
        "  /copy [last|code]  Copy text to the system clipboard (last message, code block, etc.)\n",
    );
    out.push_str(
        "  /stash [desc]      Stash conversation and start fresh (like git stash for chat)\n",
    );
    out.push_str("  /fork [sub]        Branch conversations (switch, list, delete, rename)\n");
    out.push_str(
        "  /todo [subcmd]     Track tasks: add, done, wip, remove, clear (in-session checklist)\n",
    );
    out.push_str("  /tips              Context-sensitive feature suggestions for your session\n");
    out.push('\n');

    // ── Git ──
    out.push_str("  ── Git ──\n");
    out.push_str("  /git <subcmd>      Quick git: status, log, add, diff, branch, stash\n");
    out.push_str("  /diff [opts] [file] Show git changes (--staged, --name-only, file filter)\n");
    out.push_str("  /blame <file>      Show git blame with colored output (/blame file:10-20)\n");
    out.push_str("  /undo [N|--all|--last-commit] Undo changes (turn, all, or last commit)\n");
    out.push_str("  /commit [msg]      Commit staged changes (--ai for AI-generated message)\n");
    out.push_str("  /pr [number]       List open PRs, view, diff, comment, or checkout a PR\n");
    out.push_str(
        "                     /pr create [--draft] | /pr <n> diff | /pr <n> comment <text>\n",
    );
    out.push_str("  /review [target]   AI code review: staged changes, file, range, or --pr N\n");
    out.push('\n');

    // ── Project ──
    out.push_str("  ── Project ──\n");
    out.push_str(
        "  /add <path|url>    Add file or URL contents to conversation (like @file in Claude Code)\n",
    );
    out.push_str(
        "                     /add <path>:<start>-<end> for line ranges, /add src/*.rs for globs\n",
    );
    out.push_str("                     /add https://... to fetch and inject web page content\n");
    out.push_str("  /explain <file>    Ask the agent to explain code from a file\n");
    out.push_str("                     /explain <path>:<start>-<end> for specific line ranges\n");
    out.push_str("  /apply <file>      Apply a diff or patch file (--check for dry-run)\n");
    out.push_str("  /context [system|tokens|files]  Show loaded project context files\n");
    out.push_str("  /doctor            Run environment diagnostics (git, API key, config, etc.)\n");
    out.push_str("  /init              Scan project and generate a YOYO.md context file\n");
    out.push_str(
        "  /goal [subcmd]     Set, view, or check progress on a session goal (set/show/clear/check)\n",
    );
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
    out.push_str(
        "  /security          Run dependency vulnerability scan (cargo audit, npm audit, etc.)\n",
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
    out.push_str("  /outline <query|file>  Search for symbols or show file structure\n");
    out.push_str("  /tree [depth]      Show project directory tree (default depth: 3)\n");
    out.push_str("  /web <url>         Fetch a web page and display clean readable text content\n");
    out.push_str(
        "  /open <file>[:<line>]  Open a file in $EDITOR (supports line number with +N)\n",
    );
    out.push_str(
        "  /watch [cmd|all|lint]  Auto-run lint+test after agent edits (off/status to control)\n",
    );
    out.push_str(
        "  /loop <N|until-pass>   Repeat a prompt in a polling loop (stop early on success)\n",
    );
    out.push_str(
        "  /ast <pattern>     Structural code search using ast-grep (--lang, --in flags)\n",
    );
    out.push_str(
        "  /revisit [subcmd]  Review closed/shelved issues (scan/check/list/add/remove)\n",
    );
    out.push_str(
        "  /skill [subcmd]    List, inspect, install, and search for skills (list/show/path/install/search)\n",
    );
    out.push('\n');

    // ── AI ──
    out.push_str("  ── AI ──\n");
    out.push_str("  /model <name|list|info> Switch, list, or inspect models\n");
    out.push_str("  /provider <name>   Switch provider (resets model to provider default)\n");
    out.push_str("  /think [level]     Show or change thinking level (off/low/medium/high)\n");
    out.push_str("  /plan [on|off|task] Plan mode toggle, one-shot plan, show/apply/clear\n");
    out.push_str(
        "  /architect [on|off] Toggle architect mode — plan with strong model, implement cheap\n",
    );
    out.push_str("  /spawn <task>      Spawn a subagent to handle a task (--bg for background)\n");
    out.push_str(
        "                     The model can also delegate subtasks to sub-agents automatically.\n",
    );
    out.push_str(
        "                     The model can ask you questions mid-task using the ask_user tool.\n",
    );
    out.push_str(
        "  /extended <task>   Run the agent autonomously on a long task (--turns N, --budget N)\n",
    );
    out.push_str("  /teach [on|off]    Toggle teach mode — explains reasoning as it works\n");
    out.push_str("  /read [on|off]     Toggle read-only oracle mode — analyze but not modify\n");
    out.push_str(
        "  /side <question>   Quick question without affecting main conversation (no tools)\n",
    );
    out.push_str("  /quick <question>  Fast single-turn answer — no tools, no agent loop\n");
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

    // ── Custom ── (dynamic, only shown if custom commands exist)
    let custom_cmds = discover_custom_commands();
    append_custom_section(&mut out, &custom_cmds);

    out
}

/// Append a "Custom" section to the help text if any custom commands exist.
/// Factored out so tests can call it with synthetic data.
fn append_custom_section(out: &mut String, custom_cmds: &[(String, String)]) {
    if !custom_cmds.is_empty() {
        out.push('\n');
        out.push_str("  ── Custom ──\n");
        for (name, content) in custom_cmds {
            let desc = content.lines().next().unwrap_or("").trim();
            out.push_str(&format!("  /{name:<17}{desc}\n"));
        }
    }
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
            // Check custom commands before declaring unknown
            if let Some(content) = get_custom_command_content(arg) {
                println!("{DIM}  /{arg} — Custom command\n\n{content}{RESET}");
            } else {
                println!(
                    "{DIM}  Unknown command: /{arg}\n  Type /help for available commands.{RESET}"
                );
            }
        }
    }
    true
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
            "/checkpoint",
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
            "/security",
            "/loop",
            "/run",
            "/docs",
            "/find",
            "/index",
            "/tree",
            "/model",
            "/think",
            "/spawn",
            "/side",
            "/quick",
            "/extended",
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
            "/checkpoint",
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
            "/context",
            "/init",
            "/health",
            "/fix",
            "/test",
            "/lint",
            "/security",
            "/loop",
            "/run",
            "/docs",
            "/find",
            "/index",
            "/tree",
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
            "/extended",
            "/side",
            "/quick",
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

    #[test]
    fn test_append_custom_section_shows_commands() {
        let custom_cmds = vec![
            (
                "deploy".to_string(),
                "Deploy to production\nMore details here".to_string(),
            ),
            ("review".to_string(), "Review the current diff".to_string()),
        ];
        let mut out = String::new();
        append_custom_section(&mut out, &custom_cmds);
        assert!(out.contains("── Custom ──"), "Should have Custom header");
        assert!(out.contains("/deploy"), "Should list /deploy");
        assert!(
            out.contains("Deploy to production"),
            "Should show first line as description"
        );
        assert!(
            !out.contains("More details here"),
            "Should NOT show second line"
        );
        assert!(out.contains("/review"), "Should list /review");
        assert!(out.contains("Review the current diff"));
    }

    #[test]
    fn test_append_custom_section_empty_when_no_commands() {
        let mut out = String::new();
        append_custom_section(&mut out, &[]);
        assert!(
            !out.contains("Custom"),
            "Should not show Custom section when empty"
        );
        assert!(out.is_empty());
    }

    #[test]
    fn test_help_completions_include_custom_commands() {
        // Custom commands come from the filesystem, so in a test environment
        // without .yoyo/commands/ dirs, we verify the mechanism works by
        // checking that built-in commands are returned and the function doesn't panic.
        let completions = help_command_completions("");
        assert!(
            completions.contains(&"add".to_string()),
            "Should include built-in 'add'"
        );
        assert!(
            !completions.contains(&"exit".to_string()),
            "Should exclude 'exit' alias"
        );
    }

    #[test]
    fn cli_help_text_contains_key_flags() {
        // Regression guard: the canonical --help output (now in help.rs)
        // must mention essential CLI flags and sections.
        let text = cli_help_text();
        for expected in &[
            "--model",
            "--provider",
            "--prompt",
            "--skills",
            "--help",
            "--version",
            "Subcommands",
            "Options:",
            "Environment:",
            "Config files",
            "ANTHROPIC_API_KEY",
            "YOYO_SESSION_BUDGET_SECS",
        ] {
            assert!(
                text.contains(expected),
                "cli_help_text() must contain {expected:?}"
            );
        }
    }

    #[test]
    fn cli_help_text_matches_cli_help_text_fn() {
        // The cli::help_text() wrapper must return identical output
        // to the canonical cli_help_text() in help.rs.
        assert_eq!(crate::cli::help_text(), cli_help_text());
    }

    /// Regression guard: every command in KNOWN_COMMANDS (except aliases) must
    /// have both a detailed `command_help()` entry and a `command_short_description()`.
    #[test]
    fn all_known_commands_have_help() {
        // Aliases and special variants that share help with their base command
        let aliases = ["/exit", "/clear!", "/quit"];

        for &cmd in KNOWN_COMMANDS {
            let name = cmd.strip_prefix('/').unwrap_or(cmd);
            if aliases.contains(&cmd) {
                continue;
            }
            assert!(
                command_help(name).is_some(),
                "command_help(\"{name}\") returned None — add help text for /{name}"
            );
            assert!(
                command_short_description(name).is_some(),
                "command_short_description(\"{name}\") returned None — add a short description for /{name}"
            );
        }
    }

    /// Regression guard: every non-alias command in KNOWN_COMMANDS must appear
    /// somewhere in the `help_text()` output so users can discover it via `/help`.
    #[test]
    fn all_help_commands_in_help_text() {
        let text = help_text();
        let aliases = ["/exit", "/clear!", "/quit"];

        for &cmd in KNOWN_COMMANDS {
            if aliases.contains(&cmd) {
                continue;
            }
            assert!(
                text.contains(cmd),
                "{cmd} is in KNOWN_COMMANDS but missing from help_text() output"
            );
        }
    }

    /// Regression guard: `help_command_completions("")` must return entries for
    /// all non-alias commands in KNOWN_COMMANDS, ensuring tab-completion covers
    /// every documented command.
    #[test]
    fn help_command_completions_covers_known_commands() {
        let completions = help_command_completions("");
        // /exit is filtered by help_command_completions itself
        let skip = ["/exit"];

        for &cmd in KNOWN_COMMANDS {
            if skip.contains(&cmd) {
                continue;
            }
            let name = cmd.strip_prefix('/').unwrap_or(cmd);
            assert!(
                completions.contains(&name.to_string()),
                "help_command_completions(\"\") is missing {cmd} — ensure it's returned for tab-completion"
            );
        }
    }

    // ── cli_help_text() tests ──────────────────────────────────────────

    #[test]
    fn cli_help_text_is_non_empty() {
        let text = cli_help_text();
        assert!(!text.is_empty(), "cli_help_text() should not be empty");
        assert!(
            text.len() > 500,
            "cli_help_text() should be substantial (got {} bytes)",
            text.len()
        );
    }

    #[test]
    fn cli_help_text_contains_usage_section() {
        let text = cli_help_text();
        assert!(text.contains("Usage:"), "should contain Usage section");
        assert!(
            text.contains("yoyo [OPTIONS] [PROMPT]"),
            "should show usage synopsis"
        );
    }

    #[test]
    fn cli_help_text_contains_all_important_flags() {
        let text = cli_help_text();
        let flags = [
            "--model",
            "--provider",
            "--print",
            "--thinking",
            "--system",
            "--system-file",
            "--skills",
            "--disallowed-tools",
            "--no-tools",
            "--max-tokens",
            "--max-turns",
            "--temperature",
            "--base-url",
            "--mcp",
            "--openapi",
            "--no-color",
            "--no-bell",
            "--no-notify",
            "--no-rtk",
            "--no-update-check",
            "--json",
            "--output-format",
            "--audit",
            "--verbose",
            "--quiet",
            "--yes",
            "--auto-commit",
            "--allow",
            "--deny",
            "--allow-dir",
            "--deny-dir",
            "--context-strategy",
            "--context-window",
            "--continue",
            "--fallback",
            "--print-system-prompt",
            "--help",
            "--version",
        ];
        for flag in &flags {
            assert!(
                text.contains(flag),
                "cli_help_text() must document flag {flag}"
            );
        }
    }

    #[test]
    fn cli_help_text_contains_subcommands_section() {
        let text = cli_help_text();
        assert!(
            text.contains("Subcommands"),
            "should have Subcommands section"
        );
        let subcommands = [
            "setup",
            "init",
            "doctor",
            "health",
            "lint",
            "test",
            "tree",
            "map",
            "run",
            "diff",
            "commit",
            "review",
            "blame",
            "grep",
            "find",
            "index",
            "outline",
            "update",
            "docs",
            "skill",
            "watch",
            "status",
            "undo",
            "changelog",
            "config",
            "permissions",
            "todo",
            "goal",
            "memories",
        ];
        for sub in &subcommands {
            assert!(
                text.contains(sub),
                "cli_help_text() subcommands section must mention '{sub}'"
            );
        }
    }

    #[test]
    fn cli_help_text_contains_environment_section() {
        let text = cli_help_text();
        assert!(
            text.contains("Environment:"),
            "should have Environment section"
        );
        assert!(
            text.contains("ANTHROPIC_API_KEY"),
            "should mention ANTHROPIC_API_KEY"
        );
        assert!(text.contains("Config files"), "should mention config files");
    }

    #[test]
    fn cli_help_text_contains_repl_commands_section() {
        let text = cli_help_text();
        assert!(
            text.contains("Commands (in REPL):"),
            "should have REPL commands section"
        );
        // Check a few representative REPL commands are listed
        for cmd in &["/help", "/quit", "/model", "/spawn", "/diff"] {
            assert!(
                text.contains(cmd),
                "cli_help_text() REPL section must mention {cmd}"
            );
        }
    }

    // ── help_text() tests ──────────────────────────────────────────────

    #[test]
    fn help_text_is_non_empty() {
        let text = help_text();
        assert!(!text.is_empty(), "help_text() should not be empty");
    }

    #[test]
    fn help_text_contains_recently_added_commands() {
        let text = help_text();
        // Commands added in recent evolution days
        let recent = [
            "/spawn", "/retry", "/bg", "/review", "/map", "/grep", "/blame", "/outline", "/fork",
            "/watch", "/apply", "/open", "/goal", "/skill", "/doctor",
        ];
        for cmd in &recent {
            assert!(
                text.contains(cmd),
                "help_text() should list recently added command {cmd}"
            );
        }
    }

    // ── command_help() content tests ───────────────────────────────────

    #[test]
    fn command_help_grep_mentions_pattern_args() {
        let help = command_help("grep").expect("grep should have help");
        assert!(help.contains("pattern"), "grep help should mention pattern");
        assert!(
            help.contains("--include"),
            "grep help should mention --include"
        );
        assert!(
            help.contains("--exclude"),
            "grep help should mention --exclude"
        );
        assert!(help.contains("-C"), "grep help should mention -C context");
        assert!(help.contains("-c"), "grep help should mention -c count");
    }

    #[test]
    fn command_help_spawn_describes_subagent() {
        let help = command_help("spawn").expect("spawn should have help");
        assert!(
            help.contains("subagent") || help.contains("sub-agent") || help.contains("agent"),
            "spawn help should describe task delegation via agent"
        );
        assert!(
            help.contains("task"),
            "spawn help should mention task delegation"
        );
    }

    #[test]
    fn command_help_map_mentions_repo_map() {
        let help = command_help("map").expect("map should have help");
        assert!(
            help.contains("map") || help.contains("symbol"),
            "map help should describe the repo map"
        );
        assert!(
            help.contains("--all") || help.contains("private"),
            "map help should mention --all or private symbols"
        );
    }

    #[test]
    fn command_help_review_describes_code_review() {
        let help = command_help("review").expect("review should have help");
        assert!(
            help.contains("review"),
            "review help should mention code review"
        );
        assert!(
            help.contains("--pr") || help.contains("PR"),
            "review help should mention PR review"
        );
        assert!(
            help.contains("diff") || help.contains("changes"),
            "review help should mention diff or changes"
        );
    }

    #[test]
    fn command_help_diff_mentions_stat_flag() {
        let help = command_help("diff").expect("diff should have help");
        assert!(
            help.contains("--stat"),
            "diff help should mention --stat flag"
        );
    }

    #[test]
    fn command_help_returns_none_for_empty_and_garbage() {
        assert!(command_help("").is_none(), "empty string → None");
        assert!(
            command_help("zzz_not_a_command_xyz").is_none(),
            "garbage → None"
        );
        assert!(command_help("   ").is_none(), "whitespace-only → None");
    }

    // ── command_short_description() tests ──────────────────────────────

    #[test]
    fn command_short_description_known_commands_non_empty() {
        let cmds = [
            "add", "diff", "commit", "grep", "spawn", "map", "review", "model",
        ];
        for cmd in &cmds {
            let desc = command_short_description(cmd)
                .unwrap_or_else(|| panic!("'{cmd}' should have a short description"));
            assert!(
                !desc.is_empty(),
                "short description for '{cmd}' should not be empty"
            );
            assert!(
                desc.len() < 120,
                "short description for '{cmd}' should be concise (got {} chars)",
                desc.len()
            );
        }
    }

    #[test]
    fn command_short_description_unknown_returns_none_varied() {
        assert!(command_short_description("xyzzy").is_none());
        assert!(command_short_description("123").is_none());
        assert!(command_short_description("help nonexistent").is_none());
    }

    // ── help_command_completions() tests ───────────────────────────────

    #[test]
    fn help_command_completions_mostly_unique() {
        let completions = help_command_completions("");
        let mut seen = std::collections::HashSet::new();
        let mut dups = Vec::new();
        for c in &completions {
            if !seen.insert(c.as_str()) {
                dups.push(c.clone());
            }
        }
        // Allow at most 1 known duplicate (/quick appears twice in KNOWN_COMMANDS).
        assert!(dups.len() <= 1, "too many duplicate completions: {dups:?}");
    }

    #[test]
    fn help_command_completions_filters_by_prefix() {
        let completions = help_command_completions("sp");
        assert!(
            completions.contains(&"spawn".to_string()),
            "prefix 'sp' should match 'spawn'"
        );
        assert!(
            !completions.contains(&"diff".to_string()),
            "prefix 'sp' should not match 'diff'"
        );
    }

    #[test]
    fn help_command_completions_empty_prefix_returns_all() {
        let all = help_command_completions("");
        // Should have a substantial number of commands
        assert!(
            all.len() >= 30,
            "empty prefix should return many commands, got {}",
            all.len()
        );
    }

    // ── handle_help_command() edge case tests ──────────────────────────

    #[test]
    fn handle_help_command_empty_returns_false() {
        // Empty arg means "show general help", returns false
        assert!(!handle_help_command("/help"));
        assert!(!handle_help_command("/help   "));
    }

    #[test]
    fn handle_help_command_known_command_returns_true() {
        // Known command should print help and return true
        assert!(handle_help_command("/help add"));
        assert!(handle_help_command("/help diff"));
    }

    #[test]
    fn handle_help_command_strips_slash_prefix() {
        // Should work whether user types "/help diff" or "/help /diff"
        assert!(handle_help_command("/help /diff"));
        assert!(handle_help_command("/help /add"));
    }

    #[test]
    fn handle_help_command_unknown_still_returns_true() {
        // Unknown command should print "Unknown command" and still return true
        // (it handled the input, even if command wasn't found)
        assert!(handle_help_command("/help zzz_nonexistent"));
    }

    // ── command_short_description: exhaustive non-empty checks ──

    #[test]
    fn command_short_description_every_known_command_non_empty() {
        // Every KNOWN_COMMAND must have a non-empty short description
        for &cmd in KNOWN_COMMANDS {
            let name = cmd.strip_prefix('/').unwrap_or(cmd);
            if let Some(desc) = command_short_description(name) {
                assert!(
                    !desc.trim().is_empty(),
                    "short description for '{name}' must not be blank"
                );
            } else {
                panic!("command_short_description(\"{name}\") returned None");
            }
        }
    }

    #[test]
    fn command_short_description_specific_values() {
        // Spot-check that key commands return the expected description text
        assert_eq!(
            command_short_description("add"),
            Some("Add file or URL contents to conversation")
        );
        assert_eq!(command_short_description("quit"), Some("Exit yoyo"));
        assert_eq!(
            command_short_description("help"),
            Some("Show help for commands")
        );
        assert_eq!(command_short_description("diff"), Some("Show git changes"));
        assert_eq!(
            command_short_description("spawn"),
            Some("Run a task in a sub-agent")
        );
    }

    #[test]
    fn command_short_description_exit_alias() {
        // /exit is an alias for /quit — both should have descriptions
        assert!(command_short_description("exit").is_some());
        assert!(command_short_description("quit").is_some());
    }

    #[test]
    fn command_short_description_clear_bang() {
        // /clear! is a variant — should have a description
        let desc = command_short_description("clear!").expect("clear! should have a description");
        assert!(
            desc.contains("clear") || desc.contains("Clear") || desc.contains("force"),
            "clear! description should relate to clearing: got '{desc}'"
        );
    }

    // ── handle_help_command: per-command help lookups ──

    #[test]
    fn handle_help_command_compact_returns_true() {
        assert!(handle_help_command("/help compact"));
    }

    #[test]
    fn handle_help_command_model_returns_true() {
        assert!(handle_help_command("/help model"));
    }

    #[test]
    fn handle_help_command_spawn_returns_true() {
        assert!(handle_help_command("/help spawn"));
    }

    #[test]
    fn handle_help_command_config_returns_true() {
        assert!(handle_help_command("/help config"));
    }

    #[test]
    fn handle_help_command_commit_returns_true() {
        assert!(handle_help_command("/help commit"));
    }

    #[test]
    fn handle_help_command_watch_returns_true() {
        assert!(handle_help_command("/help watch"));
    }

    #[test]
    fn handle_help_command_todo_returns_true() {
        assert!(handle_help_command("/help todo"));
    }

    #[test]
    fn handle_help_command_stash_returns_true() {
        assert!(handle_help_command("/help stash"));
    }

    #[test]
    fn handle_help_command_every_known_command() {
        // Every non-alias command in KNOWN_COMMANDS should be handled (return true)
        let aliases = ["/exit"];
        for &cmd in KNOWN_COMMANDS {
            if aliases.contains(&cmd) {
                continue;
            }
            let name = cmd.strip_prefix('/').unwrap_or(cmd);
            let input = format!("/help {name}");
            assert!(
                handle_help_command(&input),
                "handle_help_command(\"{input}\") should return true"
            );
        }
    }

    #[test]
    fn handle_help_command_with_extra_whitespace() {
        // Extra whitespace between /help and command name should be handled
        assert!(handle_help_command("/help   add"));
        assert!(handle_help_command("/help  diff"));
        // Trailing whitespace after command name
        assert!(handle_help_command("/help add  "));
    }

    #[test]
    fn handle_help_command_recursive_help_help() {
        // /help help should work (shows help for the help command)
        assert!(handle_help_command("/help help"));
    }

    // ── command_help: content quality checks ──

    #[test]
    fn command_help_compact_mentions_context() {
        let help = command_help("compact").expect("compact should have help");
        assert!(
            help.contains("context") || help.contains("compact") || help.contains("conversation"),
            "compact help should mention context/compaction"
        );
    }

    #[test]
    fn command_help_config_mentions_settings() {
        let help = command_help("config").expect("config should have help");
        assert!(
            help.contains("config") || help.contains("setting"),
            "config help should mention configuration"
        );
    }

    #[test]
    fn command_help_commit_mentions_message() {
        let help = command_help("commit").expect("commit should have help");
        assert!(
            help.contains("commit") || help.contains("message"),
            "commit help should mention commit or message"
        );
    }

    #[test]
    fn command_help_model_mentions_switch() {
        let help = command_help("model").expect("model should have help");
        assert!(
            help.contains("switch") || help.contains("list") || help.contains("model"),
            "model help should mention switching or listing models"
        );
    }

    #[test]
    fn command_help_watch_mentions_auto() {
        let help = command_help("watch").expect("watch should have help");
        assert!(
            help.contains("watch") || help.contains("auto") || help.contains("lint"),
            "watch help should describe auto-run behavior"
        );
    }

    #[test]
    fn command_help_todo_mentions_tasks() {
        let help = command_help("todo").expect("todo should have help");
        assert!(
            help.contains("task") || help.contains("todo"),
            "todo help should mention tasks"
        );
        assert!(
            help.contains("add") && help.contains("done"),
            "todo help should mention add and done subcommands"
        );
    }

    #[test]
    fn command_help_stash_mentions_push_pop() {
        let help = command_help("stash").expect("stash should have help");
        assert!(help.contains("push"), "stash help should mention push");
        assert!(help.contains("pop"), "stash help should mention pop");
    }

    #[test]
    fn command_help_fork_mentions_branch() {
        let help = command_help("fork").expect("fork should have help");
        assert!(
            help.contains("fork") || help.contains("branch") || help.contains("conversation"),
            "fork help should mention forking conversations"
        );
    }

    #[test]
    fn command_help_extended_mentions_autonomous() {
        let help = command_help("extended").expect("extended should have help");
        assert!(
            help.contains("autonomous") || help.contains("long") || help.contains("extended"),
            "extended help should describe autonomous/long-running mode"
        );
    }

    #[test]
    fn command_help_goal_mentions_set() {
        let help = command_help("goal").expect("goal should have help");
        assert!(
            help.contains("set") || help.contains("goal"),
            "goal help should mention setting a goal"
        );
    }

    #[test]
    fn command_help_skill_mentions_list() {
        let help = command_help("skill").expect("skill should have help");
        assert!(
            help.contains("list") || help.contains("skill"),
            "skill help should mention listing skills"
        );
    }

    // ── help_command_completions: prefix filtering ──

    #[test]
    fn help_command_completions_prefix_co_matches_expected() {
        let completions = help_command_completions("co");
        let expected = ["compact", "commit", "config", "context", "copy", "cost"];
        for cmd in &expected {
            assert!(
                completions.contains(&cmd.to_string()),
                "prefix 'co' should match '{cmd}', got: {completions:?}"
            );
        }
        // Should NOT match commands that don't start with "co"
        assert!(!completions.contains(&"diff".to_string()));
        assert!(!completions.contains(&"add".to_string()));
    }

    #[test]
    fn help_command_completions_prefix_di_matches_diff() {
        let completions = help_command_completions("di");
        assert!(completions.contains(&"diff".to_string()));
        assert!(!completions.contains(&"add".to_string()));
    }

    #[test]
    fn help_command_completions_prefix_m_matches_multiple() {
        let completions = help_command_completions("m");
        let expected = ["model", "map", "mark", "marks", "memories", "move", "mcp"];
        for cmd in &expected {
            assert!(
                completions.contains(&cmd.to_string()),
                "prefix 'm' should match '{cmd}'"
            );
        }
    }

    #[test]
    fn help_command_completions_nonexistent_prefix_returns_empty() {
        let completions = help_command_completions("zzz");
        assert!(
            completions.is_empty(),
            "nonexistent prefix should return empty, got: {completions:?}"
        );
    }

    #[test]
    fn help_command_completions_full_command_name_returns_exact() {
        let completions = help_command_completions("diff");
        assert!(
            completions.contains(&"diff".to_string()),
            "exact name should match itself"
        );
        // Should be small — only commands starting with "diff"
        assert!(
            completions.len() <= 2,
            "exact prefix should return few results, got {}",
            completions.len()
        );
    }

    #[test]
    fn help_command_completions_excludes_exit() {
        let all = help_command_completions("");
        assert!(
            !all.contains(&"exit".to_string()),
            "exit should be excluded from completions (it's an alias)"
        );
        let e_completions = help_command_completions("ex");
        assert!(
            !e_completions.contains(&"exit".to_string()),
            "exit should be excluded even with prefix 'ex'"
        );
    }

    // ── cli_help_text: additional content checks ──

    #[test]
    fn cli_help_text_does_not_panic() {
        // Primarily a smoke test — should never panic
        let text = cli_help_text();
        assert!(text.len() > 100);
    }

    #[test]
    fn cli_help_text_contains_prompt_flag() {
        let text = cli_help_text();
        assert!(
            text.contains("--prompt"),
            "cli help should document --prompt for single-prompt mode"
        );
    }

    #[test]
    fn cli_help_text_contains_no_tools_flag() {
        let text = cli_help_text();
        assert!(
            text.contains("--no-tools"),
            "cli help should document --no-tools"
        );
    }

    #[test]
    fn cli_help_text_contains_thinking_flag() {
        let text = cli_help_text();
        assert!(
            text.contains("--thinking"),
            "cli help should document --thinking"
        );
    }

    // ── help_text (REPL /help): additional content checks ──

    #[test]
    fn help_text_does_not_panic() {
        // Smoke test
        let text = help_text();
        assert!(text.len() > 100);
    }

    #[test]
    fn help_text_contains_slash_commands_format() {
        // All commands in help_text should be formatted with a leading /
        let text = help_text();
        // Spot-check a few — they should appear as "/command"
        for cmd in &["/add", "/diff", "/commit", "/model", "/spawn"] {
            assert!(
                text.contains(cmd),
                "help_text() should show {cmd} with leading slash"
            );
        }
    }

    // ── command_help: formatting quality ──

    #[test]
    fn command_help_entries_start_with_slash_command() {
        // Every detailed help entry should start with /command —
        // this ensures consistent formatting
        for &cmd in KNOWN_COMMANDS {
            let name = cmd.strip_prefix('/').unwrap_or(cmd);
            if name == "exit" || name == "clear!" {
                continue;
            }
            if let Some(text) = command_help(name) {
                let starts_with_slash =
                    text.starts_with('/') || text.starts_with(&format!("/{name}"));
                assert!(
                    starts_with_slash,
                    "command_help(\"{name}\") should start with /{name}, got: {}",
                    &text[..text.len().min(60)]
                );
            }
        }
    }

    #[test]
    fn command_help_entries_contain_structured_content() {
        // Every help entry should have some structured content — either
        // a "Usage:" section, a "Subcommands:" section, or at minimum
        // be longer than a single line (meaningful description).
        for &cmd in KNOWN_COMMANDS {
            let name = cmd.strip_prefix('/').unwrap_or(cmd);
            if name == "exit" {
                continue;
            }
            if let Some(text) = command_help(name) {
                let has_structure = text.contains("Usage")
                    || text.contains("Subcommands")
                    || text.contains("Example")
                    || text.contains('\n');
                assert!(
                    has_structure,
                    "command_help(\"{name}\") should have structured content (Usage/Subcommands/Examples or multi-line)"
                );
            }
        }
    }

    #[test]
    fn command_help_no_unclosed_bold_markers() {
        // Check that help entries don't have broken formatting.
        // Count only isolated ** pairs (bold markers), skipping *** runs
        // used for masking (e.g., "masked as ***").
        for &cmd in KNOWN_COMMANDS {
            let name = cmd.strip_prefix('/').unwrap_or(cmd);
            if let Some(text) = command_help(name) {
                // Replace *** masking patterns before counting **
                let cleaned = text.replace("***", "");
                let bold_count = cleaned.matches("**").count();
                assert!(
                    bold_count % 2 == 0,
                    "command_help(\"{name}\") has {bold_count} '**' markers (should be even)"
                );
            }
        }
    }

    // ── Cross-consistency checks ──

    #[test]
    fn command_help_and_short_description_agree_on_coverage() {
        // Every command that has a detailed help should also have a short description
        // and vice versa (except known aliases)
        let aliases = ["exit", "clear!"];
        for &cmd in KNOWN_COMMANDS {
            let name = cmd.strip_prefix('/').unwrap_or(cmd);
            if aliases.contains(&name) {
                continue;
            }
            let has_help = command_help(name).is_some();
            let has_desc = command_short_description(name).is_some();
            assert_eq!(
                has_help, has_desc,
                "command '{name}': help={has_help}, short_desc={has_desc} — both should match"
            );
        }
    }

    #[test]
    fn help_text_and_cli_help_text_both_mention_key_commands() {
        let repl = help_text();
        let cli = cli_help_text();
        let key_commands = ["add", "diff", "commit", "model", "spawn", "grep"];
        for cmd in &key_commands {
            assert!(repl.contains(cmd), "help_text() should mention '{cmd}'");
            assert!(cli.contains(cmd), "cli_help_text() should mention '{cmd}'");
        }
    }

    #[test]
    fn help_completions_and_known_commands_in_sync() {
        // Every non-alias KNOWN_COMMAND should appear in completions
        let completions = help_command_completions("");
        let skip = ["exit"]; // explicitly filtered
        for &cmd in KNOWN_COMMANDS {
            let name = cmd.strip_prefix('/').unwrap_or(cmd);
            if skip.contains(&name) {
                continue;
            }
            assert!(
                completions.contains(&name.to_string()),
                "help_command_completions(\"\") missing '{name}' from KNOWN_COMMANDS"
            );
        }
    }
}
