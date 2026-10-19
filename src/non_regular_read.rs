//! `read_file` refuses FIFOs, devices and sockets instead of hanging (#1047).
//!
//! A FIFO, a tty or a character device reports `metadata.len() == 0`, so a
//! size check passes and `read_to_string` runs: on a FIFO or tty it blocks
//! forever, on `/dev/zero` it grows without bound. yoagent 0.25.8 now refuses
//! these upstream (`require_regular_file` in `tools/file.rs`, called by
//! `read_file`, `write_file` and `edit_file`); before 0.25.8 its `ReadFileTool`
//! trusted `metadata.len()` and hung. So for the model's tools yoyo's wrapper
//! below is now a **second** check, kept so the refusal does not depend on the
//! dependency's version. It is unconditional — it runs with or without
//! `--allow-dir`/`--deny-dir`, because `GuardedTool`/`ArcGuardedTool` wrap every
//! `read_file` unconditionally.
//!
//! For yoyo's **own** `std::fs` readers (startup config, instruction files,
//! goal/memory, the trusted-dirs store, `/add`, `/config set`) yoagent offers
//! nothing: `read_regular_to_string` and its siblings below are the only guard.
//! <!-- yoagent-version-claim: 0.25 -->
//!
//! What it does NOT touch, so those paths stay byte-identical:
//! - **Missing paths / metadata errors** → `None`; the inner tool reports its
//!   own `Cannot access …` error rather than a second invented message.
//! - **Directories** → `None`; yoagent already fails fast and clearly
//!   (`Cannot read <dir>: Is a directory (os error 21)`), so nothing hangs.
//! - **Symlinks to regular files** → `None`; `std::fs::metadata` follows links.
//!
//! The path is checked as given, which is the path yoagent opens: with no
//! sandbox roots configured, `PathSandbox::io_path` returns it unchanged and it
//! resolves against the process cwd, exactly as `std::fs::metadata` does.

use std::path::Path;

/// The refusal for a non-regular, non-directory file, or `None` to let the
/// real tool run. Pure apart from one `metadata` call; never opens the file.
pub(crate) fn non_regular_file_refusal(path: &Path) -> Option<String> {
    non_regular_reason(path).map(|r| format!("{r}; read_file only reads regular files"))
}

/// The tool-neutral half of the refusal: why `path` must not be opened, or
/// `None` for a regular file, a directory, or a path `metadata` cannot see.
/// `metadata` follows symlinks and never opens the file, so it cannot block.
fn non_regular_reason(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let ft = meta.file_type();
    if ft.is_file() || ft.is_dir() {
        return None;
    }
    Some(format!(
        "{} is not a regular file ({}): reading it could block forever or never end",
        path.display(),
        kind_name(&ft)
    ))
}

/// `std::fs::read_to_string` for readers that load files on their own (the
/// startup instruction loader, `/add`): a FIFO, device or socket is refused
/// with an `InvalidInput` error carrying the reason, instead of being opened.
/// A symlink to a regular file still reads. #1047.
pub(crate) fn read_regular_to_string(path: &Path) -> std::io::Result<String> {
    refuse_non_regular(path)?;
    std::fs::read_to_string(path)
}

/// The config loaders' reader (#1047): `Some(text)` for a readable regular
/// file; `None` when absent or unreadable, as before; and `None` plus ONE
/// stderr warning for a FIFO/device/socket, so `load_config_file` skips it and
/// walks on to `~/.yoyo.toml`/XDG instead of blocking forever. Nobody writes a
/// FIFO as their config, so this branch is reached only by a broken or hostile
/// repo: refusing to start would let any clone stop yoyo, and a silent skip
/// would hide that the project config was not applied.
pub(crate) fn read_config_or_warn(path: &Path) -> Option<String> {
    match read_regular_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => {
            let reason = crate::cli::sanitize_for_display(&e.to_string());
            eprintln!("warning: config file skipped - {reason}");
            None
        }
        Err(_) => None,
    }
}

/// The reader for every read-then-rewrite writer (`/config set`, the
/// allow-persist writer, `/todo board evidence`): the file's text, `""` only
/// when it does not exist, and `Err` naming the file for anything else — a
/// FIFO or device (#1047: the read would block, and the write after it too),
/// or a file that exists but cannot be read, e.g. invalid UTF-8 (#1053).
///
/// Superseded claim, recorded rather than erased (#1053): until Day 233 every
/// error other than a non-regular file became `""` (the old
/// `unwrap_or_default`). The caller then writes `existing + new`, so an
/// unreadable file was replaced by just the new line, under a green success.
pub(crate) fn read_for_rewrite(path: &Path) -> Result<String, String> {
    match read_regular_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => Err(e.to_string()),
        Err(e) => Err(crate::cli::sanitize_for_display(&format!(
            "could not read {} ({e}); it was left untouched, nothing written",
            path.display()
        ))),
    }
}

/// The bytes twin of [`read_regular_to_string`] (used by `/add` for images).
pub(crate) fn read_regular(path: &Path) -> std::io::Result<Vec<u8>> {
    refuse_non_regular(path)?;
    std::fs::read(path)
}

fn refuse_non_regular(path: &Path) -> std::io::Result<()> {
    match non_regular_reason(path) {
        None => Ok(()),
        Some(r) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, r)),
    }
}

#[cfg(unix)]
fn kind_name(ft: &std::fs::FileType) -> &'static str {
    use std::os::unix::fs::FileTypeExt;
    if ft.is_fifo() {
        "FIFO"
    } else if ft.is_char_device() {
        "character device"
    } else if ft.is_block_device() {
        "block device"
    } else if ft.is_socket() {
        "socket"
    } else {
        "special file"
    }
}

#[cfg(not(unix))]
fn kind_name(_ft: &std::fs::FileType) -> &'static str {
    "special file"
}

/// The wiring seam for the path-argument guards: only `read_file` is checked.
pub(crate) fn check(tool: &str, path: &str) -> Option<String> {
    if tool != "read_file" {
        return None;
    }
    non_regular_file_refusal(Path::new(path))
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use std::os::unix::fs::FileTypeExt;

    /// Make a FIFO in `dir`, or `None` if `mkfifo` is unavailable.
    pub(crate) fn make_fifo(dir: &Path) -> Option<std::path::PathBuf> {
        let p = dir.join("pipe.txt");
        let ok = std::process::Command::new("mkfifo")
            .arg(&p)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        ok.then_some(p)
    }

    /// `/config set` and the allow-persist writer refuse a FIFO `.yoyo.toml`
    /// with an error naming it, and return instead of blocking (the read, and
    /// then the write, would each wait forever for a peer).
    #[test]
    fn config_writers_refuse_a_fifo_and_name_it() {
        let dir = tempfile::tempdir().unwrap();
        let Some(fifo) = make_fifo(dir.path()) else {
            eprintln!("mkfifo unavailable; skipping");
            return;
        };
        assert!(std::fs::metadata(&fifo).unwrap().file_type().is_fifo());
        // Run on a thread so a regression fails by name instead of hanging the suite.
        let (tx, rx) = std::sync::mpsc::channel();
        let f = fifo.clone();
        std::thread::spawn(move || {
            let set = crate::config::write_config_value_to("model", "x", &f);
            let allow = crate::config::append_allow_pattern_to("ls *", &f);
            let _ = tx.send((set, allow));
        });
        let (set, allow) = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("config writers blocked on a FIFO .yoyo.toml (#1047)");
        let set = set.unwrap_err();
        assert!(set.contains("pipe.txt") && set.contains("FIFO"), "{set}");
        let allow = allow.unwrap_err();
        assert!(
            allow.contains("pipe.txt") && allow.contains("FIFO"),
            "{allow}"
        );
        // Still a FIFO: nothing replaced it with a regular file.
        assert!(std::fs::metadata(&fifo).unwrap().file_type().is_fifo());
    }

    /// Near-miss: a missing file still starts empty, and a regular one is read.
    #[test]
    fn read_for_rewrite_keeps_missing_and_regular_behaviour() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("absent.toml");
        assert_eq!(read_for_rewrite(&missing), Ok(String::new()));
        let regular = dir.path().join("real.toml");
        std::fs::write(&regular, "model = \"m\"\n").unwrap();
        assert_eq!(
            read_for_rewrite(&regular),
            Ok("model = \"m\"\n".to_string())
        );
    }

    /// #1053: a file that exists but cannot be read (invalid UTF-8 is the
    /// realistic case) is an `Err` naming it, never `""` — the caller writes
    /// `existing + new` next, which would replace every byte of it.
    #[test]
    fn read_for_rewrite_refuses_an_unreadable_file_and_names_it() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("binary.toml");
        std::fs::write(&bad, [0xff, 0xfe, b'a']).unwrap();
        // Anti-vacuous: the fixture really is unreadable as text.
        assert!(std::fs::read_to_string(&bad).is_err());
        let err = read_for_rewrite(&bad).expect_err("unreadable file must not read as empty");
        assert!(err.contains("binary.toml"), "{err}");
        assert!(err.contains("nothing written"), "{err}");
    }

    /// #1053 at the `/config set` writers: a non-UTF-8 `.yoyo.toml` survives
    /// both writers byte-for-byte, and each reports an error naming it.
    #[test]
    fn config_writers_leave_a_non_utf8_config_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join(".yoyo.toml");
        let bytes: &[u8] = b"model = \"m\"\n# caf\xe9\nprovider = \"p\"\n";
        std::fs::write(&cfg, bytes).unwrap();
        let set = crate::config::write_config_value_to("thinking", "high", &cfg).unwrap_err();
        assert!(set.contains(".yoyo.toml"), "{set}");
        let allow = crate::config::append_allow_pattern_to("ls *", &cfg).unwrap_err();
        assert!(allow.contains(".yoyo.toml"), "{allow}");
        assert_eq!(std::fs::read(&cfg).unwrap(), bytes);
    }

    #[test]
    fn fifo_is_refused_and_named() {
        let dir = tempfile::tempdir().unwrap();
        let Some(fifo) = make_fifo(dir.path()) else {
            eprintln!("mkfifo unavailable; skipping");
            return;
        };
        // Anti-vacuous: the fixture really is a FIFO.
        assert!(std::fs::metadata(&fifo).unwrap().file_type().is_fifo());
        let msg = non_regular_file_refusal(&fifo).expect("FIFO must be refused");
        assert!(msg.contains("FIFO"), "{msg}");
        assert!(msg.contains("read_file only reads regular files"), "{msg}");
    }

    #[test]
    fn char_device_is_refused() {
        let p = Path::new("/dev/null");
        assert!(std::fs::metadata(p).unwrap().file_type().is_char_device());
        let msg = non_regular_file_refusal(p).expect("/dev/null must be refused");
        assert!(msg.contains("device"), "{msg}");
    }

    #[test]
    fn regular_file_passes() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.txt");
        std::fs::write(&f, "hi\n").unwrap();
        assert_eq!(non_regular_file_refusal(&f), None);
    }

    #[test]
    fn symlink_to_regular_file_passes() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.txt");
        std::fs::write(&f, "hi\n").unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&f, &link).unwrap();
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(non_regular_file_refusal(&link), None);
    }

    #[test]
    fn missing_path_and_directory_pass_through() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(non_regular_file_refusal(&dir.path().join("nope")), None);
        assert_eq!(non_regular_file_refusal(dir.path()), None);
    }

    /// Emission point: the wrapped `read_file` exactly as `tools.rs` builds it
    /// (`maybe_guard`/`maybe_guard_arc` over `ReadFileTool::default()`, NO fence
    /// configured). A hang is the bug, and `tokio::time::timeout` cannot catch
    /// it: the stuck read lives in `spawn_blocking`, which a `#[tokio::test]`
    /// runtime waits for at shutdown. So each call runs on a detached thread
    /// with its own runtime and the test waits with `recv_timeout`; a regression
    /// panics by name and the leaked thread dies with the test process.
    fn run_wrapped_read(arc: bool, path: std::path::PathBuf) -> Result<String, String> {
        use std::sync::Arc;
        use yoagent::tools::file::ReadFileTool;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let none = crate::cli::DirectoryRestrictions::default();
            let params = serde_json::json!({ "path": path.to_str().unwrap() });
            let ctx = yoagent::types::ToolContext::new("c1", "read_file");
            let out = rt.block_on(async {
                if arc {
                    crate::tool_wrappers::maybe_guard_arc(Arc::new(ReadFileTool::default()), &none)
                        .execute(params, ctx)
                        .await
                } else {
                    crate::tool_wrappers::maybe_guard(Box::new(ReadFileTool::default()), &none)
                        .execute(params, ctx)
                        .await
                }
            });
            let mapped = match out {
                Ok(r) => Ok(r
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        yoagent::types::Content::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<String>()),
                Err(yoagent::types::ToolError::Failed(m)) => Err(m),
                Err(e) => Err(format!("other error: {e:?}")),
            };
            let _ = tx.send(mapped);
        });
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("read_file (arc={arc}) hung: #1047 regressed"))
    }

    #[test]
    fn wrapped_read_file_refuses_fifo_without_hanging_both_wrappers() {
        let dir = tempfile::tempdir().unwrap();
        let Some(fifo) = make_fifo(dir.path()) else {
            eprintln!("mkfifo unavailable; skipping");
            return;
        };
        for arc in [false, true] {
            match run_wrapped_read(arc, fifo.clone()) {
                Err(m) => assert!(m.contains("FIFO"), "arc={arc}: {m}"),
                Ok(t) => panic!("arc={arc}: expected a FIFO refusal, got content {t:?}"),
            }
        }
    }

    /// Near-miss at the same seam: a regular file still reads, content intact.
    #[test]
    fn wrapped_read_file_still_reads_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.txt");
        std::fs::write(&f, "hello fifo-free world\n").unwrap();
        for arc in [false, true] {
            let text = run_wrapped_read(arc, f.clone()).expect("regular file must read");
            assert!(text.contains("hello fifo-free world"), "arc={arc}: {text}");
        }
    }

    #[test]
    fn check_only_applies_to_read_file() {
        assert!(check("read_file", "/dev/null").is_some());
        assert_eq!(check("list_files", "/dev/null"), None);
        assert_eq!(check("write_file", "/dev/null"), None);
    }

    /// Run `f` on a thread and fail by name if it blocks: a FIFO with no
    /// writer would otherwise hang the test binary instead of failing it.
    fn within_10s<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("{what} hung on a FIFO: #1047 regressed"))
    }

    #[test]
    fn read_regular_refuses_fifo_promptly_with_reason() {
        let dir = tempfile::tempdir().unwrap();
        let Some(fifo) = make_fifo(dir.path()) else {
            eprintln!("mkfifo unavailable; skipping");
            return;
        };
        let f2 = fifo.clone();
        let text = within_10s("read_regular_to_string", move || {
            read_regular_to_string(&f2)
        });
        let e = text.expect_err("FIFO must be refused");
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
        assert!(e.to_string().contains("FIFO"), "{e}");
        let bytes = within_10s("read_regular", move || read_regular(&fifo));
        assert!(bytes
            .unwrap_err()
            .to_string()
            .contains("not a regular file"));
    }

    #[test]
    fn read_regular_reads_regular_file_and_symlink_byte_identically() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.txt");
        let body = "line one\n\u{2713} two\n";
        std::fs::write(&f, body).unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&f, &link).unwrap();
        for p in [&f, &link] {
            assert_eq!(read_regular_to_string(p).unwrap(), body);
            assert_eq!(read_regular(p).unwrap(), body.as_bytes());
        }
        // A missing path keeps std's own error, not the refusal.
        let e = read_regular_to_string(&dir.path().join("nope")).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
    }

    /// The startup loader: a FIFO named CLAUDE.md must not hang turn zero,
    /// and a sibling regular AGENTS.md still loads (anti-vacuous).
    #[test]
    fn instruction_loader_skips_fifo_claude_md_and_loads_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        let Some(fifo) = make_fifo(dir.path()) else {
            eprintln!("mkfifo unavailable; skipping");
            return;
        };
        std::fs::rename(&fifo, dir.path().join("CLAUDE.md")).unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "agents-canary-1047\n").unwrap();
        let root = dir.path().to_path_buf();
        let ctx = within_10s("load_project_context_from", move || {
            crate::context::load_project_context_from(&root)
        })
        .expect("AGENTS.md must still load");
        assert!(ctx.contains("agents-canary-1047"), "{ctx}");
        assert!(!ctx.contains("BEGIN CLAUDE.md"), "{ctx}");
        let root = dir.path().to_path_buf();
        let (listed, _) = within_10s("list_project_context_files_fenced", move || {
            let none = crate::config::DirectoryRestrictions::default();
            // Two distinct handles: one Mutex passed as both would deadlock.
            let (deny, allow) = (Default::default(), Default::default());
            crate::context::list_project_context_files_fenced(&root, &none, &deny, &allow)
        });
        let names: Vec<&str> = listed.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["AGENTS.md"]);
    }

    /// `/add`: text and image readers refuse a FIFO and admit a regular file.
    #[test]
    fn add_refuses_fifo_and_admits_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let Some(fifo) = make_fifo(dir.path()) else {
            eprintln!("mkfifo unavailable; skipping");
            return;
        };
        let p = fifo.to_string_lossy().into_owned();
        let (r1, r2) = within_10s("/add", move || {
            let none = crate::config::DirectoryRestrictions::default();
            (
                crate::commands_file::read_file_for_add(&p, None, &none),
                crate::commands_file::read_image_for_add(&p, &none),
            )
        });
        let m1 = r1.expect_err("text /add must refuse a FIFO").to_string();
        let m2 = r2.expect_err("image /add must refuse a FIFO").to_string();
        assert!(m1.contains("not a regular file (FIFO)"), "{m1}");
        assert!(m2.contains("not a regular file (FIFO)"), "{m2}");
        let f = dir.path().join("ok.txt");
        std::fs::write(&f, "a\nb\n").unwrap();
        let none = crate::config::DirectoryRestrictions::default();
        let (text, lines) =
            crate::commands_file::read_file_for_add(&f.to_string_lossy(), None, &none).unwrap();
        assert_eq!((text.as_str(), lines), ("a\nb\n", 2));
    }
}
