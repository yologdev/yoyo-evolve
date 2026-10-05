//! #993: what a Ctrl+C (SIGINT) mid-turn prints, and whether it happened.

use crate::format::{DIM, RESET};

/// #993: set by `main.rs` for the non-interactive doors (`-p`, piped stdin).
static SINGLE_SHOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// #993: set when a Ctrl+C (SIGINT) cut a turn short.
pub(super) static INTERRUPTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Mark this process as a single-shot run (`-p` / piped prompt).
pub fn mark_single_shot() {
    SINGLE_SHOT.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub(super) fn is_single_shot() -> bool {
    SINGLE_SHOT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Whether a SIGINT interrupted a turn in this process (#993: `-p` exits 130).
pub fn was_interrupted() -> bool {
    INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst)
}

/// #993: the note printed when Ctrl+C interrupts a turn. The REPL row is the
/// historical hint, byte for byte; a single-shot run has already exited, so
/// it gets a plain note (the caller writes it to stderr).
pub(super) fn interrupt_note(single_shot: bool) -> String {
    if single_shot {
        format!("\n{DIM}  (interrupted){RESET}\n")
    } else {
        format!("\n{DIM}  (interrupted — press Ctrl+C again to exit){RESET}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupt_note_repl_row_is_the_old_hint_and_single_shot_drops_again() {
        assert_eq!(
            interrupt_note(false),
            format!("\n{DIM}  (interrupted — press Ctrl+C again to exit){RESET}\n")
        );
        let single = interrupt_note(true);
        assert_eq!(single, format!("\n{DIM}  (interrupted){RESET}\n"));
        assert!(!single.contains("again"));
    }
}
