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

/// Write `server` to a tempdir and start `yoyo -p` against it with a dead
/// provider port, keys scrubbed and a private HOME. The one spawn helper for
/// this file (d233: every test reuses its scrubbing, none writes its own).
/// Returns (tempdir, record path, child).
fn spawn_yoyo_with_server(
    server: &str,
    base_url: &str,
) -> (tempfile::TempDir, std::path::PathBuf, std::process::Child) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let script = tmp.path().join("pgprobe.py");
    std::fs::write(&script, server).unwrap();
    let record = tmp.path().join("pgids.txt");
    // `--mcp` splits on whitespace, so the tempdir path must not contain any.
    assert!(
        !script.display().to_string().contains(char::is_whitespace)
            && !record.display().to_string().contains(char::is_whitespace),
        "tempdir path has whitespace; --mcp would split it: {}",
        script.display()
    );
    let mcp_cmd = format!("python3 {} {}", script.display(), record.display());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    for (name, _) in std::env::vars() {
        if name == "API_KEY" || name.ends_with("_API_KEY") {
            cmd.env_remove(name);
        }
    }
    let child = cmd
        .args([
            "--provider",
            "anthropic",
            "--model",
            "claude-sonnet-4-5",
            "--base-url",
            base_url,
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
    (tmp, record, child)
}

#[test]
fn stdio_mcp_servers_run_outside_yoyos_process_group() {
    if !python3_available() {
        eprintln!("SKIPPED stdio_mcp_servers_run_outside_yoyos_process_group: python3 not found");
        return;
    }
    let (_tmp, record, mut child) =
        spawn_yoyo_with_server(SERVER, &format!("http://127.0.0.1:{}/v1", closed_port()));
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

/// A provider that answers every request 401 at once, so a `-p` turn fails
/// without the retry backoff a closed port costs. Returns its base URL.
fn refusing_provider() -> String {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().unwrap().port();
    thread::spawn(move || {
        for mut conn in l.incoming().flatten() {
            let mut buf = [0u8; 65536];
            let _ = conn.set_read_timeout(Some(Duration::from_millis(500)));
            let _ = conn.read(&mut buf);
            let body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
            let _ = write!(
                conn,
                "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("http://127.0.0.1:{port}/v1")
}

/// True while `pid` names a process (`kill -0`).
fn pid_alive(pid: i64) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// #1099 at the real binary: a server that IGNORES stdin EOF (keeps running
/// after its input closes) is gone shortly after `yoyo -p` exits on its own.
/// Since #1098 it sits in its own process group, so only yoyo closing it
/// (`McpClient::close`, a group kill) can end it.
#[test]
fn a_server_that_ignores_stdin_eof_is_closed_when_yoyo_exits() {
    if !python3_available() {
        eprintln!(
            "SKIPPED a_server_that_ignores_stdin_eof_is_closed_when_yoyo_exits: python3 not found"
        );
        return;
    }
    let stubborn = format!("{SERVER}\nimport time\nwhile True:\n    time.sleep(1)\n");
    let (_tmp, record, mut child) = spawn_yoyo_with_server(&stubborn, &refusing_provider());

    // Let yoyo finish by itself: the provider answers 401 (not retried), so
    // the turn fails and `-p` exits through its normal error door.
    let deadline = Instant::now() + Duration::from_secs(60);
    let exited = loop {
        if child.try_wait().unwrap().is_some() {
            break true;
        }
        if Instant::now() > deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(50));
    };
    if !exited {
        let _ = child.kill();
    }
    let out = child.wait_with_output().expect("collect output");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(exited, "yoyo -p did not exit within 60s; stderr:\n{stderr}");

    let pids: Vec<i64> = read_records(&record).into_iter().map(|(p, _)| p).collect();
    // Anti-vacuous: the real connect (not only the pre-flight) started it.
    assert!(
        pids.len() >= 2,
        "expected the pre-flight and the real connect to start the server, \
         got {pids:?}; yoyo stderr:\n{stderr}"
    );
    let gone_by = Instant::now() + Duration::from_secs(3);
    let mut alive: Vec<i64> = pids.iter().copied().filter(|p| pid_alive(*p)).collect();
    while !alive.is_empty() && Instant::now() < gone_by {
        thread::sleep(Duration::from_millis(50));
        alive.retain(|p| pid_alive(*p));
    }
    // Never leak a stub, whatever the verdict.
    for pid in &alive {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
    assert!(
        alive.is_empty(),
        "MCP server pid(s) {alive:?} outlived yoyo -p by 3s (#1099): nothing \
         closed them on exit; exit {:?}; yoyo stderr:\n{stderr}",
        out.status
    );
}

/// A provider that accepts and never answers, so a `-p` turn is mid-request
/// when the test signals yoyo. Returns its base URL.
fn hanging_provider() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().unwrap().port();
    thread::spawn(move || {
        let mut held = Vec::new();
        for conn in l.incoming().flatten() {
            held.push(conn);
        }
    });
    format!("http://127.0.0.1:{port}/v1")
}

/// #1099, the case measured on the ticket: a SIGTERM'd `yoyo -p` (mid-turn)
/// still closes a server that ignores stdin EOF on its way out.
#[test]
fn a_sigtermed_yoyo_still_closes_a_server_that_ignores_stdin_eof() {
    if !python3_available() {
        eprintln!(
            "SKIPPED a_sigtermed_yoyo_still_closes_a_server_that_ignores_stdin_eof: python3 not found"
        );
        return;
    }
    let stubborn = format!("{SERVER}\nimport time\nwhile True:\n    time.sleep(1)\n");
    let (_tmp, record, mut child) = spawn_yoyo_with_server(&stubborn, &hanging_provider());
    let deadline = Instant::now() + Duration::from_secs(30);
    while read_records(&record).len() < 2 && Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    // Give the turn a moment to reach the (hanging) request.
    thread::sleep(Duration::from_millis(500));
    let _ = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status();
    let deadline = Instant::now() + Duration::from_secs(20);
    let exited = loop {
        if child.try_wait().unwrap().is_some() {
            break true;
        }
        if Instant::now() > deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(50));
    };
    if !exited {
        let _ = child.kill();
    }
    let out = child.wait_with_output().expect("collect output");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(exited, "yoyo -p ignored SIGTERM for 20s; stderr:\n{stderr}");

    let pids: Vec<i64> = read_records(&record).into_iter().map(|(p, _)| p).collect();
    assert!(
        pids.len() >= 2,
        "expected both spawns, got {pids:?}; yoyo stderr:\n{stderr}"
    );
    let gone_by = Instant::now() + Duration::from_secs(3);
    let mut alive: Vec<i64> = pids.iter().copied().filter(|p| pid_alive(*p)).collect();
    while !alive.is_empty() && Instant::now() < gone_by {
        thread::sleep(Duration::from_millis(50));
        alive.retain(|p| pid_alive(*p));
    }
    for pid in &alive {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
    assert!(
        alive.is_empty(),
        "MCP server pid(s) {alive:?} outlived a SIGTERM'd yoyo -p by 3s (#1099); \
         exit {:?}; yoyo stderr:\n{stderr}",
        out.status
    );
}
