//! Readers for yoyo's two config tables of tables, `mcp_servers` and
//! `model_pricing`, over the file as the `toml` crate parses it (#1182,
//! slice 2 of #1184).
//!
//! Before this, both were read line by line: only `[mcp_servers.<name>]` /
//! `[model_pricing.<id>]` headers were seen, so every other spelling TOML
//! gives the same table (`[mcp_servers]` then `fs = { command = "npx" }`, a
//! top-level `mcp_servers = { ... }`, dotted keys, a multi-line `args`) was
//! never read, and the server never connected. Here the parser decides what
//! the table is, so every spelling of the same table reads the same.
//!
//! The load gate (`config::toml_parse_error`) has already refused a file that
//! is not TOML, so a parse error here only means "nothing configured".
//!
//! Wrong types never panic. An entry that cannot be read is skipped (MCP: the
//! server does not connect, rather than connecting with a different command
//! line) or recorded in `ModelPricingOverrides::dropped` (prices, which the
//! caller names). `config_unknown_keys` names the line for the user.

use super::{McpServerConfig, ModelPrice, ModelPricingOverrides};
use toml::{Table, Value};

/// Parse `content` as TOML; an unparseable file configures nothing.
pub(super) fn parse_table(content: &str) -> Table {
    content.parse::<Table>().unwrap_or_default()
}

/// A scalar as the string yoyo hands a child process (`PORT = 8080` is
/// `"8080"`, as the hand reader gave it). `None` for an array or table.
fn scalar_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Integer(i) => Some(i.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Boolean(b) => Some(b.to_string()),
        Value::Datetime(d) => Some(d.to_string()),
        Value::Array(_) | Value::Table(_) => None,
    }
}

/// One server from its table, or `None` if it cannot be started as written:
/// no string `command` (or an empty one), `args` not a list of scalars, or
/// `env` not a table of scalars.
fn mcp_server(name: &str, t: &Table) -> Option<McpServerConfig> {
    let command = match t.get("command")? {
        Value::String(s) if !s.is_empty() => s.clone(),
        _ => return None,
    };
    let args = match t.get("args") {
        None => Vec::new(),
        Some(Value::Array(items)) => items.iter().map(scalar_text).collect::<Option<_>>()?,
        Some(_) => return None,
    };
    let env = match t.get("env") {
        None => Vec::new(),
        Some(Value::Table(vars)) => vars
            .iter()
            .map(|(k, v)| scalar_text(v).map(|v| (k.clone(), v)))
            .collect::<Option<_>>()?,
        Some(_) => return None,
    };
    Some(McpServerConfig {
        name: name.to_string(),
        command,
        args,
        env,
    })
}

/// Every server under `mcp_servers`, in file order (the `toml` crate's
/// `preserve_order` feature), however the table is spelled.
pub(super) fn mcp_servers_from_table(doc: &Table) -> Vec<McpServerConfig> {
    let Some(Value::Table(servers)) = doc.get("mcp_servers") else {
        return Vec::new();
    };
    servers
        .iter()
        .filter_map(|(name, v)| match v {
            Value::Table(t) => mcp_server(name, t),
            _ => None,
        })
        .collect()
}

/// A rate: a finite, non-negative number. Zero is accepted on purpose (a
/// local model really can cost nothing); a quoted `"1.0"` is not a number.
fn rate(v: Option<&Value>) -> Option<f64> {
    let r = match v? {
        Value::Float(f) => *f,
        Value::Integer(i) => *i as f64,
        _ => return None,
    };
    (r.is_finite() && r >= 0.0).then_some(r)
}

/// Every price under `model_pricing`, however the table is spelled. An id
/// whose entry is not a table with two valid rates is dropped and named.
pub(super) fn model_pricing_from_table(doc: &Table) -> ModelPricingOverrides {
    let mut out = ModelPricingOverrides::default();
    let Some(Value::Table(models)) = doc.get("model_pricing") else {
        return out;
    };
    for (id, v) in models {
        let price = match v {
            Value::Table(t) => rate(t.get("input")).zip(rate(t.get("output"))),
            _ => None,
        };
        match price {
            Some((i, o)) => out.entries.push((
                id.clone(),
                ModelPrice {
                    input_per_million: i,
                    output_per_million: o,
                },
            )),
            None => out.dropped.push(id.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    type Row = (String, String, Vec<String>, Vec<(String, String)>);

    fn servers(doc: &str) -> Vec<Row> {
        mcp_servers_from_table(&parse_table(doc))
            .into_iter()
            .map(|s| (s.name, s.command, s.args, s.env))
            .collect()
    }

    fn fs(args: &[&str], env: &[(&str, &str)]) -> Vec<Row> {
        vec![(
            "fs".to_string(),
            "npx".to_string(),
            args.iter().map(|a| a.to_string()).collect(),
            env.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )]
    }

    /// One row per spelling of the same server; each must read in full.
    #[test]
    fn every_spelling_of_an_mcp_server_reads_the_same() {
        let want = fs(&["-y", "x"], &[("A", "b")]);
        for doc in [
            // The header form the hand reader read.
            "[mcp_servers.fs]\ncommand = \"npx\"\nargs = [\"-y\", \"x\"]\nenv = { A = \"b\" }\n",
            // #1182: an inline table under the bare header.
            "[mcp_servers]\nfs = { command = \"npx\", args = [\"-y\", \"x\"], env = { A = \"b\" } }\n",
            // #1180: the whole table inline at top level.
            "mcp_servers = { fs = { command = \"npx\", args = [\"-y\", \"x\"], env = { A = \"b\" } } }\n",
            // Dotted keys, top level and under the bare header.
            "mcp_servers.fs.command = \"npx\"\nmcp_servers.fs.args = [\"-y\", \"x\"]\nmcp_servers.fs.env.A = \"b\"\n",
            "[mcp_servers]\nfs.command = \"npx\"\nfs.args = [\"-y\", \"x\"]\nfs.env = { A = \"b\" }\n",
            // A sub-table for env, and an args list over several lines.
            "[mcp_servers.fs]\ncommand = \"npx\"\nargs = [\n  \"-y\",\n  \"x\",\n]\n[mcp_servers.fs.env]\nA = \"b\"\n",
            // Trailing comments on every header and value (#1174/#1177).
            "[mcp_servers] # servers\nfs = { command = \"npx\", args = [\"-y\", \"x\"], env = { A = \"b\" } } # mine\n",
            "[mcp_servers.fs] # c\ncommand = \"npx\" # c\nargs = [\"-y\", \"x\"] # c\nenv = { A = \"b\" } # c\n",
        ] {
            assert_eq!(servers(doc), want, "{doc:?}");
        }
    }

    /// The shapes the hand reader already read give exactly what it gave.
    #[test]
    fn shapes_the_hand_reader_read_are_unchanged() {
        assert_eq!(
            servers("[mcp_servers.a]\ncommand = \"x\"\n[mcp_servers.b]\ncommand = \"y\"\nargs = []\nenv = { PORT = 8080 }\n"),
            vec![
                ("a".into(), "x".into(), vec![], vec![]),
                ("b".into(), "y".into(), vec![], vec![("PORT".into(), "8080".into())]),
            ],
            "file order kept, a number in env is its text"
        );
        // No command: not a server (the hand reader dropped it too).
        assert!(servers("[mcp_servers.fs]\nargs = [\"x\"]\n").is_empty());
        assert!(servers("[mcp_servers.fs]\ncommand = \"\"\n").is_empty());
        // Padding inside quotes is kept, for the whitespace warning to name.
        assert_eq!(
            servers("[mcp_servers.fs]\ncommand = \"npx \"\n")[0].1,
            "npx "
        );
    }

    /// Wrong types skip the server (never a different command line), and
    /// never panic.
    #[test]
    fn wrong_types_skip_the_server() {
        for doc in [
            "[mcp_servers.fs]\ncommand = 5\n",
            "[mcp_servers.fs]\ncommand = [\"npx\"]\n",
            "[mcp_servers.fs]\ncommand = \"npx\"\nargs = \"-y\"\n",
            "[mcp_servers.fs]\ncommand = \"npx\"\nargs = [[\"-y\"]]\n",
            "[mcp_servers.fs]\ncommand = \"npx\"\nenv = \"A=b\"\n",
            "[mcp_servers.fs]\ncommand = \"npx\"\nenv = { A = [\"b\"] }\n",
            "[mcp_servers]\nfs = \"npx\"\n",
            "mcp_servers = \"fs\"\n",
            "[[mcp_servers.fs]]\ncommand = \"npx\"\n",
            "not toml at all = = =",
        ] {
            assert!(servers(doc).is_empty(), "{doc:?}");
        }
        // One bad server does not take the good one with it.
        assert_eq!(
            servers("[mcp_servers]\nbad = { command = 5 }\nfs = { command = \"npx\" }\n"),
            fs(&[], &[])
        );
    }

    fn prices(doc: &str) -> (Vec<(String, f64, f64)>, Vec<String>) {
        let p = model_pricing_from_table(&parse_table(doc));
        let entries = p
            .entries
            .iter()
            .map(|(id, r)| (id.clone(), r.input_per_million, r.output_per_million))
            .collect();
        (entries, p.dropped)
    }

    #[test]
    fn every_spelling_of_a_model_price_reads_the_same() {
        let want = (vec![("m".to_string(), 1.0, 2.0)], vec![]);
        for doc in [
            "[model_pricing.m]\ninput = 1.0\noutput = 2.0\n",
            "[model_pricing.\"m\"] # mine\ninput = 1.0 # c\noutput = 2 # c\n",
            "[model_pricing]\nm = { input = 1.0, output = 2.0 }\n",
            "[model_pricing] # c\nm = { input = 1, output = 2.0 } # c\n",
            "model_pricing = { m = { input = 1.0, output = 2.0 } }\n",
            "model_pricing.m = { input = 1.0, output = 2.0 }\n",
            "model_pricing.m.input = 1.0\nmodel_pricing.m.output = 2.0\n",
        ] {
            assert_eq!(prices(doc), want, "{doc:?}");
        }
    }

    #[test]
    fn a_bad_price_is_dropped_and_named_and_zero_is_a_price() {
        assert_eq!(
            prices("[model_pricing.local]\ninput = 0\noutput = 0.0\n"),
            (vec![("local".into(), 0.0, 0.0)], vec![])
        );
        for doc in [
            "[model_pricing.m]\ninput = 1.0\n",
            "[model_pricing.m]\ninput = -1.0\noutput = 2.0\n",
            "[model_pricing.m]\ninput = \"1.0\"\noutput = 2.0\n",
            "[model_pricing.m]\ninput = nan\noutput = 2.0\n",
            "[model_pricing.m]\ninput = inf\noutput = 2.0\n",
            "[model_pricing]\nm = 5\n",
        ] {
            assert_eq!(prices(doc), (vec![], vec!["m".to_string()]), "{doc:?}");
        }
    }
}
