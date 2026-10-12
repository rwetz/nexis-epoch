//! The Nexis ML protocol, version 1: NDJSON events an engine prints on
//! stdout under `--nexis-protocol` and appends to `metrics.jsonl`.
//!
//! Canonical spec: `docs/ML_SUITE.md` in the Nexis repository (the engine's
//! `PROTOCOL.md` is a vendored copy). The forward-compatibility rule is
//! "ignore what you don't understand, never throw", so [`parse_line`]
//! returns `None` for anything it can't use instead of an error.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ev")]
pub enum Event {
    #[serde(rename = "run.started", rename_all = "camelCase")]
    RunStarted {
        run: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        dir: Option<String>,
        #[serde(default)]
        config: Option<serde_json::Value>,
        #[serde(default)]
        total_epochs: Option<u64>,
        #[serde(default)]
        device: Option<String>,
        #[serde(default)]
        protocol: Option<u32>,
        #[serde(default)]
        started_at: Option<String>,
    },
    #[serde(rename = "metric")]
    Metric {
        run: String,
        #[serde(default)]
        step: Option<f64>,
        #[serde(default)]
        epoch: Option<u64>,
        name: String,
        value: f64,
    },
    #[serde(rename = "epoch")]
    Epoch {
        run: String,
        epoch: u64,
        #[serde(default)]
        of: Option<u64>,
    },
    #[serde(rename = "artifact")]
    Artifact {
        run: String,
        kind: String,
        path: String,
    },
    #[serde(rename = "sample")]
    Sample {
        run: String,
        #[serde(default)]
        input: Option<serde_json::Value>,
        #[serde(default)]
        output: Option<serde_json::Value>,
    },
    #[serde(rename = "log")]
    Log {
        run: String,
        #[serde(default)]
        level: Option<String>,
        msg: String,
    },
    #[serde(rename = "run.finished")]
    RunFinished {
        run: String,
        status: String,
        #[serde(default)]
        summary: Option<serde_json::Value>,
    },
}

const KNOWN: &[&str] = &[
    "run.started",
    "metric",
    "epoch",
    "artifact",
    "sample",
    "log",
    "run.finished",
];

/// Parse one protocol line. Blank lines, invalid JSON, unknown `ev` values
/// and malformed known events all yield `None`. Unknown fields are ignored.
pub fn parse_line(line: &str) -> Option<Event> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let ev = value.get("ev")?.as_str()?;
    if !KNOWN.contains(&ev) {
        return None;
    }
    serde_json::from_value(value).ok()
}

/// A control message written to an engine's stdin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Control {
    Cancel,
    Pause,
    Resume,
}

impl Control {
    pub fn to_line(self) -> String {
        let mut s = serde_json::to_string(&self).expect("control messages serialize");
        s.push('\n');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_events_written_by_the_rust_engine() {
        let line =
            r#"{"epoch":1,"ev":"metric","name":"loss/train","run":"r","step":1,"value":0.018}"#;
        assert_eq!(
            parse_line(line),
            Some(Event::Metric {
                run: "r".into(),
                step: Some(1.0),
                epoch: Some(1),
                name: "loss/train".into(),
                value: 0.018
            })
        );
        let started = r#"{"config":{"engine":"nexis-ml-rs"},"device":"cpu","ev":"run.started","name":"tabular","protocol":1,"run":"r","startedAt":"2026-09-18T16:40:48Z","totalEpochs":30}"#;
        match parse_line(started) {
            Some(Event::RunStarted {
                total_epochs,
                device,
                ..
            }) => {
                assert_eq!(total_epochs, Some(30));
                assert_eq!(device.as_deref(), Some("cpu"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn ignores_what_it_does_not_understand() {
        assert_eq!(parse_line(""), None);
        assert_eq!(parse_line("not json"), None);
        assert_eq!(parse_line(r#"{"ev":"future.thing","run":"r"}"#), None);
        assert_eq!(parse_line(r#"{"ev":"metric","run":"r","name":"x"}"#), None);
        // Unknown fields on a known event are fine.
        assert!(parse_line(r#"{"ev":"log","run":"r","msg":"hi","color":"red"}"#).is_some());
    }

    #[test]
    fn control_messages_match_the_protocol() {
        assert_eq!(Control::Cancel.to_line(), "{\"cmd\":\"cancel\"}\n");
        assert_eq!(Control::Pause.to_line(), "{\"cmd\":\"pause\"}\n");
    }
}
