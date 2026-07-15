use std::collections::BTreeMap;

use minimax_protocol::{
    RuntimeEvent, RuntimeEventV1, RuntimeTerminalOutcome, SessionRecord, SessionStatus, ToolEffect,
    ToolInvocation, ToolResult, TraceCode, TraceEntry,
};

const MAX_RENDER_CHARS: usize = 16_000;

pub struct EventRenderer;

impl EventRenderer {
    #[must_use]
    pub fn event(record: &RuntimeEventV1) -> String {
        let rendered = match &record.event {
            RuntimeEvent::TurnStarted {
                session_id,
                turn_id,
                request_id,
            } => format!(
                "turn started | session={} | turn={} | request={}",
                session_id.as_str(),
                turn_id.as_str(),
                request_id.as_str()
            ),
            RuntimeEvent::VisibleTextDelta { delta } => delta.clone(),
            RuntimeEvent::ReasoningFiltered => "hidden reasoning filtered".to_owned(),
            RuntimeEvent::ToolCallObserved { call_id, name } => format!(
                "tool request observed | call={} | name={}",
                call_id.as_str(),
                name.as_deref().unwrap_or("unknown")
            ),
            RuntimeEvent::Usage { usage } => format!(
                "usage | input={} | output={} | total={}",
                render_token_count(usage.input_tokens),
                render_token_count(usage.output_tokens),
                render_token_count(usage.total_tokens)
            ),
            RuntimeEvent::Diagnostic { code } => format!("diagnostic | {code:?}"),
            RuntimeEvent::Terminal { outcome } => terminal(outcome),
        };
        sanitize_bounded(&rendered)
    }

    #[must_use]
    pub fn history(session: &SessionRecord) -> String {
        let mut lines = vec![format!(
            "session {} | {:?}",
            session.session_id.as_str(),
            session.status
        )];
        for turn in &session.turns {
            lines.push(format!(
                "user [{}]: {}",
                turn.turn_id.as_str(),
                turn.user_message.content
            ));
            if let Some(assistant) = &turn.assistant_message {
                let suffix = if assistant.partial {
                    format!(" [partial {:?}]", turn.status)
                } else {
                    String::new()
                };
                lines.push(format!("assistant: {}{suffix}", assistant.content));
            }
        }
        sanitize_bounded(&lines.join("\n"))
    }

    #[must_use]
    pub fn sessions(sessions: &[(&str, SessionStatus, u64, usize)]) -> String {
        if sessions.is_empty() {
            return "no sessions".to_owned();
        }
        sanitize_bounded(
            &sessions
                .iter()
                .map(|(id, status, updated, turns)| {
                    let marker = if *status == SessionStatus::Active {
                        "*"
                    } else {
                        " "
                    };
                    format!("{marker} {id} | {status:?} | updated={updated} | turns={turns}")
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    #[must_use]
    pub fn trace(entries: &[TraceEntry], expanded: bool) -> String {
        if expanded {
            return sanitize_bounded(
                &entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{} | {:?} | {}",
                            entry.recorded_at_unix_ms,
                            entry.code,
                            render_facts(&entry.facts)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        let mut counts = BTreeMap::<TraceCode, u64>::new();
        for entry in entries {
            *counts.entry(entry.code).or_insert(0) += 1;
        }
        sanitize_bounded(
            &counts
                .into_iter()
                .map(|(code, count)| format!("{code:?}={count}"))
                .collect::<Vec<_>>()
                .join(" | "),
        )
    }

    #[must_use]
    pub fn not_available(command: &str, owning_phase: u8) -> String {
        sanitize_bounded(&format!(
            "{command} is not available in the Rust development shell until Phase {owning_phase}"
        ))
    }

    #[must_use]
    pub fn approval_request(invocation: &ToolInvocation) -> String {
        let mut value = serde_json::from_str::<serde_json::Value>(&invocation.call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let scope = value
            .get("path")
            .and_then(|path| path.as_str())
            .map(|path| path.replace('\\', "/"))
            .unwrap_or_else(|| "project".to_owned());
        if let Some(path) = value.get_mut("path")
            && let Some(raw) = path.as_str()
        {
            *path = serde_json::Value::String(raw.replace('\\', "/"));
        }
        let arguments = serde_json::to_string(&value).unwrap_or_else(|_| "<invalid>".to_owned());
        sanitize_bounded(&format!(
            "approval required | call={} | tool={} | effect={} | scope={} | arguments={}\nType exactly yes to allow this one call: ",
            invocation.call.call_id.as_str(),
            invocation.call.name,
            effect_name(invocation.effect),
            scope,
            arguments
        ))
    }

    #[must_use]
    pub fn tool_result(result: &ToolResult) -> String {
        sanitize_bounded(&format!(
            "tool result | call={} | tool={} | status={:?} | code={}{}",
            result.call_id.as_str(),
            result.tool_name,
            result.status,
            result.code,
            result
                .output
                .as_deref()
                .map_or_else(String::new, |output| format!(" | output={output}"))
        ))
    }
}

const fn effect_name(effect: ToolEffect) -> &'static str {
    match effect {
        ToolEffect::Read => "read",
        ToolEffect::Write => "write",
        ToolEffect::Process => "process",
    }
}

fn terminal(outcome: &RuntimeTerminalOutcome) -> String {
    match outcome {
        RuntimeTerminalOutcome::Completed => "terminal | completed".to_owned(),
        RuntimeTerminalOutcome::Interrupted => "terminal | interrupted".to_owned(),
        RuntimeTerminalOutcome::Stopped => "terminal | stopped".to_owned(),
        RuntimeTerminalOutcome::Failed { failure } => format!("terminal | failed | {failure}"),
    }
}

fn render_facts(facts: &BTreeMap<String, String>) -> String {
    facts
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_token_count(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |value| value.to_string())
}

fn sanitize_bounded(value: &str) -> String {
    let mut rendered = String::new();
    let mut rendered_chars = 0_usize;
    for character in value.chars() {
        if rendered_chars >= MAX_RENDER_CHARS {
            rendered.push('…');
            break;
        }
        match character {
            '\n' => {
                rendered.push('\n');
                rendered_chars += 1;
            }
            '\t' => {
                let spaces = (MAX_RENDER_CHARS - rendered_chars).min(4);
                rendered.extend(std::iter::repeat_n(' ', spaces));
                rendered_chars += spaces;
            }
            character if character.is_control() => {
                rendered.push('�');
                rendered_chars += 1;
            }
            character => {
                rendered.push(character);
                rendered_chars += 1;
            }
        }
    }
    rendered
}
