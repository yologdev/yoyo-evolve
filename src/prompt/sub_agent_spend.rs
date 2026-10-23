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

/// Called from both `ToolExecutionEnd` arms in `prompt.rs`.
pub(crate) fn record_sub_agent_spend(result: &ToolResult) {
    record_with(&SUB_AGENT_SPEND, result);
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
