//! Harness-only validation. Uses the runtime reducer without changing yoyo's CLI.
use std::{collections::HashSet, error::Error, fs, path::PathBuf};
use yoagent_state::{project_event, replay_with_diagnostics, Event};

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("missing repository path")?,
    );
    let raw = fs::read_to_string(root.join("state/events.jsonl"))?;
    let mut ids = HashSet::new();
    let mut events = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let event: Event = serde_json::from_str(line)?;
        if !ids.insert(event.id.clone()) {
            return Err("duplicate GASP event ID".into());
        }
        events.push(event);
    }
    // Preserve the runtime's tolerance of already-published legacy errors,
    // while refusing to introduce new dangling operations during delivery.
    let baseline = match std::env::args_os().nth(2) {
        Some(path) => fs::read_to_string(path)?,
        None => String::new(),
    };
    if !raw.starts_with(&baseline) {
        return Err("published event prefix changed".into());
    }
    let count = baseline
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let (mut graph, skipped) = replay_with_diagnostics(&events[..count])?;
    for event in &events[count..] {
        project_event(&mut graph, event)?;
    }
    println!(
        "{}",
        serde_json::json!({"reducer":yoagent_state::VERSION,
        "events":events.len(), "diagnostics":skipped.len()})
    );
    Ok(())
}
