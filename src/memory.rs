//! Project memory system for yoyo.
//!
//! Persists project-specific notes across sessions in `.yoyo/memory.json`.
//! Each memory is a `{note, timestamp}` pair stored as a JSON array.
//! Users can add memories with `/remember`, list with `/memories`, remove with `/forget`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Valid memory categories.
pub const MEMORY_CATEGORIES: &[&str] = &["general", "build", "convention", "architecture", "bug"];

/// Default category for memories without one (backward compatibility).
fn default_category() -> String {
    "general".to_string()
}

/// Check whether a string is a valid memory category.
pub fn is_valid_category(cat: &str) -> bool {
    MEMORY_CATEGORIES.contains(&cat)
}

/// A single project memory entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEntry {
    pub note: String,
    pub timestamp: String,
    #[serde(default = "default_category")]
    pub category: String,
}

/// The in-memory store of project memories.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectMemory {
    pub entries: Vec<MemoryEntry>,
}

/// The directory name for yoyo project data.
const YOYO_DIR: &str = ".yoyo";

/// The filename for the memory store within `.yoyo/`.
const MEMORY_FILE: &str = "memory.json";

/// Get the path to the memory file for the current project.
pub fn memory_file_path() -> PathBuf {
    Path::new(YOYO_DIR).join(MEMORY_FILE)
}

/// Load project memories from `.yoyo/memory.json`.
/// Returns an empty `ProjectMemory` if the file doesn't exist or can't be parsed.
pub fn load_memories() -> ProjectMemory {
    load_memories_from(&memory_file_path())
}

/// Load project memories from a specific path (for testing).
pub fn load_memories_from(path: &Path) -> ProjectMemory {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => ProjectMemory::default(),
    }
}

/// Save project memories to `.yoyo/memory.json`.
/// Creates the `.yoyo/` directory if it doesn't exist.
pub fn save_memories(memory: &ProjectMemory) -> Result<(), String> {
    save_memories_to(memory, &memory_file_path())
}

/// Save project memories to a specific path (for testing).
pub fn save_memories_to(memory: &ProjectMemory, path: &Path) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }
    let json =
        serde_json::to_string_pretty(memory).map_err(|e| format!("Serialization error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Return entries whose timestamp parses to an epoch ≥ `since_epoch`.
/// Entries with unparseable timestamps are silently skipped.
pub fn memories_since(memory: &ProjectMemory, since_epoch: i64) -> Vec<&MemoryEntry> {
    memory
        .entries
        .iter()
        .filter(|entry| {
            parse_timestamp_to_epoch(entry.timestamp.trim())
                .map(|ts| ts >= since_epoch)
                .unwrap_or(false)
        })
        .collect()
}

/// Add a new memory entry with the current timestamp and default category ("general").
pub fn add_memory(memory: &mut ProjectMemory, note: &str) {
    add_memory_with_category(memory, note, "general");
}

/// Add a new memory entry with the current timestamp and a specific category.
pub fn add_memory_with_category(memory: &mut ProjectMemory, note: &str, category: &str) {
    let timestamp = current_timestamp();
    let cat = if is_valid_category(category) {
        category.to_string()
    } else {
        default_category()
    };
    memory.entries.push(MemoryEntry {
        note: note.to_string(),
        timestamp,
        category: cat,
    });
}

/// Build a concise note describing a successful watch-fix recovery.
pub fn build_fix_memory_note(watch_cmd: &str, attempt: usize) -> String {
    format!(
        "Watch fix: '{}' failed, fixed on attempt {}",
        watch_cmd, attempt
    )
}

/// Build a learning note from a watch-mode fix, capturing the *project fact* discovered.
///
/// Takes the watch command and the error hint (from `error_category_hint`) and
/// constructs a note that captures the specific project knowledge learned from
/// the fix. Returns `None` if the error hint is too generic to be worth
/// remembering as a project fact (e.g. "other" category).
pub fn build_learn_memory_note(
    watch_cmd: &str,
    error_messages: &[String],
    error_category: &str,
) -> Option<String> {
    // Only learn from recognizable patterns — generic "other" errors are too vague
    if error_category == "other" || error_messages.is_empty() {
        return None;
    }

    // Extract the most informative error message (first one, trimmed)
    let primary_msg = error_messages[0].trim();
    if primary_msg.is_empty() {
        return None;
    }

    // Truncate long error messages to keep notes concise
    let msg_display: String = primary_msg.chars().take(120).collect();
    let msg_display = if msg_display.len() < primary_msg.len() {
        format!("{msg_display}…")
    } else {
        msg_display
    };

    // Map category to a useful prefix that categorizes the memory
    let prefix = match error_category {
        "import" => "[build] Missing import pattern",
        "type" => "[build] Type issue",
        "borrow" => "[build] Ownership pattern",
        "lifetime" => "[build] Lifetime pattern",
        "unused" => "[build] Lint: unused code",
        "syntax" => "[build] Syntax issue",
        "test_assertion" => "[bug] Test expectation",
        _ => return None,
    };

    Some(format!("{prefix} in `{watch_cmd}`: {msg_display}",))
}

/// Automatically remember a note if it's not a near-duplicate of an existing memory.
///
/// Loads memories from disk, checks for duplicates (substring match on first 50 chars),
/// saves if new, and prints a dim confirmation. Returns `true` if a memory was saved.
pub fn auto_remember(note: &str) -> bool {
    auto_remember_to(note, &memory_file_path())
}

/// Maximum number of entries the automatic (watch-fix) path will store.
///
/// A judgment number, not a measurement: every stored memory is injected into the prompt
/// by [`format_memories_for_prompt`], so the auto path needs *some* ceiling. Dedup is exact
/// (whole-note), so without a cap near-identical notes — compiler errors carry process ids
/// and `file:line` positions — could accumulate without bound. 20 is roughly a screenful and
/// well under any context budget; `/forget` is how a user makes room.
pub const MAX_AUTO_MEMORIES: usize = 20;

/// Testable variant of [`auto_remember`] that writes to a specific path.
pub fn auto_remember_to(note: &str, path: &Path) -> bool {
    let mut memory = load_memories_from(path);

    // Dedup on the WHOLE note, not a fixed-width head. The old 50-char prefix compare (#672)
    // was consumed entirely by the note template plus the watch command, so distinct
    // learnings about the same command were silently discarded forever.
    let key = note.trim();
    let is_dup = memory.entries.iter().any(|e| e.note.trim() == key);
    if is_dup {
        return false;
    }

    // Honest dedup means unbounded growth is possible, so the auto path is capped — and says
    // so out loud. Silence when full is the same failure shape as the bug above: the user
    // must be able to tell "already known" from "I stopped learning".
    if memory.entries.len() >= MAX_AUTO_MEMORIES {
        eprintln!(
            "  ⚠ memory full ({MAX_AUTO_MEMORIES}) — not auto-saving; use /forget to make room"
        );
        return false;
    }

    add_memory(&mut memory, note);
    if let Err(e) = save_memories_to(&memory, path) {
        eprintln!("  ⚠ auto-remember failed to save: {e}");
        return false;
    }

    // Truncate for display (char-boundary safe)
    let display: String = note.chars().take(60).collect();
    let display = if display.len() < note.len() {
        format!("{display}…")
    } else {
        display
    };
    eprintln!("  💾 Auto-remembered: {display}");
    true
}

/// Remove a memory entry by index (0-based).
/// Returns the removed entry, or None if the index is out of bounds.
pub fn remove_memory(memory: &mut ProjectMemory, index: usize) -> Option<MemoryEntry> {
    if index < memory.entries.len() {
        Some(memory.entries.remove(index))
    } else {
        None
    }
}

/// Score a memory note against a query using fuzzy matching.
///
/// Returns `None` for non-matches and `Some(score)` for matches.
/// Higher scores indicate better matches.
///
/// Matching rules:
/// - Single word query: case-insensitive substring match
/// - Multi-word query: ALL words must appear in the note (AND semantics, case-insensitive)
///
/// Score boosters:
/// - Exact substring match of the full query → highest base score
/// - Word-boundary matches (word starts at beginning of a note word) → bonus
/// - Consecutive word matches in the note → bonus
/// - Shorter notes (more focused) → bonus
pub fn fuzzy_score_memory(note: &str, query: &str) -> Option<f64> {
    if query.is_empty() {
        // Empty query matches everything with a neutral score
        return Some(1.0);
    }

    let note_lower = note.to_lowercase();
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    if query_words.is_empty() {
        return Some(1.0);
    }

    if query_words.len() == 1 {
        // Single word: substring match
        let word = query_words[0];
        let pos = note_lower.find(word)?;
        return Some(single_word_score(&note_lower, word, pos));
    }

    // Multi-word: all words must be present (AND semantics)
    let mut word_positions: Vec<usize> = Vec::with_capacity(query_words.len());
    for word in &query_words {
        let pos = note_lower.find(word)?;
        word_positions.push(pos);
    }

    // Base score for matching all words
    let mut score = 10.0;

    // Bonus for exact full-query substring match
    if note_lower.contains(&query_lower) {
        score += 20.0;
    }

    // Word-boundary bonus: each query word that starts at a word boundary in the note
    for (i, word) in query_words.iter().enumerate() {
        let pos = word_positions[i];
        if is_word_boundary(&note_lower, pos) {
            score += 3.0;
        }
        // Bonus if word appears early in note
        let position_bonus = 1.0 / (1.0 + pos as f64 / 20.0);
        score += position_bonus;
        // Extra bonus if exact word (bounded on both sides)
        if is_exact_word(&note_lower, word, pos) {
            score += 2.0;
        }
    }

    // Consecutive-match bonus: check if query words appear in order
    let mut sorted_positions = word_positions.clone();
    sorted_positions.sort_unstable();
    let in_order = word_positions == sorted_positions;
    if in_order {
        score += 5.0;
    }

    // Check for consecutive (adjacent) word matches in the note
    if in_order && word_positions.len() > 1 {
        let mut consecutive_count = 0;
        for j in 1..word_positions.len() {
            let prev_end = word_positions[j - 1] + query_words[j - 1].len();
            // Allow a small gap (whitespace, punctuation) between consecutive matches
            let gap = word_positions[j].saturating_sub(prev_end);
            if gap <= 3 {
                consecutive_count += 1;
            }
        }
        score += consecutive_count as f64 * 3.0;
    }

    // Shorter notes are more focused → small bonus
    let length_bonus = 10.0 / (1.0 + note.len() as f64 / 30.0);
    score += length_bonus;

    Some(score)
}

/// Score a single-word match.
fn single_word_score(note_lower: &str, word: &str, pos: usize) -> f64 {
    let mut score = 10.0;

    // Bonus for word-boundary match
    if is_word_boundary(note_lower, pos) {
        score += 5.0;
    }

    // Bonus for exact word match (bounded on both sides)
    if is_exact_word(note_lower, word, pos) {
        score += 3.0;
    }

    // Earlier position in note → better
    let position_bonus = 2.0 / (1.0 + pos as f64 / 20.0);
    score += position_bonus;

    // Shorter notes → more focused
    let length_bonus = 5.0 / (1.0 + note_lower.len() as f64 / 30.0);
    score += length_bonus;

    score
}

/// Check if position `pos` in `text` is at a word boundary (start of a word).
fn is_word_boundary(text: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    text.as_bytes()
        .get(pos.wrapping_sub(1))
        .map(|&b| !b.is_ascii_alphanumeric() && b != b'_')
        .unwrap_or(true)
}

/// Check if the match at `pos` for `word` is an exact word (bounded on both sides).
fn is_exact_word(text: &str, word: &str, pos: usize) -> bool {
    let end = pos + word.len();
    let start_ok = is_word_boundary(text, pos);
    let end_ok = end >= text.len()
        || text
            .as_bytes()
            .get(end)
            .map(|&b| !b.is_ascii_alphanumeric() && b != b'_')
            .unwrap_or(true);
    start_ok && end_ok
}

/// Search memories with fuzzy matching and relevance scoring.
///
/// Returns a vec of `(index, &MemoryEntry)` sorted by relevance (highest first).
/// An empty query matches all entries (sorted by recency, newest first).
///
/// For non-empty queries, results are scored by `fuzzy_score_memory()` with a
/// recency bonus for newer memories.
pub fn search_memories<'a>(
    memory: &'a ProjectMemory,
    query: &str,
) -> Vec<(usize, &'a MemoryEntry)> {
    if query.trim().is_empty() {
        // Empty query: return all, newest first
        let mut results: Vec<(usize, &MemoryEntry)> = memory.entries.iter().enumerate().collect();
        results.reverse();
        return results;
    }

    let now = now_epoch_secs();

    let mut scored: Vec<(usize, &MemoryEntry, f64)> = memory
        .entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            let mut score = fuzzy_score_memory(&entry.note, query)?;

            // Recency bonus: newer memories get a small boost
            if let Some(ts_epoch) = parse_timestamp_to_epoch(&entry.timestamp) {
                let age_days = (now - ts_epoch).max(0) as f64 / 86400.0;
                // Recency bonus decays over ~30 days, max +5.0
                let recency = 5.0 / (1.0 + age_days / 7.0);
                score += recency;
            }

            Some((i, entry, score))
        })
        .collect();

    // Sort by score descending (highest relevance first)
    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    scored.into_iter().map(|(i, entry, _)| (i, entry)).collect()
}

/// Format memories for display in the system prompt.
/// Returns None if there are no memories.
///
/// When there are 3 or more memories, groups them by category with headers.
/// Below 3, keeps the flat list for simplicity.
pub fn format_memories_for_prompt(memory: &ProjectMemory) -> Option<String> {
    if memory.entries.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    lines.push("## Project Memories".to_string());
    lines.push(String::new());

    if memory.entries.len() < 3 {
        // Flat list for small counts
        for entry in &memory.entries {
            lines.push(format!("- {} ({})", entry.note, entry.timestamp));
        }
    } else {
        // Group by category
        let category_order = MEMORY_CATEGORIES;
        for &cat in category_order {
            let cat_entries: Vec<_> = memory
                .entries
                .iter()
                .filter(|e| e.category == cat)
                .collect();
            if cat_entries.is_empty() {
                continue;
            }
            lines.push(format!("### {}", capitalize_category(cat)));
            for entry in &cat_entries {
                lines.push(format!("- {} ({})", entry.note, entry.timestamp));
            }
            lines.push(String::new());
        }
    }

    Some(lines.join("\n"))
}

/// Capitalize a category name for display (e.g., "build" → "Build").
fn capitalize_category(cat: &str) -> String {
    let mut chars = cat.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Format an ISO-like timestamp as a human-readable relative time.
///
/// Accepts timestamps in the `YYYY-MM-DD HH:MM` format (as stored by
/// `current_timestamp()`). Returns strings like "just now", "5m ago",
/// "2h ago", "3d ago", "2w ago", "3mo ago".
///
/// On parse failure, returns the original timestamp unchanged.
pub fn format_relative_time(iso_timestamp: &str) -> String {
    format_relative_time_from(iso_timestamp, now_epoch_secs())
}

/// Testable inner implementation — takes an explicit "now" epoch.
fn format_relative_time_from(iso_timestamp: &str, now_secs: i64) -> String {
    let ts_secs = match parse_timestamp_to_epoch(iso_timestamp.trim()) {
        Some(s) => s,
        None => return iso_timestamp.to_string(),
    };
    let diff = now_secs.saturating_sub(ts_secs);
    if diff < 0 {
        return iso_timestamp.to_string();
    }
    let diff = diff as u64;

    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    const WEEK: u64 = 7 * DAY;
    const MONTH: u64 = 30 * DAY;

    if diff < MINUTE {
        "just now".to_string()
    } else if diff < HOUR {
        format!("{}m ago", diff / MINUTE)
    } else if diff < DAY {
        format!("{}h ago", diff / HOUR)
    } else if diff < WEEK {
        format!("{}d ago", diff / DAY)
    } else if diff < MONTH {
        format!("{}w ago", diff / WEEK)
    } else {
        format!("{}mo ago", diff / MONTH)
    }
}

/// Parse `YYYY-MM-DD HH:MM` (local time) into epoch seconds.
/// Returns `None` on any parse failure.
pub(crate) fn parse_timestamp_to_epoch(s: &str) -> Option<i64> {
    // Expected format: "2026-03-15 08:32"
    if s.len() < 16 {
        return None;
    }
    let year: i32 = s.get(0..4)?.parse().ok()?;
    if s.as_bytes().get(4)? != &b'-' {
        return None;
    }
    let month: u32 = s.get(5..7)?.parse().ok()?;
    if s.as_bytes().get(7)? != &b'-' {
        return None;
    }
    let day: u32 = s.get(8..10)?.parse().ok()?;
    if s.as_bytes().get(10)? != &b' ' {
        return None;
    }
    let hour: u32 = s.get(11..13)?.parse().ok()?;
    if s.as_bytes().get(13)? != &b':' {
        return None;
    }
    let min: u32 = s.get(14..16)?.parse().ok()?;

    // Validate ranges
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour >= 24 || min >= 60 {
        return None;
    }

    // Convert to epoch using the same local-time assumption as `date`.
    // We shell out to `date -d` (Linux) or `date -j -f` (macOS) for
    // portability — but that's fragile. Instead, compute a UTC-ish epoch
    // inline and accept the same local-time offset error that `date +%Y-%m-%d`
    // introduces. Since both the stored timestamp and "now" share the offset,
    // the *difference* is correct (which is all we need for relative display).

    // Days from year 0 to start of `year`, then add month/day.
    let epoch = simple_local_epoch(year, month, day, hour, min);
    Some(epoch)
}

/// A lightweight local-time epoch (seconds since 1970-01-01 00:00 local).
/// Does NOT account for timezone — but since both stored timestamps and
/// `now_epoch_secs` share the same offset, the difference is correct.
fn simple_local_epoch(year: i32, month: u32, day: u32, hour: u32, min: u32) -> i64 {
    // Days in each month (non-leap)
    const MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    fn is_leap(y: i32) -> bool {
        (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
    }

    // Days from 1970-01-01 to start of `year`
    let mut days: i64 = 0;
    if year >= 1970 {
        for y in 1970..year {
            days += if is_leap(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            days -= if is_leap(y) { 366 } else { 365 };
        }
    }

    // Add months
    for (m, &md) in MONTH_DAYS.iter().enumerate().take((month - 1) as usize) {
        days += md as i64;
        if m == 1 && is_leap(year) {
            days += 1; // Feb in leap year
        }
    }

    // Add days (1-indexed)
    days += (day - 1) as i64;

    days * 86400 + hour as i64 * 3600 + min as i64 * 60
}

/// Current local time as epoch seconds (same basis as `current_timestamp`).
pub(crate) fn now_epoch_secs() -> i64 {
    // Parse the output of `date +%s` for a local-consistent epoch.
    // Falls back to UNIX_EPOCH-based SystemTime if shell fails.
    std::process::Command::new("date")
        .arg("+%Y-%m-%d %H:%M")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8(o.stdout).ok()?;
                parse_timestamp_to_epoch(s.trim())
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        })
}

/// Get the current timestamp in a human-readable format.
fn current_timestamp() -> String {
    // Use a simple approach: shell out to date command for portability
    std::process::Command::new("date")
        .arg("+%Y-%m-%d %H:%M")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_memory_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("yoyo_test_memory_{}", name));
        let _ = fs::create_dir_all(&dir);
        dir.join(MEMORY_FILE)
    }

    fn cleanup(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn test_memory_entry_serialize_deserialize() {
        let entry = MemoryEntry {
            note: "uses sqlx for database access".to_string(),
            timestamp: "2026-03-15 08:32".to_string(),
            category: "general".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn test_project_memory_serialize_deserialize() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "tests require docker running".to_string(),
                    timestamp: "2026-03-15 08:00".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "use pnpm not npm".to_string(),
                    timestamp: "2026-03-15 09:00".to_string(),
                    category: "general".to_string(),
                },
            ],
        };
        let json = serde_json::to_string_pretty(&memory).unwrap();
        let parsed: ProjectMemory = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].note, "tests require docker running");
        assert_eq!(parsed.entries[1].note, "use pnpm not npm");
    }

    #[test]
    fn test_add_memory() {
        let mut memory = ProjectMemory::default();
        assert!(memory.entries.is_empty());

        add_memory(&mut memory, "this project uses sqlx");
        assert_eq!(memory.entries.len(), 1);
        assert_eq!(memory.entries[0].note, "this project uses sqlx");
        assert!(!memory.entries[0].timestamp.is_empty());

        add_memory(&mut memory, "tests need docker");
        assert_eq!(memory.entries.len(), 2);
        assert_eq!(memory.entries[1].note, "tests need docker");
    }

    #[test]
    fn test_remove_memory_valid_index() {
        let mut memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "note 0".to_string(),
                    timestamp: "t0".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "note 1".to_string(),
                    timestamp: "t1".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "note 2".to_string(),
                    timestamp: "t2".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let removed = remove_memory(&mut memory, 1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().note, "note 1");
        assert_eq!(memory.entries.len(), 2);
        assert_eq!(memory.entries[0].note, "note 0");
        assert_eq!(memory.entries[1].note, "note 2");
    }

    #[test]
    fn test_remove_memory_invalid_index() {
        let mut memory = ProjectMemory {
            entries: vec![MemoryEntry {
                note: "only one".to_string(),
                timestamp: "t0".to_string(),
                category: "general".to_string(),
            }],
        };

        let removed = remove_memory(&mut memory, 5);
        assert!(removed.is_none());
        assert_eq!(memory.entries.len(), 1);
    }

    #[test]
    fn test_remove_memory_empty() {
        let mut memory = ProjectMemory::default();
        let removed = remove_memory(&mut memory, 0);
        assert!(removed.is_none());
    }

    #[test]
    fn test_save_and_load_memories() {
        let path = temp_memory_path("save_load");
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "first note".to_string(),
                    timestamp: "2026-03-15 08:00".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "second note".to_string(),
                    timestamp: "2026-03-15 09:00".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let result = save_memories_to(&memory, &path);
        assert!(result.is_ok(), "Save should succeed: {:?}", result);

        let loaded = load_memories_from(&path);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].note, "first note");
        assert_eq!(loaded.entries[1].note, "second note");

        cleanup(&path);
    }

    #[test]
    fn test_load_memories_nonexistent_file() {
        let path = Path::new("/tmp/yoyo_test_nonexistent_12345/memory.json");
        let memory = load_memories_from(path);
        assert!(memory.entries.is_empty());
    }

    #[test]
    fn test_load_memories_invalid_json() {
        let path = temp_memory_path("invalid_json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not valid json at all {{{").unwrap();

        let memory = load_memories_from(&path);
        assert!(
            memory.entries.is_empty(),
            "Invalid JSON should return empty memory"
        );

        cleanup(&path);
    }

    #[test]
    fn test_save_creates_directory() {
        let dir = std::env::temp_dir().join("yoyo_test_memory_create_dir");
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("subdir").join(MEMORY_FILE);

        let memory = ProjectMemory {
            entries: vec![MemoryEntry {
                note: "test".to_string(),
                timestamp: "now".to_string(),
                category: "general".to_string(),
            }],
        };

        let result = save_memories_to(&memory, &path);
        assert!(
            result.is_ok(),
            "Save should create parent dirs: {:?}",
            result
        );
        assert!(path.exists(), "File should exist after save");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_format_memories_for_prompt_empty() {
        let memory = ProjectMemory::default();
        assert!(format_memories_for_prompt(&memory).is_none());
    }

    #[test]
    fn test_format_memories_for_prompt_with_entries() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "uses sqlx".to_string(),
                    timestamp: "2026-03-15 08:00".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "docker needed for tests".to_string(),
                    timestamp: "2026-03-15 09:00".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let prompt = format_memories_for_prompt(&memory).unwrap();
        assert!(prompt.contains("## Project Memories"));
        assert!(prompt.contains("uses sqlx"));
        assert!(prompt.contains("docker needed for tests"));
        assert!(prompt.contains("2026-03-15 08:00"));
    }

    #[test]
    fn test_memory_file_path() {
        let path = memory_file_path();
        assert!(path.to_string_lossy().contains(".yoyo"));
        assert!(path.to_string_lossy().contains("memory.json"));
    }

    #[test]
    fn test_full_crud_workflow() {
        let path = temp_memory_path("crud_workflow");

        // Start fresh
        let mut memory = load_memories_from(&path);
        assert!(memory.entries.is_empty());

        // Add entries
        add_memory(&mut memory, "first");
        add_memory(&mut memory, "second");
        add_memory(&mut memory, "third");
        assert_eq!(memory.entries.len(), 3);

        // Save
        save_memories_to(&memory, &path).unwrap();

        // Reload
        let mut loaded = load_memories_from(&path);
        assert_eq!(loaded.entries.len(), 3);
        assert_eq!(loaded.entries[0].note, "first");

        // Remove middle entry
        let removed = remove_memory(&mut loaded, 1);
        assert_eq!(removed.unwrap().note, "second");
        assert_eq!(loaded.entries.len(), 2);

        // Save and reload again
        save_memories_to(&loaded, &path).unwrap();
        let final_load = load_memories_from(&path);
        assert_eq!(final_load.entries.len(), 2);
        assert_eq!(final_load.entries[0].note, "first");
        assert_eq!(final_load.entries[1].note, "third");

        cleanup(&path);
    }

    #[test]
    fn test_search_memories_basic() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "uses sqlx for database".to_string(),
                    timestamp: "t0".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "docker needed for tests".to_string(),
                    timestamp: "t1".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "always run cargo fmt".to_string(),
                    timestamp: "t2".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let results = search_memories(&memory, "docker");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 1); // index 1
        assert_eq!(results[0].1.note, "docker needed for tests");
    }

    #[test]
    fn test_search_memories_case_insensitive() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "Uses SQLx for Database".to_string(),
                    timestamp: "t0".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "docker NEEDED".to_string(),
                    timestamp: "t1".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let results = search_memories(&memory, "SQLX");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.note, "Uses SQLx for Database");

        let results = search_memories(&memory, "needed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_search_memories_no_match() {
        let memory = ProjectMemory {
            entries: vec![MemoryEntry {
                note: "uses sqlx".to_string(),
                timestamp: "t0".to_string(),
                category: "general".to_string(),
            }],
        };

        let results = search_memories(&memory, "python");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_memories_empty_query() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "first".to_string(),
                    timestamp: "t0".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "second".to_string(),
                    timestamp: "t1".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let results = search_memories(&memory, "");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_memories_multiple_matches() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "cargo build first".to_string(),
                    timestamp: "t0".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "docker needed".to_string(),
                    timestamp: "t1".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "cargo fmt before commit".to_string(),
                    timestamp: "t2".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let results = search_memories(&memory, "cargo");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 0);
        assert_eq!(results[1].0, 2);
    }

    // --- format_relative_time tests ---

    fn fixed_now() -> i64 {
        // 2026-05-24 12:00 as local epoch
        simple_local_epoch(2026, 5, 24, 12, 0)
    }

    #[test]
    fn test_relative_time_just_now() {
        let now = fixed_now();
        // 30 seconds ago — "just now"
        let ts = "2026-05-24 12:00"; // same minute
        assert_eq!(format_relative_time_from(ts, now), "just now");
    }

    #[test]
    fn test_relative_time_minutes_ago() {
        let now = fixed_now();
        // 5 minutes ago → 2026-05-24 11:55
        let ts = "2026-05-24 11:55";
        assert_eq!(format_relative_time_from(ts, now), "5m ago");
    }

    #[test]
    fn test_relative_time_hours_ago() {
        let now = fixed_now();
        // 2 hours ago → 2026-05-24 10:00
        let ts = "2026-05-24 10:00";
        let result = format_relative_time_from(ts, now);
        assert_eq!(result, "2h ago");
    }

    #[test]
    fn test_relative_time_days_ago() {
        let now = fixed_now();
        // 3 days ago → 2026-05-21 12:00
        let ts = "2026-05-21 12:00";
        assert_eq!(format_relative_time_from(ts, now), "3d ago");
    }

    #[test]
    fn test_relative_time_weeks_ago() {
        let now = fixed_now();
        // 14 days ago → 2026-05-10 12:00
        let ts = "2026-05-10 12:00";
        assert_eq!(format_relative_time_from(ts, now), "2w ago");
    }

    #[test]
    fn test_relative_time_months_ago() {
        let now = fixed_now();
        // ~90 days ago → 2026-02-23 12:00
        let ts = "2026-02-23 12:00";
        assert_eq!(format_relative_time_from(ts, now), "3mo ago");
    }

    #[test]
    fn test_relative_time_malformed_returns_original() {
        let now = fixed_now();
        let ts = "not-a-timestamp";
        assert_eq!(format_relative_time_from(ts, now), "not-a-timestamp");
    }

    #[test]
    fn test_relative_time_exactly_one_hour() {
        let now = fixed_now();
        // Exactly 1 hour ago → 2026-05-24 11:00
        let ts = "2026-05-24 11:00";
        assert_eq!(format_relative_time_from(ts, now), "1h ago");
    }

    #[test]
    fn test_parse_timestamp_to_epoch_valid() {
        let epoch = parse_timestamp_to_epoch("2026-05-24 12:00");
        assert!(epoch.is_some());
    }

    #[test]
    fn test_parse_timestamp_to_epoch_invalid() {
        assert!(parse_timestamp_to_epoch("garbage").is_none());
        assert!(parse_timestamp_to_epoch("2026-13-01 00:00").is_none()); // bad month
        assert!(parse_timestamp_to_epoch("2026-05-32 00:00").is_none()); // bad day
        assert!(parse_timestamp_to_epoch("2026-05-24 25:00").is_none()); // bad hour
    }

    // --- fuzzy_score_memory tests ---

    #[test]
    fn test_fuzzy_score_single_word_exact_match() {
        let score = fuzzy_score_memory("docker required", "docker");
        assert!(score.is_some());
        let s = score.unwrap();
        assert!(s > 15.0, "exact word-boundary match should score high: {s}");
    }

    #[test]
    fn test_fuzzy_score_single_word_mid_word_match() {
        // "sqlx" inside "postgresql_sqlx_config" — not at word boundary
        let score = fuzzy_score_memory("postgresql_sqlx_config", "sqlx");
        assert!(score.is_some(), "substring should still match");
        let s = score.unwrap();
        // Should be lower than a word-boundary match
        let boundary_score = fuzzy_score_memory("sqlx config", "sqlx").unwrap();
        assert!(
            boundary_score > s,
            "word-boundary match ({boundary_score}) should score higher than mid-word ({s})"
        );
    }

    #[test]
    fn test_fuzzy_score_multi_word_all_present() {
        let score = fuzzy_score_memory("uses sqlx for database access", "sqlx database");
        assert!(score.is_some(), "both words present → should match");
    }

    #[test]
    fn test_fuzzy_score_multi_word_partial_no_match() {
        let score = fuzzy_score_memory("uses sqlx for database access", "sqlx python");
        assert!(score.is_none(), "not all words present → should not match");
    }

    #[test]
    fn test_fuzzy_score_multi_word_missing_one() {
        let score = fuzzy_score_memory("docker needed for tests", "docker python");
        assert!(score.is_none(), "python not in note → no match");
    }

    #[test]
    fn test_fuzzy_score_empty_query_matches_all() {
        let score = fuzzy_score_memory("anything at all", "");
        assert!(score.is_some());
        assert!(
            (score.unwrap() - 1.0).abs() < f64::EPSILON,
            "empty query should return neutral score of 1.0"
        );
    }

    #[test]
    fn test_fuzzy_score_case_insensitive() {
        let score_lower = fuzzy_score_memory("Docker Required", "docker");
        let score_upper = fuzzy_score_memory("Docker Required", "DOCKER");
        assert!(score_lower.is_some());
        assert!(score_upper.is_some());
        assert!(
            (score_lower.unwrap() - score_upper.unwrap()).abs() < f64::EPSILON,
            "case should not affect score"
        );
    }

    #[test]
    fn test_fuzzy_score_ordering_exact_over_boundary_over_midword() {
        // Exact full-query substring match should score highest
        let exact = fuzzy_score_memory("sqlx database config", "sqlx database").unwrap();
        // Words present but not as exact substring
        let scattered = fuzzy_score_memory("database layer uses sqlx", "sqlx database").unwrap();
        // Word boundary single word vs mid-word
        let boundary = fuzzy_score_memory("sqlx config", "sqlx").unwrap();
        let midword = fuzzy_score_memory("nosqlx config", "sqlx").unwrap();

        assert!(
            exact > scattered,
            "exact substring ({exact}) should score higher than scattered ({scattered})"
        );
        assert!(
            boundary > midword,
            "word-boundary ({boundary}) should score higher than mid-word ({midword})"
        );
    }

    #[test]
    fn test_fuzzy_score_shorter_note_bonus() {
        // Same match, but shorter note should score higher
        let short = fuzzy_score_memory("uses docker", "docker").unwrap();
        let long = fuzzy_score_memory(
            "this project uses docker for running integration tests in CI",
            "docker",
        )
        .unwrap();
        assert!(
            short > long,
            "shorter note ({short}) should score higher than longer ({long})"
        );
    }

    #[test]
    fn test_fuzzy_score_no_match_returns_none() {
        assert!(fuzzy_score_memory("uses sqlx for database", "python").is_none());
        assert!(fuzzy_score_memory("", "something").is_none());
    }

    #[test]
    fn test_search_memories_sorted_by_relevance() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "this project has a complex database layer using sqlx".to_string(),
                    timestamp: "t0".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "docker needed for tests".to_string(),
                    timestamp: "t1".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "sqlx config in .env".to_string(),
                    timestamp: "t2".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let results = search_memories(&memory, "sqlx");
        assert_eq!(results.len(), 2);
        // "sqlx config in .env" (shorter, sqlx at word boundary pos 0) should rank
        // higher than the longer note where sqlx appears later
        assert_eq!(
            results[0].1.note, "sqlx config in .env",
            "shorter, earlier-match note should rank first"
        );
    }

    #[test]
    fn test_search_memories_multi_word_filters_correctly() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "uses sqlx for database access".to_string(),
                    timestamp: "t0".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "docker needed for tests".to_string(),
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

        let results = search_memories(&memory, "sqlx database");
        assert_eq!(
            results.len(),
            1,
            "only one note has both 'sqlx' and 'database'"
        );
        assert_eq!(results[0].1.note, "uses sqlx for database access");
    }

    #[test]
    fn test_search_memories_single_word_backward_compatible() {
        // Single-word substring search still works as before
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "uses sqlx for database".to_string(),
                    timestamp: "t0".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "docker needed".to_string(),
                    timestamp: "t1".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let results = search_memories(&memory, "docker");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.note, "docker needed");
    }

    #[test]
    fn test_search_memories_recency_boost() {
        // Two identical notes, but one newer — newer should rank first
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "uses sqlx for database".to_string(),
                    timestamp: "2020-01-01 00:00".to_string(),
                    category: "general".to_string(),
                },
                MemoryEntry {
                    note: "uses sqlx for database".to_string(),
                    timestamp: "2026-05-24 12:00".to_string(),
                    category: "general".to_string(),
                },
            ],
        };

        let results = search_memories(&memory, "sqlx");
        assert_eq!(results.len(), 2);
        // The newer one (index 1) should rank first due to recency bonus
        assert_eq!(results[0].0, 1, "newer memory should rank first");
    }

    #[test]
    fn test_is_word_boundary() {
        assert!(is_word_boundary("docker needed", 0)); // start of string
        assert!(is_word_boundary("uses docker", 5)); // after space
        assert!(!is_word_boundary("nosqlx", 2)); // mid-word
        assert!(is_word_boundary("(docker)", 1)); // after paren
    }

    #[test]
    fn test_is_exact_word() {
        assert!(is_exact_word("docker needed", "docker", 0));
        assert!(is_exact_word("uses docker here", "docker", 5));
        assert!(!is_exact_word("dockerized app", "docker", 0)); // not bounded on right
        assert!(!is_exact_word("mydocker", "docker", 2)); // not bounded on left
    }

    #[test]
    fn test_build_fix_memory_note() {
        let note = build_fix_memory_note("cargo test", 1);
        assert_eq!(note, "Watch fix: 'cargo test' failed, fixed on attempt 1");

        let note = build_fix_memory_note("cargo clippy && cargo test", 3);
        assert_eq!(
            note,
            "Watch fix: 'cargo clippy && cargo test' failed, fixed on attempt 3"
        );
    }

    #[test]
    fn test_build_learn_memory_note_import_error() {
        let msgs = vec!["cannot find value `foo` in this scope".to_string()];
        let note = build_learn_memory_note("cargo build", &msgs, "import");
        assert!(note.is_some());
        let note = note.unwrap();
        assert!(note.starts_with("[build]"));
        assert!(note.contains("cargo build"));
        assert!(note.contains("cannot find value `foo`"));
    }

    #[test]
    fn test_build_learn_memory_note_type_error() {
        let msgs = vec!["expected `usize`, found `&str`".to_string()];
        let note = build_learn_memory_note("cargo test", &msgs, "type");
        assert!(note.is_some());
        let note = note.unwrap();
        assert!(note.starts_with("[build]"));
        assert!(note.contains("Type issue"));
    }

    #[test]
    fn test_build_learn_memory_note_test_assertion() {
        let msgs = vec!["assertion `left == right` failed".to_string()];
        let note = build_learn_memory_note("cargo test", &msgs, "test_assertion");
        assert!(note.is_some());
        let note = note.unwrap();
        assert!(note.starts_with("[bug]"));
        assert!(note.contains("Test expectation"));
    }

    #[test]
    fn test_build_learn_memory_note_other_category_returns_none() {
        let msgs = vec!["some unknown error".to_string()];
        let note = build_learn_memory_note("cargo build", &msgs, "other");
        assert!(note.is_none(), "other category should not produce a note");
    }

    #[test]
    fn test_build_learn_memory_note_empty_messages_returns_none() {
        let msgs: Vec<String> = vec![];
        let note = build_learn_memory_note("cargo build", &msgs, "import");
        assert!(note.is_none(), "empty messages should not produce a note");
    }

    #[test]
    fn test_build_learn_memory_note_truncates_long_message() {
        let long_msg = "x".repeat(200);
        let msgs = vec![long_msg];
        let note = build_learn_memory_note("cargo build", &msgs, "type");
        assert!(note.is_some());
        let note = note.unwrap();
        // Message should be truncated to 120 chars + ellipsis
        assert!(note.contains('…'));
        assert!(note.len() < 200);
    }

    #[test]
    fn test_build_learn_memory_note_all_categories() {
        let msgs = vec!["test error".to_string()];
        // All recognized categories should produce Some
        for cat in &[
            "import",
            "type",
            "borrow",
            "lifetime",
            "unused",
            "syntax",
            "test_assertion",
        ] {
            let note = build_learn_memory_note("cargo test", &msgs, cat);
            assert!(
                note.is_some(),
                "category '{cat}' should produce a learning note"
            );
        }
        // Unrecognized should produce None
        for cat in &["other", "unknown", ""] {
            let note = build_learn_memory_note("cargo test", &msgs, cat);
            assert!(
                note.is_none(),
                "category '{cat}' should NOT produce a learning note"
            );
        }
    }

    #[test]
    fn test_auto_remember_creates_entry() {
        let path = temp_memory_path("auto_remember_create");
        cleanup(&path);

        let saved = auto_remember_to("project uses postgresql for data", &path);
        assert!(saved, "should save new memory");

        let memory = load_memories_from(&path);
        assert_eq!(memory.entries.len(), 1);
        assert_eq!(memory.entries[0].note, "project uses postgresql for data");

        cleanup(&path);
    }

    #[test]
    fn test_auto_remember_deduplicates() {
        let path = temp_memory_path("auto_remember_dedup");
        cleanup(&path);

        let saved1 = auto_remember_to("Watch fix: 'cargo test' failed, fixed on attempt 1", &path);
        assert!(saved1, "first save should succeed");

        // Same note again — should be detected as duplicate
        let saved2 = auto_remember_to("Watch fix: 'cargo test' failed, fixed on attempt 1", &path);
        assert!(!saved2, "duplicate should not be saved");

        let memory = load_memories_from(&path);
        assert_eq!(memory.entries.len(), 1, "only one entry should exist");

        cleanup(&path);
    }

    #[test]
    fn test_auto_remember_different_notes_both_saved() {
        let path = temp_memory_path("auto_remember_different");
        cleanup(&path);

        let saved1 = auto_remember_to("Watch fix: 'cargo test' failed, fixed on attempt 1", &path);
        assert!(saved1);

        let saved2 = auto_remember_to(
            "Watch fix: 'cargo clippy' failed, fixed on attempt 2",
            &path,
        );
        assert!(saved2);

        let memory = load_memories_from(&path);
        assert_eq!(memory.entries.len(), 2);

        cleanup(&path);
    }

    // #672: the old dedup compared only the first 50 chars, and both note builders put a
    // fixed template + the watch command in front of the discriminating part. With a
    // realistic watch command the whole window is consumed before anything varies, so
    // auto-learning stored at most one memory per (template, watch-command) pair forever.
    const LONG_WATCH_CMD: &str = "cargo clippy --all-targets -- -D warnings && cargo test";

    #[test]
    fn test_auto_remember_long_command_fix_notes_both_saved() {
        let path = temp_memory_path("auto_remember_long_fix");
        cleanup(&path);

        let note1 = build_fix_memory_note(LONG_WATCH_CMD, 1);
        let note2 = build_fix_memory_note(LONG_WATCH_CMD, 5);
        assert_ne!(note1, note2, "fixture must be two distinct notes");

        assert!(auto_remember_to(&note1, &path), "first should save");
        assert!(
            auto_remember_to(&note2, &path),
            "distinct note beyond the old 50-char window should save"
        );

        let memory = load_memories_from(&path);
        assert_eq!(
            memory.entries.len(),
            2,
            "both distinct notes should persist"
        );

        cleanup(&path);
    }

    #[test]
    fn test_auto_remember_long_command_learn_notes_both_saved() {
        let path = temp_memory_path("auto_remember_long_learn");
        cleanup(&path);

        let note1 = build_learn_memory_note(
            LONG_WATCH_CMD,
            &["cannot find type `Foo` in this scope".to_string()],
            "type",
        )
        .expect("learn note should be built");
        let note2 = build_learn_memory_note(
            LONG_WATCH_CMD,
            &["mismatched types: expected `u8`, found `String`".to_string()],
            "type",
        )
        .expect("learn note should be built");
        assert_ne!(note1, note2, "fixture must be two distinct notes");

        assert!(auto_remember_to(&note1, &path), "first should save");
        assert!(
            auto_remember_to(&note2, &path),
            "different error message should save even behind a long watch command"
        );

        let memory = load_memories_from(&path);
        assert_eq!(memory.entries.len(), 2);

        cleanup(&path);
    }

    #[test]
    fn test_auto_remember_deduplicates_long_command_identical_note() {
        let path = temp_memory_path("auto_remember_dedup_long");
        cleanup(&path);

        let note = build_fix_memory_note(LONG_WATCH_CMD, 3);
        assert!(auto_remember_to(&note, &path), "first save should succeed");
        assert!(
            !auto_remember_to(&note, &path),
            "the identical note should still be deduplicated"
        );

        let memory = load_memories_from(&path);
        assert_eq!(memory.entries.len(), 1);

        cleanup(&path);
    }

    #[test]
    fn test_auto_remember_refuses_when_full() {
        let path = temp_memory_path("auto_remember_cap");
        cleanup(&path);

        for i in 0..MAX_AUTO_MEMORIES {
            let note = format!("fact number {i} about this project");
            assert!(auto_remember_to(&note, &path), "entry {i} should save");
        }

        let before = load_memories_from(&path);
        assert_eq!(before.entries.len(), MAX_AUTO_MEMORIES);

        let saved = auto_remember_to("a genuinely distinct fact that arrives too late", &path);
        assert!(
            !saved,
            "auto-remember should refuse once the cap is reached"
        );

        let after = load_memories_from(&path);
        assert_eq!(
            after.entries.len(),
            MAX_AUTO_MEMORIES,
            "the file must not grow past the cap"
        );

        cleanup(&path);
    }

    #[test]
    fn test_category_deserialization_missing_field() {
        // Old-format JSON without category field should deserialize with "general"
        let json = r#"{"entries":[{"note":"old note","timestamp":"2026-01-01 00:00"}]}"#;
        let memory: ProjectMemory = serde_json::from_str(json).unwrap();
        assert_eq!(memory.entries.len(), 1);
        assert_eq!(memory.entries[0].category, "general");
    }

    #[test]
    fn test_category_deserialization_with_field() {
        let json = r#"{"entries":[{"note":"build note","timestamp":"2026-01-01 00:00","category":"build"}]}"#;
        let memory: ProjectMemory = serde_json::from_str(json).unwrap();
        assert_eq!(memory.entries[0].category, "build");
    }

    #[test]
    fn test_add_memory_with_category() {
        let mut memory = ProjectMemory::default();
        add_memory_with_category(&mut memory, "use cargo fmt", "build");
        assert_eq!(memory.entries.len(), 1);
        assert_eq!(memory.entries[0].note, "use cargo fmt");
        assert_eq!(memory.entries[0].category, "build");
    }

    #[test]
    fn test_add_memory_with_invalid_category_falls_back() {
        let mut memory = ProjectMemory::default();
        add_memory_with_category(&mut memory, "some note", "invalid_cat");
        assert_eq!(memory.entries[0].category, "general");
    }

    #[test]
    fn test_add_memory_default_category() {
        let mut memory = ProjectMemory::default();
        add_memory(&mut memory, "plain note");
        assert_eq!(memory.entries[0].category, "general");
    }

    #[test]
    fn test_is_valid_category() {
        assert!(is_valid_category("general"));
        assert!(is_valid_category("build"));
        assert!(is_valid_category("convention"));
        assert!(is_valid_category("architecture"));
        assert!(is_valid_category("bug"));
        assert!(!is_valid_category("random"));
        assert!(!is_valid_category(""));
    }

    #[test]
    fn test_format_memories_grouped_by_category() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "use cargo fmt".to_string(),
                    timestamp: "2026-01-01 00:00".to_string(),
                    category: "build".to_string(),
                },
                MemoryEntry {
                    note: "snake_case only".to_string(),
                    timestamp: "2026-01-02 00:00".to_string(),
                    category: "convention".to_string(),
                },
                MemoryEntry {
                    note: "tests need docker".to_string(),
                    timestamp: "2026-01-03 00:00".to_string(),
                    category: "build".to_string(),
                },
            ],
        };
        let prompt = format_memories_for_prompt(&memory).unwrap();
        assert!(prompt.contains("### Build"), "should have Build header");
        assert!(
            prompt.contains("### Convention"),
            "should have Convention header"
        );
        assert!(prompt.contains("use cargo fmt"));
        assert!(prompt.contains("snake_case only"));
        assert!(prompt.contains("tests need docker"));
        // Build entries should be grouped together
        let build_pos = prompt.find("### Build").unwrap();
        let convention_pos = prompt.find("### Convention").unwrap();
        let fmt_pos = prompt.find("use cargo fmt").unwrap();
        let docker_pos = prompt.find("tests need docker").unwrap();
        // Both build entries should appear after the Build header and before Convention
        assert!(fmt_pos > build_pos);
        assert!(docker_pos > build_pos);
        assert!(fmt_pos < convention_pos);
        assert!(docker_pos < convention_pos);
    }

    #[test]
    fn test_format_memories_flat_when_few() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "note one".to_string(),
                    timestamp: "t0".to_string(),
                    category: "build".to_string(),
                },
                MemoryEntry {
                    note: "note two".to_string(),
                    timestamp: "t1".to_string(),
                    category: "convention".to_string(),
                },
            ],
        };
        let prompt = format_memories_for_prompt(&memory).unwrap();
        // Should NOT have category headers when < 3 entries
        assert!(!prompt.contains("### Build"));
        assert!(!prompt.contains("### Convention"));
        assert!(prompt.contains("note one"));
        assert!(prompt.contains("note two"));
    }

    #[test]
    fn test_category_roundtrip_through_file() {
        let path = temp_memory_path("category_roundtrip");
        cleanup(&path);

        let mut memory = ProjectMemory::default();
        add_memory_with_category(&mut memory, "lint command", "build");
        add_memory_with_category(&mut memory, "naming rule", "convention");
        add_memory(&mut memory, "plain note");

        save_memories_to(&memory, &path).unwrap();
        let loaded = load_memories_from(&path);

        assert_eq!(loaded.entries[0].category, "build");
        assert_eq!(loaded.entries[1].category, "convention");
        assert_eq!(loaded.entries[2].category, "general");

        cleanup(&path);
    }
}
