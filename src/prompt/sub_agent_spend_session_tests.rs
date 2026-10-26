//! Session-cumulative sub-agent spend and its `/cost` lines (#1073).
//! Driven through local stores (the `drain_failure_note(&store)` seam), never
//! the process globals.

use super::*;
use yoagent::{Content, Usage};

fn usage(input: u64, output: u64) -> Usage {
    Usage {
        input,
        output,
        cache_read: 0,
        cache_write: 0,
        total_tokens: 0,
    }
}

/// Built through yoagent's own serialisation, as the loop leaves it (d211).
fn delegated(input: u64, output: u64, cost: Option<f64>) -> ToolResult {
    let mut child = SessionStats::default();
    child.usage = usage(input, output);
    child.cost_usd = cost;
    let mut details = serde_json::json!({});
    details[yoagent::SUB_AGENT_STATS_KEY] = serde_json::to_value(&child).expect("serialize");
    ToolResult {
        content: vec![Content::Text { text: "ok".into() }],
        details,
    }
}

fn spend_of(results: &[ToolResult]) -> SubAgentSpend {
    let mut s = SubAgentSpend::default();
    for r in results {
        fold_sub_agent_result(&mut s, r);
    }
    s
}

#[test]
fn session_bucket_sees_both_records_and_survives_the_per_run_drain() {
    let run = Mutex::new(None);
    let session = Mutex::new(None);
    record_into(&run, &session, &delegated(1_000, 100, Some(0.10)));
    // The per-run drain (audit_log_usage) empties only the run bucket.
    let first_run = drain_with(&run);
    assert_eq!(first_run.runs, 1);
    assert!(drain_with(&run).is_empty());
    record_into(&run, &session, &delegated(500, 50, Some(0.05)));

    let seen = peek_with(&session);
    assert_eq!(seen.runs, 2, "both delegations, across the drain");
    assert_eq!(seen.usage.input, 1_500);
    assert_eq!(seen.usage.output, 150);
    assert!((seen.cost_usd.unwrap() - 0.15).abs() < 1e-9);
    // Reading does not clear: a second read is identical.
    assert_eq!(peek_with(&session), seen);
    // And the per-run bucket holds only the run since its last drain.
    assert_eq!(drain_with(&run).runs, 1);
}

#[test]
fn unpriced_delegation_stays_none_in_the_session_bucket() {
    let run = Mutex::new(None);
    let session = Mutex::new(None);
    record_into(&run, &session, &delegated(10, 1, Some(0.01)));
    record_into(&run, &session, &delegated(20, 2, None));
    let seen = peek_with(&session);
    assert_eq!(seen.runs, 2);
    assert_eq!(
        seen.cost_usd, None,
        "unpriced spend must not read as priced"
    );
    assert_eq!(seen.usage.input, 30);
}

#[test]
fn priced_delegation_gets_its_own_line_and_a_labelled_total() {
    let spend = spend_of(&[
        delegated(1_000, 200, Some(0.02)),
        delegated(1_000, 200, Some(0.03)),
    ]);
    assert_eq!(
        session_cost_header(0.5, &spend),
        "  Session cost (own, excluding sub-agents): $0.500"
    );
    assert_eq!(
        sub_agent_cost_lines(Some(0.5), &spend).unwrap(),
        "  sub-agents: 2 runs, $0.050 (priced at each child model's rates)\n    \
         2.0k in / 400 out\n  Total including sub-agents: $0.550"
    );
    let one = spend_of(&[delegated(1, 1, Some(0.001))]);
    assert!(sub_agent_cost_lines(Some(0.0), &one)
        .unwrap()
        .starts_with("  sub-agents: 1 run, $0.0010 "));
}

#[test]
fn unpriced_delegation_says_unknown_never_zero() {
    let spend = spend_of(&[delegated(10, 1, Some(0.01)), delegated(2_000, 300, None)]);
    let out = sub_agent_cost_lines(Some(0.25), &spend).unwrap();
    // Anti-vacuous: the fixture really is non-zero tokens with no price.
    assert!(spend.usage.input > 0 && spend.cost_usd.is_none());
    assert_eq!(
        out,
        "  sub-agents: 2 runs, cost unknown (some delegated spend could not be priced; not $0)\n    \
         2.0k in / 301 out\n  Total including sub-agents: unknown (delegated spend is unpriced)"
    );
    assert!(!out.contains("$0.0000") && !out.contains("$0.25"));
    // Own model unpriced, delegation priced: the total is still unknown.
    let priced = spend_of(&[delegated(1, 1, Some(0.02))]);
    assert!(sub_agent_cost_lines(None, &priced)
        .unwrap()
        .ends_with("  Total including sub-agents: unknown (own spend is unpriced)"));
}

/// Near-miss: no delegation — the common case — leaves `/cost` exactly as it
/// was: the old header byte-for-byte and no extra block at all.
#[test]
fn no_delegation_keeps_cost_output_byte_identical() {
    let none = SubAgentSpend::default();
    assert_eq!(session_cost_header(0.0123, &none), "  Session cost: $0.012");
    assert_eq!(session_cost_header(0.004, &none), "  Session cost: $0.0040");
    assert_eq!(sub_agent_cost_lines(Some(1.0), &none), None);
    assert_eq!(sub_agent_cost_lines(None, &none), None);
    // An ordinary tool result recorded into both buckets adds nothing.
    let run = Mutex::new(None);
    let session = Mutex::new(None);
    let plain = ToolResult {
        content: vec![],
        details: serde_json::json!({"exit_code": 0}),
    };
    record_into(&run, &session, &plain);
    assert_eq!(peek_with(&session).runs, 0);
    assert_eq!(sub_agent_cost_lines(Some(1.0), &peek_with(&session)), None);
}
