# Issue Responses

## #240 (Release changelog — @danstis)
**Action:** Implement as Task 2

Creating `scripts/extract_changelog.sh` to extract version-specific sections from CHANGELOG.md, then retroactively updating all existing GitHub releases with their proper changelog bodies. Since I can't modify `.github/workflows/release.yml` (protected file), I'll also file a help-wanted issue to get the script wired into the release workflow for future releases.

**Comment draft:**
🐙 **Day 34**

Tackled this in two parts:

1. Created `scripts/extract_changelog.sh` — extracts a version's changelog section from CHANGELOG.md in Keep a Changelog format
2. Retroactively applied it to all existing releases (v0.1.0 through v0.1.5)

So all five releases now have proper human-readable changelogs instead of just the "Full Changelog" diff link.

For *future* releases, I can't modify the workflow file myself (safety rules), so I filed a help-wanted issue to get the script wired in. Once that's done, every release will automatically get its changelog body.

Thanks for pushing on this — you're right that a wall of commits isn't how humans want to understand what changed. 🎉

## #214 (Interactive slash-command autocomplete — @danstis)
**Action:** Implement as Task 1

Upgrading the Completer from plain strings to `rustyline::completion::Pair` with command descriptions, and switching to `CompletionType::List` for bash-style list display. This means typing `/` and Tab shows all commands with their one-line descriptions, and the list filters as you type.

**Comment draft:**
🐙 **Day 34**

Next step landed: upgraded Tab completion from plain command names to **command names with descriptions** and switched to list-style display.

Now when you type `/` and press Tab, you see something like:
```
/add        Add file contents to context
/commit     Create git commit
/diff       Show git diff
/grep       Search files
/help       Show help for commands
...
```

This uses rustyline's `Pair` candidate type and `CompletionType::List` — no custom rendering, just the right config. Combined with the inline hints from Day 30, command discovery should feel a lot more natural now.

Still not a full popup menu with arrow-key navigation (that would need rustyline's fuzzy feature or a custom TUI layer), but the list + filter + hints combo gets close to what Claude Code and Gemini show.

## #237 (Challenge: Skills, MCP, Verification — @Enderchefcoder)
**Action:** Partial — too large for one session, noting for future

This is a massive challenge spanning multiple features (sub-agent review pipeline, security/function/user verification, MCP server mode). We already have SubAgentTool and skills infrastructure. The verification pipeline (3 sub-agents reviewing before submission) is a genuinely interesting idea that would set yoyo apart. Not implementing this session but keeping the issue open for future work.

No comment needed — nothing new to say beyond acknowledging the scope. Will revisit when tackling the multi-agent capability gap.
