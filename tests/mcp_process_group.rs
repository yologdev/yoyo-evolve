//! #1098 at the real spawn: every stdio MCP server yoyo starts runs in a
//! process group of its own, so the REPL's "interrupt this turn" Ctrl+C
//! (SIGINT to the terminal's foreground group, which is yoyo's group) no
//! longer reaches it and kills it silently.
//!
//! The test runs the built binary with `--mcp "python3 <server>"` and `-p`
//! against a closed local port (no provider is ever reached; the MCP connect
//! happens before the first request). The server appends `<pid> <pgid>` to a
//! file at startup, so BOTH spawns are observed: the pre-flight tool listing
//! (`agent_builder::fetch_mcp_tool_names`) and the real connect
//! (`mcp_prefix::mcp_tool_source`). yoyo itself is spawned with
//! `process_group(0)`, so its group id is its own pid — the group a terminal's
//! Ctrl+C would signal — and every recorded server pgid must differ from it.
//!
//! Without python3 the test prints SKIPPED and returns without asserting.
//! Provider keys are scrubbed from the child the way `tests/config_fifo.rs`
//! does it (every `API_KEY` / `*_API_KEY`), then a dummy anthropic key is set
//! so the run gets as far as connecting. Never invokes cargo.
#![cfg(unix)]

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SERVER: &str = r#"
import json, os, sys
with open(sys.argv[1], "a") as f:
    f.write("%d %d\n" % (os.getpid(), os.getpgrp()))
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except Exception:
        continue
    if "id" not in msg:
        continue
    method = msg.get("method")
    if method == "initialize":
        result = {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}},
                  "serverInfo": {"name": "pgprobe", "version": "0"}}
    elif method == "tools/list":
        result = {"tools": [{"name": "pg_probe_tool", "description": "probe",
                             "inputSchema": {"type": "object", "properties": {}}}]}
    else:
        result = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}) + "\n")
    sys.stdout.flush()
"#;

fn python3_available() -> bool {
    Command::new("python3")
        .arg("-c")
        .arg("pass")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// A local port nothing listens on: bind an ephemeral one, then drop it.
fn closed_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    l.local_addr().unwrap().port()
}

fn read_records(path: &Path) -> Vec<(i64, i64)> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect()
}

#[test]
fn stdio_mcp_servers_run_outside_yoyos_process_group() {
    if !python3_available() {
        eprintln!("SKIPPED stdio_mcp_servers_run_outside_yoyos_process_group: python3 not found");
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let script = tmp.path().join("pgprobe.py");
    std::fs::write(&script, SERVER).unwrap();
    let record = tmp.path().join("pgids.txt");
    // `--mcp` splits on whitespace, so the tempdir path must not contain any.
    assert!(
        !script.display().to_string().contains(char::is_whitespace)
            && !record.display().to_string().contains(char::is_whitespace),
        "tempdir path has whitespace; --mcp would split it: {}",
        script.display()
    );
    let mcp_cmd = format!("python3 {} {}", script.display(), record.display());
    let base_url = format!("http://127.0.0.1:{}/v1", closed_port());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    for (name, _) in std::env::vars() {
        if name == "API_KEY" || name.ends_with("_API_KEY") {
            cmd.env_remove(name);
        }
    }
    let mut child = cmd
        .args([
            "--provider",
            "anthropic",
            "--model",
            "claude-sonnet-4-5",
            "--base-url",
            &base_url,
            "--mcp",
            &mcp_cmd,
            "--max-turns",
            "1",
            "-p",
            "hi",
        ])
        .current_dir(&proj)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("ANTHROPIC_API_KEY", "sk-ant-dummy-test-key")
        .env_remove("MODEL")
        .env_remove("YOYO_MODEL")
        .env_remove("YOYO_RESTRICTED")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .expect("spawn yoyo");
    // process_group(0): yoyo's group id is its own pid.
    let yoyo_group = child.id() as i64;

    // Wait for both spawns (pre-flight + real connect) to record, or for
    // yoyo to exit on its own.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if read_records(&record).len() >= 2 {
            break;
        }
        if child.try_wait().unwrap().is_some() || Instant::now() > deadline {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let out = child.wait_with_output().expect("collect output");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let records = read_records(&record);

    // Anti-vacuous: both spawns really happened.
    assert!(
        records.len() >= 2,
        "expected the pre-flight and the real connect to each start the \
         server (2 records), got {records:?}; yoyo stderr:\n{stderr}"
    );
    for (pid, pgid) in &records {
        assert_ne!(
            *pgid, yoyo_group,
            "MCP server pid {pid} shares yoyo's process group {yoyo_group}, so a \
             terminal Ctrl+C would kill it (#1098); records {records:?}"
        );
        assert_eq!(
            pid, pgid,
            "MCP server pid {pid} should lead a process group of its own \
             (pgid {pgid}); records {records:?}"
        );
    }
}
