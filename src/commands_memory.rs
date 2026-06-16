//! `/remember`, `/memories`, and `/forget` REPL command handlers.
//!
//! Extracted from `commands.rs` as another slice of issue #260, which tracks
//! splitting the multi-thousand-line `commands.rs` into focused modules.
//! These three handlers form a coherent unit — they all operate on
//! `memory::ProjectMemory` through helpers already living in `src/memory.rs`,
//! so the move is purely mechanical and carries no behavioral risk.

use crate::format::*;
use crate::memory::{
    add_memory_with_category, format_relative_time, is_valid_category, load_memories,
    remove_memory, save_memories, search_memories, MEMORY_CATEGORIES,
};

// ── /remember ────────────────────────────────────────────────────────────

/// Parse an optional `[category:X]` prefix from a note string.
/// Returns `(category, remaining_note)`. If no prefix, category is `"general"`.
fn parse_category_prefix(note: &str) -> (&str, &str) {
    if let Some(rest) = note.strip_prefix("[category:") {
        if let Some(end) = rest.find(']') {
            let cat = &rest[..end];
            let remaining = rest[end + 1..].trim();
            if is_valid_category(cat) && !remaining.is_empty() {
                return (cat, remaining);
            }
        }
    }
    ("general", note)
}

pub fn handle_remember(input: &str) {
    let note = input
        .strip_prefix("/remember")
        .unwrap_or("")
        .trim()
        .to_string();
    if note.is_empty() {
        println!("{DIM}  usage: /remember [category:TYPE] <note>");
        println!("  Save a project-specific memory that persists across sessions.");
        println!("  Categories: {}", MEMORY_CATEGORIES.join(", "));
        println!("  Examples:");
        println!("    /remember this project uses sqlx for database access");
        println!("    /remember [category:build] always run cargo fmt before committing");
        println!("    /remember [category:bug] watch out for UTF-8 panics in truncate{RESET}\n");
        return;
    }
    let (category, actual_note) = parse_category_prefix(&note);
    let mut memory = load_memories();
    add_memory_with_category(&mut memory, actual_note, category);
    match save_memories(&memory) {
        Ok(_) => {
            let cat_label = if category == "general" {
                String::new()
            } else {
                format!(" [{category}]")
            };
            println!(
                "{GREEN}  ✓ Remembered{cat_label}: \"{actual_note}\" ({} total memories){RESET}\n",
                memory.entries.len()
            );
        }
        Err(e) => {
            eprintln!("{RED}  error saving memory: {e}{RESET}\n");
        }
    }
}

// ── /memories ────────────────────────────────────────────────────────────

pub fn handle_memories(input: &str) {
    let query = input.strip_prefix("/memories").unwrap_or("").trim();

    let memory = load_memories();
    if memory.entries.is_empty() {
        println!("{DIM}  No project memories yet.");
        println!("  Use /remember <note> to add one.{RESET}\n");
        return;
    }

    if query.is_empty() {
        // Show all memories
        println!("{DIM}  Project memories ({}):", memory.entries.len());
        for (i, entry) in memory.entries.iter().enumerate() {
            let cat_tag = format!("[{}]", entry.category);
            println!(
                "    [{i}] {cat_tag} {} ({})",
                entry.note,
                format_relative_time(&entry.timestamp)
            );
        }
        println!("  Use /forget <n> to remove a memory.{RESET}\n");
    } else {
        // Search memories
        let results = search_memories(&memory, query);
        if results.is_empty() {
            println!("{DIM}  No memories matching '{query}'.{RESET}\n");
        } else {
            println!(
                "{DIM}  Found {} {} matching '{query}':",
                results.len(),
                if results.len() == 1 {
                    "memory"
                } else {
                    "memories"
                }
            );
            for (i, entry) in &results {
                let cat_tag = format!("[{}]", entry.category);
                println!(
                    "    [{i}] {cat_tag} {} ({})",
                    entry.note,
                    format_relative_time(&entry.timestamp)
                );
            }
            println!("  Use /forget <n> to remove a memory.{RESET}\n");
        }
    }
}

// ── /forget ──────────────────────────────────────────────────────────────

pub fn handle_forget(input: &str) {
    let arg = input.strip_prefix("/forget").unwrap_or("").trim();
    if arg.is_empty() {
        println!("{DIM}  usage: /forget <n>");
        println!("  Remove a project memory by index. Use /memories to see indexes.{RESET}\n");
        return;
    }
    let index = match arg.parse::<usize>() {
        Ok(i) => i,
        Err(_) => {
            eprintln!("{RED}  error: '{arg}' is not a valid index. Use /memories to see indexes.{RESET}\n");
            return;
        }
    };
    let mut memory = load_memories();
    match remove_memory(&mut memory, index) {
        Some(removed) => match save_memories(&memory) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ Forgot: \"{}\" ({} memories remaining){RESET}\n",
                    removed.note,
                    memory.entries.len()
                );
            }
            Err(e) => {
                eprintln!("{RED}  error saving memory: {e}{RESET}\n");
            }
        },
        None => {
            eprintln!(
                "{RED}  error: index {index} out of range (have {} memories). Use /memories to see indexes.{RESET}\n",
                memory.entries.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
    use crate::memory::{
        add_memory, format_memories_for_prompt, load_memories_from, remove_memory, search_memories,
        MemoryEntry, ProjectMemory,
    };

    #[test]
    fn test_remember_command_recognized() {
        assert!(!is_unknown_command("/remember"));
        assert!(!is_unknown_command("/remember this uses sqlx"));
        assert!(
            KNOWN_COMMANDS.contains(&"/remember"),
            "/remember should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_memories_command_recognized() {
        assert!(!is_unknown_command("/memories"));
        assert!(
            KNOWN_COMMANDS.contains(&"/memories"),
            "/memories should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_forget_command_recognized() {
        assert!(!is_unknown_command("/forget"));
        assert!(!is_unknown_command("/forget 0"));
        assert!(
            KNOWN_COMMANDS.contains(&"/forget"),
            "/forget should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_remember_command_matching() {
        let remember_matches = |s: &str| s == "/remember" || s.starts_with("/remember ");
        assert!(remember_matches("/remember"));
        assert!(remember_matches("/remember this uses sqlx"));
        assert!(!remember_matches("/remembering"));
        assert!(!remember_matches("/remembrance"));
    }

    #[test]
    fn test_forget_command_matching() {
        let forget_matches = |s: &str| s == "/forget" || s.starts_with("/forget ");
        assert!(forget_matches("/forget"));
        assert!(forget_matches("/forget 0"));
        assert!(forget_matches("/forget 42"));
        assert!(!forget_matches("/forgetting"));
        assert!(!forget_matches("/forgetful"));
    }

    #[test]
    fn test_memory_crud_roundtrip() {
        use std::fs;
        let dir = std::env::temp_dir().join("yoyo_test_memory_cmd_crud");
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("memory.json");

        // Start empty
        let mut mem = load_memories_from(&path);
        assert!(mem.entries.is_empty());

        // Add
        add_memory(&mut mem, "uses sqlx");
        add_memory(&mut mem, "docker needed");
        assert_eq!(mem.entries.len(), 2);

        // Save & reload
        crate::memory::save_memories_to(&mem, &path).unwrap();
        let reloaded = load_memories_from(&path);
        assert_eq!(reloaded.entries.len(), 2);
        assert_eq!(reloaded.entries[0].note, "uses sqlx");

        // Remove
        let mut reloaded = reloaded;
        let removed = remove_memory(&mut reloaded, 0);
        assert_eq!(removed.unwrap().note, "uses sqlx");
        assert_eq!(reloaded.entries.len(), 1);
        assert_eq!(reloaded.entries[0].note, "docker needed");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_memory_format_for_prompt_integration() {
        let memory = ProjectMemory {
            entries: vec![MemoryEntry {
                note: "always run cargo fmt".to_string(),
                timestamp: "2026-03-15 08:00".to_string(),
                category: "general".to_string(),
            }],
        };
        let prompt = format_memories_for_prompt(&memory);
        assert!(prompt.is_some());
        let prompt = prompt.unwrap();
        assert!(prompt.contains("Project Memories"));
        assert!(prompt.contains("always run cargo fmt"));
    }

    #[test]
    fn test_memories_command_with_search_arg() {
        // Verify that /memories with an argument is still recognized
        // (it should match via starts_with pattern in repl.rs)
        assert!(!is_unknown_command("/memories"));
    }

    #[test]
    fn test_search_memories_from_command() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "uses sqlx for DB".to_string(),
                    timestamp: "t0".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "docker required".to_string(),
                    timestamp: "t1".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "sqlx migrations in ./migrations".to_string(),
                    timestamp: "t2".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let results = search_memories(&memory, "sqlx");
        assert_eq!(results.len(), 2);
        // "uses sqlx for DB" is shorter (more focused) so scores slightly
        // higher than "sqlx migrations in ./migrations" despite later position
        assert_eq!(results[0].0, 0);
        assert_eq!(results[1].0, 2);

        let results = search_memories(&memory, "python");
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_category_prefix_with_valid_category() {
        use super::parse_category_prefix;
        let (cat, note) = parse_category_prefix("[category:build] always run cargo fmt");
        assert_eq!(cat, "build");
        assert_eq!(note, "always run cargo fmt");
    }

    #[test]
    fn test_parse_category_prefix_without_category() {
        use super::parse_category_prefix;
        let (cat, note) = parse_category_prefix("just a plain note");
        assert_eq!(cat, "general");
        assert_eq!(note, "just a plain note");
    }

    #[test]
    fn test_parse_category_prefix_invalid_category() {
        use super::parse_category_prefix;
        // Invalid category falls back to general, entire string is the note
        let (cat, note) = parse_category_prefix("[category:unknown] some note");
        assert_eq!(cat, "general");
        assert_eq!(note, "[category:unknown] some note");
    }

    #[test]
    fn test_parse_category_prefix_all_valid_categories() {
        use super::parse_category_prefix;
        for &valid_cat in crate::memory::MEMORY_CATEGORIES {
            let input = format!("[category:{valid_cat}] test note");
            let (cat, note) = parse_category_prefix(&input);
            assert_eq!(cat, valid_cat, "category {valid_cat} should be parsed");
            assert_eq!(note, "test note");
        }
    }

    #[test]
    fn test_parse_category_prefix_empty_note_after_prefix() {
        use super::parse_category_prefix;
        // If note after prefix is empty, treat whole string as note with general category
        let (cat, note) = parse_category_prefix("[category:build]");
        assert_eq!(cat, "general");
        assert_eq!(note, "[category:build]");
    }

    #[test]
    fn test_parse_category_prefix_whitespace_only_after_prefix() {
        use super::parse_category_prefix;
        let (cat, note) = parse_category_prefix("[category:build]   ");
        assert_eq!(cat, "general");
        assert_eq!(note, "[category:build]   ");
    }
}
