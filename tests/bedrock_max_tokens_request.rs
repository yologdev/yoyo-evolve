//! #1160 at the wire: what `inferenceConfig.maxTokens` yoyo sends to Bedrock
//! for a model id with no preset.
//!
//! yoagent 0.27.7's `BedrockProvider` (`src/provider/bedrock.rs`,
//! `build_bedrock_body`) sets `inferenceConfig.maxTokens` only from
//! `StreamConfig.max_tokens`, and yoyo's agent passes that only when the user
//! set a cap (`with_max_tokens` in `AgentConfig::configure_agent`). So the
//! 16000 `ModelConfig` guesses for an unlisted bedrock id is held, never sent,
//! and the guessed-limits note must not say replies are "capped at 16000".
//! Same shape as google's #1158 (tests/google_max_tokens_request.rs).
//!
//! Stub only: `--base-url` points at 127.0.0.1, the AWS credentials are junk,
//! and no AWS endpoint is contacted. HOME and cwd are temp dirs because this
//! repo's own `.yoyo.toml` sets `max_tokens`, which would silence the note.
//!
//! Coupling with `src/model_limits_note.rs`: row (a) asserts both the wire
//! (no `maxTokens`) and the note's wording (no "capped at"), so if the wire
//! changes the note's claim fails here, and if the note starts claiming a cap
//! again this file fails while the stub shows none. Row (b) is the positive
//! control: a user cap does reach `inferenceConfig.maxTokens`, so row (a)'s
//! absence is not a parse miss.
//!
//! #1173 adds rows (c)-(e) on a Claude id yoagent has a Bedrock preset for:
//! with no user cap the preset's output maximum goes out, so a thinking
//! budget always rides with a larger `maxTokens`; a user cap goes out as given.
//!
//! #1198 (yoagent 0.27.9): with thinking on, `build_bedrock_body` itself now
//! sends `maxTokens` above `budget_tokens`: the request's cap, else the
//! `ModelConfig`'s, raised to budget + 1024 when not above it. Rows (f) and (g)
//! are the regression test for that rule with yoyo's `bedrock_max_tokens`
//! out of the path (it returns the user's 2048 in (f) and `None` in (g), so
//! only yoagent can put a value above the budget). Row (a) stays thinking-off:
//! there yoagent still sends only an explicit cap, and the note says so.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// The JSON body of an AWS validation error (`message`, as Bedrock sends it).
const ERROR_REPLY: &str = "{\"message\":\"stub: validation error (yoyo test #1160)\"}";

/// A stub on 127.0.0.1:0 that records every POST as `(request line, body)`
/// and answers each with a Bedrock-shaped 400 `ValidationException`, whatever
/// the path: a 400 is not retried, so each run makes exactly one request.
#[allow(clippy::type_complexity)]
fn start_capturing_stub() -> (u16, Arc<Mutex<Vec<(String, String)>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let port = listener.local_addr().unwrap().port();
    let posts = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::clone(&posts);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Ok(clone) = stream.try_clone() else {
                continue;
            };
            let mut reader = BufReader::new(clone);
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    break;
                }
                if let Some((k, v)) = trimmed.split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        content_length = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let body = String::from_utf8_lossy(&body).into_owned();
            if request_line.starts_with("POST") {
                store
                    .lock()
                    .unwrap()
                    .push((request_line.trim_end().to_string(), body));
            }
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\nx-amzn-errortype: ValidationException:http://internal.amazon.com/coral/com.amazon.bedrock/\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                ERROR_REPLY.len(),
                ERROR_REPLY
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, posts)
}

struct Run {
    posts: Vec<(String, String)>,
    stderr: String,
    stdout: String,
    exit: Option<i32>,
    /// `exit=None` alone cannot say why yoyo died (Day 242: a SIGPIPE death
    /// read as `exit=None`, stderr empty), so the signal is kept too.
    signal: Option<i32>,
}

#[cfg(unix)]
fn termination_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn termination_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

impl Run {
    fn ctx(&self) -> String {
        let posts: Vec<String> = self
            .posts
            .iter()
            .map(|(line, body)| format!("{line}\n{body}"))
            .collect();
        format!(
            "posts={} exit={:?} signal={:?}\nstderr:\n{}\nstdout:\n{}\nrequests:\n{}",
            self.posts.len(),
            self.exit,
            self.signal,
            self.stderr,
            self.stdout,
            posts.join("\n---\n")
        )
    }

    /// The first request's `inferenceConfig.maxTokens`, as JSON (`Null` when
    /// the key, or the whole `inferenceConfig`, is absent).
    fn max_tokens(&self) -> serde_json::Value {
        self.body()["inferenceConfig"]["maxTokens"].clone()
    }

    /// The first request's `additionalModelRequestFields.thinking.budget_tokens`
    /// (`Null` when thinking is off).
    fn thinking_budget(&self) -> serde_json::Value {
        self.body()["additionalModelRequestFields"]["thinking"]["budget_tokens"].clone()
    }

    /// The first request's body, checked to be a Bedrock Converse body.
    fn body(&self) -> serde_json::Value {
        let ctx = self.ctx();
        let (line, body) = self
            .posts
            .first()
            .unwrap_or_else(|| panic!("yoyo never reached the stub\n{ctx}"));
        assert!(
            line.contains("/converse-stream"),
            "first POST is not a Bedrock ConverseStream call\n{ctx}"
        );
        let req: serde_json::Value = serde_json::from_str(body)
            .unwrap_or_else(|e| panic!("request body is not JSON ({e})\n{ctx}"));
        assert!(
            req["messages"].is_array(),
            "request body has no `messages`; not a Converse body\n{ctx}"
        );
        req
    }
}

/// An id no preset covers (Amazon's own Nova, not a Claude id), so it runs on
/// bedrock's guess.
const UNLISTED_ID: &str = "amazon.nova-pro-v1:0";

fn run_bedrock(extra: &[&str]) -> Run {
    run_bedrock_model(UNLISTED_ID, extra)
}

/// A Claude id yoagent's Bedrock preset covers (yoyo's bedrock default).
const CLAUDE_ID: &str = "anthropic.claude-sonnet-4-6";

fn run_bedrock_model(model: &str, extra: &[&str]) -> Run {
    let tmp = tempfile::tempdir().expect("tempdir");
    let proj = tmp.path().join("proj");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    run_in(&proj, &home, model, extra)
}

fn run_in(proj: &Path, home: &Path, model: &str, extra: &[&str]) -> Run {
    let (port, posts) = start_capturing_stub();
    let base_url = format!("http://127.0.0.1:{port}");
    let mut args: Vec<&str> = vec![
        "--provider",
        "bedrock",
        "--model",
        model,
        "--base-url",
        &base_url,
    ];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["-p", "hi"]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_yoyo"))
        .args(&args)
        .current_dir(proj)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        // Junk credentials: SigV4 signs with them, the stub ignores them.
        .env("AWS_ACCESS_KEY_ID", "AKIAJUNKJUNKJUNKJUNK")
        .env("AWS_SECRET_ACCESS_KEY", "junk-secret")
        // The stub's host names no region, so SigV4 takes it from here.
        .env("AWS_REGION", "us-east-1")
        .env_remove("AWS_SESSION_TOKEN")
        .env_remove("AWS_BEARER_TOKEN_BEDROCK")
        .env_remove("AWS_DEFAULT_REGION")
        .env_remove("AWS_PROFILE")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("API_KEY")
        .env_remove("MODEL")
        .env_remove("YOYO_RESTRICTED")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn yoyo");
    let deadline = Instant::now() + Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    let posts = posts.lock().unwrap().clone();
    Run {
        posts,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        exit: out.status.code(),
        signal: termination_signal(&out.status),
    }
}

/// Row (a): no `--max-tokens`. The guessed 16000 is NOT sent, so Bedrock
/// applies the model's own default output limit, and the note says that
/// instead of claiming a cap.
#[test]
fn no_user_max_tokens_sends_no_output_cap_and_the_note_says_so() {
    let run = run_bedrock(&[]);
    let ctx = run.ctx();
    assert_eq!(
        run.max_tokens(),
        serde_json::Value::Null,
        "inferenceConfig.maxTokens went out with no user cap\n{ctx}"
    );
    assert!(
        run.stderr
            .contains("no preset is known for bedrock model 'amazon.nova-pro-v1:0'"),
        "guessed-limits note did not fire\n{ctx}"
    );
    assert!(
        !run.stderr.contains("capped at"),
        "note still claims an output cap that is never sent\n{ctx}"
    );
    assert!(
        run.stderr
            .contains("yoyo sends no output cap, so Bedrock applies the model's own default"),
        "note does not say what goes on the wire\n{ctx}"
    );
}

/// Row (b), the positive control: a user cap reaches the wire, so the stub
/// does read the field and row (a)'s `Null` is a real absence.
#[test]
fn user_max_tokens_reaches_the_wire_as_inference_config_max_tokens() {
    let run = run_bedrock(&["--max-tokens", "1234"]);
    let ctx = run.ctx();
    assert_eq!(
        run.max_tokens(),
        serde_json::json!(1234),
        "--max-tokens 1234 did not reach inferenceConfig.maxTokens\n{ctx}"
    );
    assert!(
        !run.stderr.contains("no preset is known"),
        "note fired despite a user max_tokens\n{ctx}"
    );
}

/// #1173, row (c): a Claude id with thinking on and no `--max-tokens`. The
/// thinking budget goes out, and so must an explicit `maxTokens` above it:
/// Anthropic requires `budget_tokens < max_tokens`, and an omitted
/// `maxTokens` leaves that comparison to whatever default Bedrock applies.
/// The value sent is the model's output maximum from yoagent's preset; since
/// #1198 it comes from yoagent 0.27.9's own fallback to the `ModelConfig`,
/// not from yoyo (`bedrock_max_tokens` returns `None` with thinking on).
#[test]
fn claude_with_thinking_sends_max_tokens_above_the_thinking_budget() {
    let run = run_bedrock_model(CLAUDE_ID, &["--thinking", "high"]);
    let ctx = run.ctx();
    let budget = run
        .thinking_budget()
        .as_u64()
        .unwrap_or_else(|| panic!("no thinking budget went out\n{ctx}"));
    let max = run
        .max_tokens()
        .as_u64()
        .unwrap_or_else(|| panic!("thinking budget {budget} went out with no maxTokens\n{ctx}"));
    assert!(max > budget, "maxTokens {max} <= budget {budget}\n{ctx}");
    let preset = yoagent::provider::ModelConfig::preset("bedrock", CLAUDE_ID)
        .expect("yoagent has a bedrock preset for the default Claude id");
    assert_eq!(
        max,
        u64::from(preset.max_tokens),
        "maxTokens is not the preset's output maximum\n{ctx}"
    );
}

/// #1173, row (d): the same Claude id with thinking off still sends the
/// preset's cap, one rule for preset-backed ids (the first-party Anthropic
/// path in yoagent always sends a `max_tokens` too).
#[test]
fn claude_without_thinking_sends_the_preset_max_tokens() {
    let run = run_bedrock_model(CLAUDE_ID, &[]);
    let ctx = run.ctx();
    assert_eq!(run.thinking_budget(), serde_json::Value::Null, "{ctx}");
    let preset = yoagent::provider::ModelConfig::preset("bedrock", CLAUDE_ID).unwrap();
    assert_eq!(
        run.max_tokens(),
        serde_json::json!(preset.max_tokens),
        "{ctx}"
    );
}

/// #1173, row (e): a user cap on a Claude id is sent as given, never
/// replaced by the preset's maximum.
#[test]
fn claude_user_max_tokens_is_sent_unchanged() {
    let run = run_bedrock_model(CLAUDE_ID, &["--thinking", "high", "--max-tokens", "20000"]);
    let ctx = run.ctx();
    assert_eq!(run.max_tokens(), serde_json::json!(20000), "{ctx}");
}

/// #1198, row (f): a user cap at or below the thinking budget on a Claude id.
/// yoyo passes the 2048 through unchanged (row (e)'s rule), and yoagent 0.27.9
/// raises it above `budget_tokens` (budget + 1024), as the first-party
/// Anthropic path does. Before 0.27.9 this went out as 2048 under a budget
/// of 8192, which Anthropic's extended-thinking rule forbids.
#[test]
fn claude_user_cap_below_the_thinking_budget_is_raised_above_it() {
    let run = run_bedrock_model(CLAUDE_ID, &["--thinking", "high", "--max-tokens", "2048"]);
    let ctx = run.ctx();
    let budget = run
        .thinking_budget()
        .as_u64()
        .unwrap_or_else(|| panic!("no thinking budget went out\n{ctx}"));
    assert!(
        budget >= 2048,
        "budget {budget} no longer above the cap; row is vacuous\n{ctx}"
    );
    assert_eq!(run.max_tokens(), serde_json::json!(budget + 1024), "{ctx}");
}

/// #1198, row (g): an id with no preset, thinking on, no user cap. yoyo sets
/// no request cap here, so yoagent 0.27.9 falls back to the `ModelConfig`'s
/// guessed 16000 and sends it. The guessed-limits note must say that, and
/// not only "sends no output cap", which is true only with thinking off.
#[test]
fn unlisted_id_with_thinking_sends_the_guessed_cap_and_the_note_says_so() {
    let run = run_bedrock(&["--thinking", "high"]);
    let ctx = run.ctx();
    let budget = run
        .thinking_budget()
        .as_u64()
        .unwrap_or_else(|| panic!("no thinking budget went out\n{ctx}"));
    let max = run
        .max_tokens()
        .as_u64()
        .unwrap_or_else(|| panic!("thinking budget {budget} went out with no maxTokens\n{ctx}"));
    assert!(max > budget, "maxTokens {max} <= budget {budget}\n{ctx}");
    assert_eq!(max, 16_000, "not the guessed ModelConfig cap\n{ctx}");
    assert!(
        run.stderr
            .contains("with thinking on, the guessed max_tokens = 16000 goes out as the cap"),
        "note does not say what goes on the wire with thinking on\n{ctx}"
    );
}
