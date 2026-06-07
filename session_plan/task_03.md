Title: Effort levels — /effort command for preset mode switching
Files: src/commands_config.rs, src/dispatch.rs, src/commands.rs
Issue: none

## Description

Add `/effort` command that sets a named effort level, adjusting multiple settings at once. This addresses Claude Code gap #6 (effort levels: low→medium→high→max).

### Implementation

**In `src/commands_config.rs`:**

Add effort level system:

```rust
pub const EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "max"];

pub fn handle_effort(args: &str, agent: &mut Agent) -> String {
    // Parse the level from args
    // Apply the preset settings
    // Return confirmation message
}
```

Effort level presets:

| Level | Thinking | Max Auto-Continues | Description |
|-------|----------|-------------------|-------------|
| `low` | `off` | 0 | Quick answers, no extended thinking, no auto-continue |
| `medium` | `low` | 2 | Balanced — light thinking, limited follow-up |
| `high` | `medium` | 5 | Deep work — extended thinking, full auto-continue |
| `max` | `high` | 10 | Maximum effort — highest thinking, aggressive follow-up |

Implementation details:
- Store current effort level in a `static OnceLock<Mutex<String>>` (or just set the underlying settings directly without tracking the level name).
- Actually: just apply the settings directly. Use the existing `agent.set_thinking_level()` API and the `set_max_auto_continues()` function (or equivalent global).
- Show current effort: `/effort` with no args prints the closest matching level based on current settings.
- Show available: `/effort` with no args also lists available levels.

For thinking level, use the existing `agent.config_mut()` or `agent.set_thinking()` — check what yoagent provides. The REPL already handles `/think <level>` so look at how that works and replicate.

For max_auto_continues, check how `get_max_auto_continues()` in `repl.rs` works — it likely reads from a global or config. Set it the same way.

**In `src/dispatch.rs`:**
- Add `/effort` to command routing. Route to `commands_config::handle_effort`.

**In `src/commands.rs`:**
- Add `"effort"` to `KNOWN_COMMANDS`.
- Add tab completion: `/effort <Tab>` should show `EFFORT_LEVELS`.
- Add to `command_arg_completions` and `command_arg_hint`.

### Behavior

```
> /effort
Current effort: medium (thinking=low, auto-continues=2)
Available: low, medium, high, max

> /effort high
✓ Effort set to high (thinking=medium, auto-continues=5)

> /effort max
✓ Effort set to max (thinking=high, auto-continues=10)
```

### Tests

- Test each effort level applies correct thinking level
- Test `/effort` with no args doesn't crash
- Test invalid level returns helpful error with available options

### Notes
- This is a convenience command — it doesn't add new capability, it makes existing settings easier to use together.
- Keep it simple: just set thinking + auto-continues. Don't try to also change context window or tool restrictions in v1.
- Check how `/think` command works in the dispatch — effort should work similarly but set multiple things.
