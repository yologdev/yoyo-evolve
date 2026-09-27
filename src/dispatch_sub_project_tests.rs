//! #962: `yoyo setup` / `yoyo init` dispatch tests, routed into a tempdir.
//! The old tests ran the real wizard / init handler against the process cwd
//! (the repo root) and clobbered `.yoyo.toml` in the evolve job.
use crate::dispatch_sub::{try_dispatch_subcommand_in, ProjectIo};

fn repo_file(name: &str) -> Option<Vec<u8>> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name)).ok()
}

#[test]
fn test_try_dispatch_subcommand_setup_bare() {
    let before = repo_file(".yoyo.toml");
    let tmp = tempfile::TempDir::new().expect("tempdir");
    // ollama (4), default model, save to project (1) -- no key prompt, no env read.
    let mut reader = std::io::Cursor::new(b"4\n\n1\n".to_vec());
    let args: Vec<String> = vec!["yoyo".into(), "setup".into()];
    let io = ProjectIo {
        dir: tmp.path(),
        input: &mut reader,
    };
    let result = try_dispatch_subcommand_in(&args, Some(io));
    assert!(matches!(result, Some(None)), "`setup` must route");
    let written = std::fs::read_to_string(tmp.path().join(".yoyo.toml"))
        .expect("wizard must write into the tempdir, proving it routed there");
    assert!(written.contains("ollama"), "wrote: {written}");
    assert_eq!(before, repo_file(".yoyo.toml"), "repo .yoyo.toml changed");
}

#[test]
fn test_try_dispatch_subcommand_init_bare() {
    let yoyo_md_before = repo_file("YOYO.md");
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let mut reader = std::io::Cursor::new(Vec::new());
    let args: Vec<String> = vec!["yoyo".into(), "init".into()];
    let io = ProjectIo {
        dir: tmp.path(),
        input: &mut reader,
    };
    let result = try_dispatch_subcommand_in(&args, Some(io));
    assert!(matches!(result, Some(None)), "`init` must route");
    assert!(
        tmp.path().join("YOYO.md").exists(),
        "init must write YOYO.md into the tempdir"
    );
    assert_eq!(yoyo_md_before, None, "fixture: repo root has no YOYO.md");
    assert_eq!(
        repo_file("YOYO.md"),
        None,
        "init wrote YOYO.md into repo root"
    );
}
