use std::collections::BTreeMap;

use minimax_protocol::{TraceCode, TraceEntry};

const MAX_FACT_VALUE_BYTES: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SafeTraceFact {
    String(String),
    U64(u64),
    I64(i64),
    Bool(bool),
    Null,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoldedTrace {
    pub total: u64,
    pub last_recorded_at_unix_ms: Option<u64>,
    pub counts: BTreeMap<TraceCode, u64>,
}

pub struct SafeTraceRecorder;

impl SafeTraceRecorder {
    #[must_use]
    pub fn record(
        recorded_at_unix_ms: u64,
        code: TraceCode,
        input: BTreeMap<String, SafeTraceFact>,
    ) -> TraceEntry {
        let allowed = allowed_facts(code);
        let facts = input
            .into_iter()
            .filter(|(key, _)| allowed.contains(&key.as_str()))
            .filter_map(|(key, value)| sanitize_fact(value).map(|value| (key, value)))
            .collect();
        TraceEntry {
            recorded_at_unix_ms,
            code,
            facts,
        }
    }

    #[must_use]
    pub fn fold(entries: &[TraceEntry]) -> FoldedTrace {
        let mut counts = BTreeMap::new();
        for entry in entries {
            *counts.entry(entry.code).or_insert(0) += 1;
        }
        FoldedTrace {
            total: u64::try_from(entries.len()).unwrap_or(u64::MAX),
            last_recorded_at_unix_ms: entries.iter().map(|entry| entry.recorded_at_unix_ms).max(),
            counts,
        }
    }
}

fn allowed_facts(code: TraceCode) -> &'static [&'static str] {
    match code {
        TraceCode::TurnStarted => &["turn_id"],
        TraceCode::ProviderConnected => &["provider_id", "protocol", "model"],
        TraceCode::ProviderFailed => &["provider_id", "kind", "status", "retryable", "request_id"],
        TraceCode::TurnInterrupted | TraceCode::TurnRecovered => {
            &["turn_id", "had_assistant_draft"]
        }
        TraceCode::CompactionCompleted => &[
            "compaction_id",
            "covered_through_turn_id",
            "before_tokens",
            "after_tokens",
        ],
        TraceCode::CommandRejected => &["command", "reason"],
    }
}

fn sanitize_fact(value: SafeTraceFact) -> Option<String> {
    let rendered = match value {
        SafeTraceFact::String(value) => {
            if contains_prohibited_material(&value) {
                "[REDACTED]".to_owned()
            } else {
                value
            }
        }
        SafeTraceFact::U64(value) => value.to_string(),
        SafeTraceFact::I64(value) => value.to_string(),
        SafeTraceFact::Bool(value) => value.to_string(),
        SafeTraceFact::Null => "null".to_owned(),
    };
    (rendered.len() <= MAX_FACT_VALUE_BYTES).then_some(rendered)
}

fn contains_prohibited_material(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let named_marker = [
        "api_key",
        "apikey",
        "password",
        "authorization",
        "bearer ",
        "sk-",
        "ghp_",
        "github_pat_",
        "glpat-",
        "private key",
        "do_not_persist",
        "<think",
        "<analysis",
        "<reasoning",
        "raw_frame",
        "tool_body",
        "tool body",
        "\"choices\"",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern));
    let high_entropy = value.len() >= 32
        && value.bytes().any(|byte| byte.is_ascii_alphabetic())
        && value.bytes().any(|byte| byte.is_ascii_digit())
        && value
            .bytes()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= 10;
    named_marker || high_entropy
}
