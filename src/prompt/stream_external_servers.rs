//! The one extra NDJSON line `--output-format stream-json` emits when a
//! configured MCP/OpenAPI server failed to connect.
//!
//! Four audiences learn about a failed connect: the user (stderr notes), the
//! model (the one-shot turn prefix, Day 181), `--output-format json` (the
//! `external_servers` envelope key, #895) and — here — a stream-json consumer.
//! Before this, the stream only forwarded yoagent events, so a script reading
//! it had no machine-readable way to tell a degraded run from a healthy one.
//!
//! Contract, pinned by the tests below:
//! - The line is emitted **after** the last yoagent event of the stream, never
//!   before, so the first line is still `{"type":"agentStart"}`.
//! - It is emitted **only** when at least one server failed. A healthy run, or
//!   one with no servers configured, is byte-identical to before — that is the
//!   whole regression surface.
//! - Its fields are the **same object** `external_servers_json` produces (one
//!   serializer, never a second), plus `"type":"externalServers"`.
//! - It reads the **never-drained** report (`external_server_report`), never
//!   `take_external_failure_note`, so it cannot steal the model-facing note.

use crate::agent_builder::{external_servers_json, ExternalServerReport};

/// The NDJSON line for a degraded run, or `None` when every server connected.
pub(crate) fn external_servers_stream_line(report: &ExternalServerReport) -> Option<String> {
    if report.mcp_failed.is_empty() && report.openapi_failed.is_empty() {
        return None;
    }
    let mut value = external_servers_json(report);
    let obj = value.as_object_mut()?;
    obj.insert(
        "type".to_string(),
        serde_json::Value::String("externalServers".to_string()),
    );
    serde_json::to_string(&value).ok()
}

/// Print the line to stdout if this session's report has a failure.
/// Called by both stream-json entry points after the event stream ends.
pub(crate) fn emit_external_servers_line() {
    if let Some(line) =
        external_servers_stream_line(&crate::agent_builder::external_server_report())
    {
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_emits_nothing() {
        // The regression surface: healthy / no-server runs stay byte-identical.
        assert_eq!(
            external_servers_stream_line(&ExternalServerReport::default()),
            None
        );
        // Connected-only is still healthy: a count alone is not a failure.
        let connected = ExternalServerReport {
            mcp_connected: 3,
            openapi_connected: 1,
            ..Default::default()
        };
        assert_eq!(external_servers_stream_line(&connected), None);
    }

    #[test]
    fn one_mcp_failure_names_kind_and_id() {
        let report = ExternalServerReport {
            mcp_connected: 1,
            mcp_failed: vec!["npx bad-server".to_string()],
            ..Default::default()
        };
        let line = external_servers_stream_line(&report).expect("a failure must emit");
        assert!(!line.contains('\n'), "NDJSON: exactly one line: {line}");
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({
                "type": "externalServers",
                "mcp_connected": 1,
                "mcp_failed": ["npx bad-server"],
                "openapi_connected": 0,
                "openapi_failed": [],
            })
        );
    }

    #[test]
    fn one_openapi_failure_reports_openapi_not_mcp() {
        let report = ExternalServerReport {
            openapi_failed: vec!["./spec.yaml".to_string()],
            ..Default::default()
        };
        let line = external_servers_stream_line(&report).expect("a failure must emit");
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["type"], "externalServers");
        assert_eq!(parsed["openapi_failed"], serde_json::json!(["./spec.yaml"]));
        assert_eq!(parsed["mcp_failed"], serde_json::json!([]));
    }

    #[test]
    fn line_carries_exactly_the_envelope_object_plus_type() {
        // One authority: every field of the json envelope's object appears
        // unchanged, and the only addition is `type`.
        let report = ExternalServerReport {
            mcp_connected: 2,
            mcp_failed: vec!["cmd-a".to_string()],
            openapi_connected: 1,
            openapi_failed: vec!["spec-b".to_string()],
        };
        let line = external_servers_stream_line(&report).unwrap();
        let mut parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        let obj = parsed.as_object_mut().unwrap();
        assert_eq!(obj.remove("type"), Some(serde_json::json!("externalServers")));
        assert_eq!(parsed, external_servers_json(&report));
    }

    #[test]
    fn emitter_reads_the_never_drained_report() {
        // Weak source-level guard: the emitter must read the snapshot, never
        // drain the model-facing note. Needles assembled at runtime so this
        // test cannot match itself.
        let src = include_str!("stream_external_servers.rs");
        let start = src
            .find(&format!("fn {}()", "emit_external_servers_line"))
            .unwrap();
        let body = &src[start..start + src[start..].find("\n}\n").unwrap()];
        assert!(body.contains(&format!("{}()", "external_server_report")));
        assert!(!body.contains(&format!("take_{}", "external_failure_note")));
    }
}
