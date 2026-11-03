//! #1064 at the real spawn: every stdio MCP server yoyo starts gets the
//! provider API keys UNSET by default, like shell hooks, and
//! `YOYO_SUBPROCESS_ENV_SCRUB=0`/`false` turns that off. A misspelled value
//! keeps the scrub on (fails closed). A key given to one server through its
//! `[mcp_servers.<name>] env` map still reaches it, because yoagent applies
//! the caller's map after the removal.
//!
//! The "server" is a two-line shell script that dumps its environment to a
//! file and exits. It is not an MCP server, and it does not need to be: what
//! we observe is the environment of the spawned process, which both the
//! pre-flight listing and the real connect build through one seam
//! (`mcp_prefix::mcp_stdio_options`). The key is set on the yoyo CHILD
//! command only, never in this test process. Never invokes cargo.
#![cfg(unix)]

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CANARY: &str = "sk-canary-mcp";
const PLAIN_LINE: &str = "YOYO_MCP_PLAIN=passes";

/// How the dumping server gets configured for one run.
enum Server {
    /// `--mcp "sh dump.sh"` on the command line.
    Flag,
    /// `[mcp_servers.dump]` in the user config, with an explicit `env` key.
    ConfigWithEnvKey(&'static str),
}

/// Start yoyo with the dumping server and `scrub` as the value of
/// `YOYO_SUBPROCESS_ENV_SCRUB` (`None` = unset). Returns the dump's lines.
fn dump_env_of_mcp_server(scrub: Option<&str>, server: Server) -> Vec<String> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let proj = tmp.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    let out = tmp.path().join("env.txt");
    let script = tmp.path().join("dump.sh");
    // Write then rename, so the poller never reads a half-written dump.
    std::fs::write(
        &script,
        format!(
            "env > '{out}.tmp' && mv '{out}.tmp' '{out}'\nexit 0\n",
            out = out.display()
        ),
    )
    .unwrap();
    // `--mcp` splits on whitespace, so the tempdir path must not contain any.
    assert!(
        !script.display().to_string().contains(char::is_whitespace),
        "tempdir path has whitespace; --mcp would split it: {}",
        script.display()
    );

    let config_home = home.join(".config");
    let mut args: Vec<String> = vec![
        "--provider".into(),
        "anthropic".into(),
        "--model".into(),
        "claude-sonnet-4-5".into(),
        "--base-url".into(),
        "http://127.0.0.1:1".into(),
        "--max-turns".into(),
        "1".into(),
    ];
    match server {
        Server::Flag => {
            args.push("--mcp".into());
            args.push(format!("sh {}", script.display()));
        }
        Server::ConfigWithEnvKey(key) => {
            let dir = config_home.join("yoyo");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("config.toml"),
                format!(
                    "[mcp_servers.dump]\ncommand = \"sh\"\nargs = [\"{}\"]\nenv = {{ ANTHROPIC_API_KEY = \"{key}\" }}\n",
                    script.display()
                ),
            )
            .unwrap();
        }
    }
    args.push("-p".into());
    args.push("hi".into());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yoyo"));
    // Same ambient-key scrub as the sibling binary tests (mcp_process_group.rs).
    for (name, _) in std::env::vars() {
        if name == "API_KEY" || name.ends_with("_API_KEY") {
            cmd.env_remove(name);
        }
    }
    cmd.env_remove("YOYO_SUBPROCESS_ENV_SCRUB");
    if let Some(v) = scrub {
        cmd.env("YOYO_SUBPROCESS_ENV_SCRUB", v);
    }
    let mut child = cmd
        .args(&args)
        .current_dir(&proj)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("ANTHROPIC_API_KEY", CANARY)
        .env("YOYO_MCP_PLAIN", "passes")
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

    // The first spawn (the pre-flight) writes the dump; then stop yoyo.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !out.exists() && Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let output = child.wait_with_output().expect("wait");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    read_dump(&out, &stderr)
}

fn read_dump(out: &Path, stderr: &str) -> Vec<String> {
    let text = std::fs::read_to_string(out).unwrap_or_else(|e| {
        panic!(
            "the MCP server never wrote its environment to {} ({e}); \
             yoyo stderr:\n{stderr}",
            PathBuf::from(out).display()
        )
    });
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    // Anti-vacuous: the dump is the server's real environment, and an
    // ordinary variable passed through, so an absent key means removed.
    assert!(
        lines.iter().any(|l| l == PLAIN_LINE),
        "dump lacks {PLAIN_LINE}; it is not the spawned server's environment:\n{text}"
    );
    lines
}

fn key_line(lines: &[String]) -> Option<&String> {
    lines.iter().find(|l| l.starts_with("ANTHROPIC_API_KEY="))
}

fn assert_scrubbed(scrub: Option<&str>) {
    let lines = dump_env_of_mcp_server(scrub, Server::Flag);
    assert_eq!(
        key_line(&lines),
        None,
        "YOYO_SUBPROCESS_ENV_SCRUB={scrub:?}: the MCP server inherited the provider key (#1064)"
    );
}

fn assert_inherited(scrub: &str) {
    let lines = dump_env_of_mcp_server(Some(scrub), Server::Flag);
    let want = format!("ANTHROPIC_API_KEY={CANARY}");
    assert_eq!(
        key_line(&lines),
        Some(&want),
        "YOYO_SUBPROCESS_ENV_SCRUB={scrub}: the opt-out should pass the key through"
    );
}

#[test]
fn mcp_server_gets_no_provider_key_when_scrub_unset() {
    assert_scrubbed(None);
}

#[test]
fn mcp_server_gets_no_provider_key_when_scrub_is_1() {
    assert_scrubbed(Some("1"));
}

#[test]
fn mcp_server_gets_no_provider_key_when_scrub_misspelled() {
    assert_scrubbed(Some("bogus"));
}

#[test]
fn mcp_server_inherits_provider_key_when_scrub_is_0() {
    assert_inherited("0");
}

#[test]
fn mcp_server_inherits_provider_key_when_scrub_is_false() {
    assert_inherited("false");
}

#[test]
fn mcp_server_env_map_still_hands_one_server_a_key() {
    let lines = dump_env_of_mcp_server(None, Server::ConfigWithEnvKey("sk-explicit"));
    assert_eq!(
        key_line(&lines).map(String::as_str),
        Some("ANTHROPIC_API_KEY=sk-explicit"),
        "a key in [mcp_servers.<name>] env must reach that server after the scrub"
    );
}
