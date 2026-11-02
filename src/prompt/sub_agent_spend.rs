//! Sub-agent spend for the per-run usage record (#1073).
//!
//! `.yoyo/audit.jsonl`'s `"type":"usage"` record counted only the parent
//! agent's own `Usage`, so any run that called `sub_agent` under-reported what
//! it cost. yoagent (since 0.19) already hands delegated spend to the parent:
//! after every tool call, the loop attaches the combined `SessionStats` of the
//! runs that call delegated under `ToolResult.details["sub_agent_stats"]`, and
//! `SessionStats::from_sub_agent_result` reads it back. It does this AFTER the
//! tool returns, from a report channel the tool fills through its
//! `ToolContext` — so yoyo's sub-agent wrappers (`FallbackSubAgentTool`,
//! `DiagnosticSubAgentTool`, `SubAgentOutputMarkerTool`), which all pass the
//! caller's `ctx` through, cannot drop it even when they rebuild the result.
//! The missing hop was yoyo's: nothing read the field.
//!
//! Both `ToolExecutionEnd` doors in `prompt.rs` (the display path and the
//! quiet path) call [`record_sub_agent_spend`]; `audit_log_usage` drains the
//! store once per run with [`take_sub_agent_spend`] and the record gains a
//! `"sub_agents"` object via [`attach_sub_agent_spend`].
//!
//! **Kept apart, never folded in.** The record's top-level token fields and
//! `cost_usd` keep meaning *this agent's own turns*, exactly as every older
//! record and its readers (`scripts/extract_trajectory.py`'s usage-coverage
//! and phase-spend parsers, which read `type` and top-level `cost_usd` only)
//! understand them. A run with no delegation gets NO `sub_agents` key, so its
//! record is byte-identical to before — the same rule yoagent applies to
//! `SessionStats::sub_agents` (`skip_serializing_if = is_empty`).
//!
//! The sub-agent `cost_usd` is yoagent's own figure — each delegated run
//! priced at its own model's rates — and is `null` when any delegated spend
//! could not be priced (never "free"; non-zero tokens beside a `null` say so).

use std::sync::Mutex;

use yoagent::{SessionStats, SubAgentSpend, ToolResult};

use crate::sync_util::lock_or_recover;

/// The process-wide accumulator the two prompt doors write and
/// `audit_log_usage` drains. Pure helpers below take the store as a parameter
/// so tests drive a local `Mutex` instead (the `drain_failure_note` seam).
static SUB_AGENT_SPEND: Mutex<Option<SubAgentSpend>> = Mutex::new(None);

/// Fold one tool result's delegated spend into `spend`. A result with no
/// `sub_agent_stats` (every non-delegating tool) adds nothing.
///
/// Mirrors yoagent's own (crate-private) `SubAgentSpend::record_run`: the
/// child's own usage and cost count as one run, plus everything the child
/// itself delegated, so nested sub-agents are counted. One `sub_agent` call
/// runs one child, so the per-call combined stats are one run.
pub(crate) fn fold_sub_agent_result(spend: &mut SubAgentSpend, result: &ToolResult) {
    let Some(child) = SessionStats::from_sub_agent_result(result) else {
        return;
    };
    let mut subtree = SubAgentSpend::default();
    subtree.usage = child.usage.clone();
    subtree.cost_usd = child.cost_usd;
    subtree.runs = 1;
    subtree.merge(&child.sub_agents);
    spend.merge(&subtree);
}

// `SubAgentSpend` is `#[non_exhaustive]`, so a `static` cannot spell it out;
// `None` is the const-constructible "nothing recorded yet".
fn record_with(store: &Mutex<Option<SubAgentSpend>>, result: &ToolResult) {
    let mut guard = lock_or_recover(store);
    fold_sub_agent_result(guard.get_or_insert_with(SubAgentSpend::default), result);
}

fn drain_with(store: &Mutex<Option<SubAgentSpend>>) -> SubAgentSpend {
    lock_or_recover(store).take().unwrap_or_default()
}

/// The session-cumulative twin of [`SUB_AGENT_SPEND`], read by `/cost`.
/// Fed at the same point as the per-run bucket and **never drained**: the
/// per-run bucket is emptied by `audit_log_usage` once per run, so a second
/// drainer would steal from it (and it from `/cost`). Placement, stated
/// (d226): in the authority, so every reader of delegated spend sees it.
static SESSION_SUB_AGENT_SPEND: Mutex<Option<SubAgentSpend>> = Mutex::new(None);

/// Record one result into both buckets — the per-run one (drained) and the
/// session one (only ever read).
fn record_into(
    run: &Mutex<Option<SubAgentSpend>>,
    session: &Mutex<Option<SubAgentSpend>>,
    result: &ToolResult,
) {
    record_with(run, result);
    record_with(session, result);
}

/// Read a bucket without clearing it.
fn peek_with(store: &Mutex<Option<SubAgentSpend>>) -> SubAgentSpend {
    lock_or_recover(store).clone().unwrap_or_default()
}

/// Delegated spend per sub_agent call, keyed by the parent's `tool_call_id`
/// (#1108). The id is also on the `ToolCall` block of the assistant message
/// that issued the call, so `/cost` can put each call's spend on the turn
/// that dispatched it. Only delegating calls are stored.
static SUB_AGENT_SPEND_BY_CALL: Mutex<Vec<(String, SubAgentSpend)>> = Mutex::new(Vec::new());

/// Record one call's delegated spend under its id, or nothing for a result
/// that carried no sub-agent stats.
fn record_call_with(
    store: &Mutex<Vec<(String, SubAgentSpend)>>,
    tool_call_id: &str,
    result: &ToolResult,
) {
    let mut one = SubAgentSpend::default();
    fold_sub_agent_result(&mut one, result);
    if !one.is_empty() {
        lock_or_recover(store).push((tool_call_id.to_string(), one));
    }
}

/// Called from both `ToolExecutionEnd` arms in `prompt.rs`.
pub(crate) fn record_sub_agent_spend(tool_call_id: &str, result: &ToolResult) {
    record_into(&SUB_AGENT_SPEND, &SESSION_SUB_AGENT_SPEND, result);
    record_call_with(&SUB_AGENT_SPEND_BY_CALL, tool_call_id, result);
}

/// Every delegating call so far this session, with its call id. Never drains.
pub(crate) fn sub_agent_spend_by_call() -> Vec<(String, SubAgentSpend)> {
    lock_or_recover(&SUB_AGENT_SPEND_BY_CALL).clone()
}

/// `N run(s), X in / Y out, $Z` -- or `unpriced (not $0)` when any delegated
/// spend in `s` had no price.
fn spend_summary(s: &SubAgentSpend) -> String {
    use crate::format::{format_cost, format_token_count};
    let cost = match s.cost_usd {
        Some(c) => format_cost(c),
        None => "unpriced (not $0)".to_string(),
    };
    format!(
        "{} run{}, {} in / {} out, {cost}",
        s.runs,
        if s.runs == 1 { "" } else { "s" },
        format_token_count(s.usage.input),
        format_token_count(s.usage.output)
    )
}

/// The block `/cost` prints under its per-turn table (#1108), or `None` when
/// nothing was delegated -- every user who never used sub_agent, whose
/// `/cost` must stay byte-identical. Turns are numbered exactly as
/// `format::extract_turn_costs` numbers them (one per assistant message), and
/// a call is attributed to the assistant message whose `ToolCall` carries its
/// id. Calls whose turn is no longer in `messages` (`/clear`, compaction) get
/// their own line rather than vanishing, and the total covers all of them.
/// ASCII only, so plain output needs no separate form.
pub(crate) fn delegated_turn_lines(
    messages: &[yoagent::AgentMessage],
    by_call: &[(String, SubAgentSpend)],
) -> Option<String> {
    if by_call.is_empty() {
        return None;
    }
    let mut used = vec![false; by_call.len()];
    let mut lines =
        vec!["    Sub-agent spend by turn (not in the per-turn costs above):".to_string()];
    let mut turn = 0;
    for msg in messages {
        let yoagent::AgentMessage::Llm(yoagent::Message::Assistant { content, .. }) = msg else {
            continue;
        };
        turn += 1;
        let mut here = SubAgentSpend::default();
        for block in content {
            let yoagent::Content::ToolCall { id, .. } = block else {
                continue;
            };
            for (i, (call_id, spend)) in by_call.iter().enumerate() {
                if !used[i] && call_id == id {
                    here.merge(spend);
                    used[i] = true;
                }
            }
        }
        if !here.is_empty() {
            lines.push(format!("      Turn {turn}: {}", spend_summary(&here)));
        }
    }
    let mut elsewhere = SubAgentSpend::default();
    let mut total = SubAgentSpend::default();
    for (i, (_, spend)) in by_call.iter().enumerate() {
        total.merge(spend);
        if !used[i] {
            elsewhere.merge(spend);
        }
    }
    if !elsewhere.is_empty() {
        lines.push(format!(
            "      Not in the turns above (cleared or compacted): {}",
            spend_summary(&elsewhere)
        ));
    }
    lines.push(format!("      Total delegated: {}", spend_summary(&total)));
    Some(lines.join("\n"))
}

/// Everything delegated so far this session. Does not clear anything, so it
/// cannot race the per-run drain.
pub(crate) fn session_sub_agent_spend() -> SubAgentSpend {
    peek_with(&SESSION_SUB_AGENT_SPEND)
}

/// The label `/cost` puts on its first line. With nothing delegated it is the
/// old `Session cost: $X` byte-for-byte; with delegation it says the number
/// is this session's own spend only, so it cannot be read as the whole bill.
pub(crate) fn session_cost_header(own_cost: f64, spend: &SubAgentSpend) -> String {
    let label = if spend.runs == 0 {
        "Session cost"
    } else {
        "Session cost (own, excluding sub-agents)"
    };
    format!("  {label}: {}", crate::format::format_cost(own_cost))
}

/// The delegated-spend block for `/cost`, or `None` when nothing was
/// delegated (the common case, which keeps `/cost` byte-identical). Unpriced
/// delegated spend is reported as unknown with its tokens — never as `$0`,
/// the same rule the usage record follows with `null`. `own_cost` is `None`
/// when the session's own model has no price either.
pub(crate) fn sub_agent_cost_lines(own_cost: Option<f64>, spend: &SubAgentSpend) -> Option<String> {
    use crate::format::{format_cost, format_token_count};
    if spend.runs == 0 {
        return None;
    }
    let runs = format!(
        "{} run{}",
        spend.runs,
        if spend.runs == 1 { "" } else { "s" }
    );
    let tokens = format!(
        "{} in / {} out",
        format_token_count(spend.usage.input),
        format_token_count(spend.usage.output)
    );
    let (sub_line, total_line) = match (spend.cost_usd, own_cost) {
        (Some(sub), Some(own)) => (
            format!("  sub-agents: {runs}, {} (priced at each child model's rates)", format_cost(sub)),
            format!("  Total including sub-agents: {}", format_cost(own + sub)),
        ),
        (Some(sub), None) => (
            format!("  sub-agents: {runs}, {} (priced at each child model's rates)", format_cost(sub)),
            "  Total including sub-agents: unknown (own spend is unpriced)".to_string(),
        ),
        (None, _) => (
            format!("  sub-agents: {runs}, cost unknown (some delegated spend could not be priced; not $0)"),
            "  Total including sub-agents: unknown (delegated spend is unpriced)".to_string(),
        ),
    };
    Some(format!("{sub_line}\n    {tokens}\n{total_line}"))
}

/// Drain everything delegated since the last drain (once per run, at emit).
pub(crate) fn take_sub_agent_spend() -> SubAgentSpend {
    drain_with(&SUB_AGENT_SPEND)
}

/// Add a `"sub_agents"` object to a usage record — or nothing at all when no
/// delegation happened, so a non-delegating run's record is byte-identical.
pub(crate) fn attach_sub_agent_spend(
    mut record: serde_json::Value,
    spend: &SubAgentSpend,
) -> serde_json::Value {
    if spend.is_empty() {
        return record;
    }
    if let Some(obj) = record.as_object_mut() {
        obj.insert(
            "sub_agents".to_string(),
            serde_json::json!({
                "runs": spend.runs,
                "input_tokens": spend.usage.input,
                "output_tokens": spend.usage.output,
                "cache_read_input_tokens": spend.usage.cache_read,
                "cache_creation_input_tokens": spend.usage.cache_write,
                "cost_usd": spend.cost_usd,
            }),
        );
    }
    record
}

#[cfg(test)]
#[path = "sub_agent_spend_session_tests.rs"]
mod sub_agent_spend_session_tests;

#[cfg(test)]
#[path = "sub_agent_spend_turn_tests.rs"]
mod sub_agent_spend_turn_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use yoagent::{Content, Usage};

    fn usage(input: u64, output: u64, cache_read: u64, cache_write: u64) -> Usage {
        Usage {
            input,
            output,
            cache_read,
            cache_write,
            total_tokens: 0,
        }
    }

    /// A tool result shaped exactly as yoagent's loop leaves it after a
    /// `sub_agent` call: the child's `SessionStats` serialized under
    /// `SUB_AGENT_STATS_KEY` — produced through yoagent's own serializer,
    /// never a hand-typed JSON row (d211).
    fn delegated_result(child: &SessionStats) -> ToolResult {
        let mut details = serde_json::json!({});
        details[yoagent::SUB_AGENT_STATS_KEY] = serde_json::to_value(child).expect("serialize");
        ToolResult {
            content: vec![Content::Text {
                text: "child said ok".into(),
            }],
            details,
        }
    }

    fn child(u: Usage, cost: Option<f64>) -> SessionStats {
        let mut s = SessionStats::default();
        s.usage = u;
        s.cost_usd = cost;
        s.turns = 2;
        s
    }

    #[test]
    fn one_delegation_folds_into_the_spend_and_reaches_the_record() {
        let store = Mutex::new(None);
        let c = child(usage(1_000, 200, 300, 40), Some(0.25));
        record_with(&store, &delegated_result(&c));
        let spend = drain_with(&store);
        assert_eq!(spend.runs, 1);
        assert_eq!(spend.usage.input, 1_000);
        assert_eq!(spend.cost_usd, Some(0.25));

        let own = serde_json::json!({"type": "usage", "input_tokens": 7, "cost_usd": 0.01});
        let rec = attach_sub_agent_spend(own.clone(), &spend);
        assert_eq!(
            rec["sub_agents"],
            serde_json::json!({
                "runs": 1,
                "input_tokens": 1_000,
                "output_tokens": 200,
                "cache_read_input_tokens": 300,
                "cache_creation_input_tokens": 40,
                "cost_usd": 0.25,
            })
        );
        // Own spend stays attributable: every pre-existing key is untouched.
        let mut without = rec.clone();
        without.as_object_mut().unwrap().remove("sub_agents");
        assert_eq!(without, own);
        // Drained: a second drain is empty.
        assert!(drain_with(&store).is_empty());
    }

    #[test]
    fn nested_delegation_and_two_calls_are_both_counted() {
        let store = Mutex::new(None);
        let mut parent_child = child(usage(100, 10, 0, 0), Some(0.10));
        // The child itself delegated one grandchild.
        let grandchild = child(usage(50, 5, 0, 0), Some(0.05));
        let mut nested = SubAgentSpend::default();
        fold_sub_agent_result(&mut nested, &delegated_result(&grandchild));
        parent_child.sub_agents = nested;
        record_with(&store, &delegated_result(&parent_child));
        record_with(
            &store,
            &delegated_result(&child(usage(1, 1, 0, 0), Some(0.01))),
        );
        let spend = drain_with(&store);
        assert_eq!(spend.runs, 3, "child + grandchild + second child");
        assert_eq!(spend.usage.input, 151);
        assert!((spend.cost_usd.unwrap() - 0.16).abs() < 1e-9);
    }

    #[test]
    fn unpriced_delegation_reports_null_cost_with_its_tokens() {
        let mut spend = SubAgentSpend::default();
        fold_sub_agent_result(
            &mut spend,
            &delegated_result(&child(usage(10, 1, 0, 0), None)),
        );
        let rec = attach_sub_agent_spend(serde_json::json!({}), &spend);
        assert_eq!(rec["sub_agents"]["cost_usd"], serde_json::Value::Null);
        assert_eq!(rec["sub_agents"]["input_tokens"], 10);
    }

    /// Near-miss: an ordinary tool result (no `sub_agent_stats`) adds nothing,
    /// and a record with nothing delegated is byte-identical — no
    /// `sub_agents` key at all, full-value `assert_eq!`.
    #[test]
    fn a_run_with_no_delegation_leaves_the_record_byte_identical() {
        let store = Mutex::new(None);
        let plain = ToolResult {
            content: vec![Content::Text { text: "ok".into() }],
            details: serde_json::json!({"exit_code": 0}),
        };
        record_with(&store, &plain);
        let null_details = ToolResult {
            content: vec![],
            details: serde_json::Value::Null,
        };
        record_with(&store, &null_details);
        let spend = drain_with(&store);
        assert!(spend.is_empty());
        let own = serde_json::json!({
            "type": "usage", "model": "m", "input_tokens": 3, "cost_usd": null,
        });
        let rec = attach_sub_agent_spend(own.clone(), &spend);
        assert_eq!(rec, own);
        assert_eq!(rec.to_string(), own.to_string());
    }
}
