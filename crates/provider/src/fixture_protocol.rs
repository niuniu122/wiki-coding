use std::collections::BTreeMap;

use minimax_core::StreamSequence;
use minimax_protocol::{
    ProtocolErrorCode, ProviderProtocolKind, StreamEvent, TerminalOutcome, ToolCall,
    ToolCallFragment, ToolCallId, ToolValidationError, Usage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Final compatibility events compared with the language-neutral fixtures.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompatibilityEvent {
    ReasoningFiltered,
    TextDelta {
        delta: String,
    },
    ToolCall {
        call_id: ToolCallId,
        name: String,
        arguments_json: String,
    },
    Usage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_tokens: Option<u64>,
    },
    Completed,
    Failed {
        code: ProtocolErrorCode,
    },
    Interrupted,
    Stopped,
}

/// Both reducer input and final compatibility projection from one fixture replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureReplay {
    pub stream_events: Vec<StreamEvent>,
    pub compatibility_events: Vec<CompatibilityEvent>,
}

pub fn parse_responses_event(raw: &str) -> Result<Vec<StreamEvent>, ProtocolErrorCode> {
    if raw == "[DONE]" {
        return Ok(vec![completed()]);
    }
    let value = parse_json(raw)?;
    let event_type = string_at(&value, &["type"]).ok_or(ProtocolErrorCode::UnknownEvent)?;

    match event_type {
        "response.reasoning.delta" => Ok(vec![StreamEvent::ReasoningFiltered]),
        "response.output_text.delta" => Ok(vec![StreamEvent::VisibleTextDelta {
            delta: string_at(&value, &["delta"])
                .ok_or(ProtocolErrorCode::MalformedJson)?
                .to_owned(),
        }]),
        "response.function_call_arguments.delta" => {
            let stream_id = string_at(&value, &["item_id"]).map(str::to_owned);
            let call_id = string_at(&value, &["call_id"])
                .map(str::to_owned)
                .or_else(|| stream_id.as_ref().map(|value| format!("stream:{value}")))
                .ok_or(ProtocolErrorCode::MissingToolCallId)?;
            Ok(vec![StreamEvent::ToolCallFragments {
                fragments: vec![ToolCallFragment {
                    call_id: ToolCallId::new(call_id)?,
                    stream_id,
                    name: None,
                    arguments_delta: Some(
                        string_at(&value, &["delta"]).unwrap_or_default().to_owned(),
                    ),
                    arguments_complete: false,
                    index: None,
                }],
            }])
        }
        "response.output_item.added" | "response.output_item.done" => {
            let item = value
                .get("item")
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
                .ok_or(ProtocolErrorCode::UnknownEvent)?;
            let call_id = string_at(item, &["call_id"])
                .or_else(|| string_at(item, &["id"]))
                .ok_or(ProtocolErrorCode::MissingToolCallId)?;
            let name = string_at(item, &["name"])
                .ok_or(ProtocolErrorCode::MalformedJson)?
                .to_owned();
            Ok(vec![StreamEvent::ToolCallFragments {
                fragments: vec![ToolCallFragment {
                    call_id: ToolCallId::new(call_id)?,
                    stream_id: string_at(item, &["id"]).map(str::to_owned),
                    name: Some(name),
                    arguments_delta: (event_type == "response.output_item.done")
                        .then(|| string_at(item, &["arguments"]).map(str::to_owned))
                        .flatten(),
                    arguments_complete: event_type == "response.output_item.done"
                        && string_at(item, &["arguments"]).is_some(),
                    index: None,
                }],
            }])
        }
        "response.completed" => {
            let mut events = Vec::new();
            if let Some(usage) =
                usage_at(&value, &["response", "usage"]).or_else(|| usage_at(&value, &["usage"]))
            {
                events.push(StreamEvent::Usage { usage });
            }
            events.push(completed());
            Ok(events)
        }
        "response.failed" | "response.incomplete" => Ok(vec![StreamEvent::Terminal {
            outcome: TerminalOutcome::Failed {
                code: ProtocolErrorCode::UnknownEvent,
            },
        }]),
        _ => Err(ProtocolErrorCode::UnknownEvent),
    }
}

pub fn parse_chat_completions_event(raw: &str) -> Result<Vec<StreamEvent>, ProtocolErrorCode> {
    if raw == "[DONE]" {
        return Ok(vec![completed()]);
    }
    let value = parse_json(raw)?;

    if value.get("error").is_some() {
        return Ok(vec![StreamEvent::Terminal {
            outcome: TerminalOutcome::Failed {
                code: ProtocolErrorCode::UnknownEvent,
            },
        }]);
    }
    if let Some(content) = string_at(&value, &["choices", "0", "delta", "content"]) {
        return Ok(vec![StreamEvent::VisibleTextDelta {
            delta: content.to_owned(),
        }]);
    }
    if string_at(&value, &["choices", "0", "delta", "reasoning_content"]).is_some() {
        return Ok(vec![StreamEvent::ReasoningFiltered]);
    }
    if let Some(tool_calls) = value
        .pointer("/choices/0/delta/tool_calls")
        .and_then(Value::as_array)
    {
        let fragments = tool_calls
            .iter()
            .enumerate()
            .map(|(array_index, call)| {
                let index = call
                    .get("index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or(u32::try_from(array_index).unwrap_or(u32::MAX));
                let provisional = format!("index:{index}");
                let call_id = string_at(call, &["id"]).unwrap_or(&provisional);
                Ok(ToolCallFragment {
                    call_id: ToolCallId::new(call_id)?,
                    stream_id: None,
                    name: string_at(call, &["function", "name"]).map(str::to_owned),
                    arguments_delta: string_at(call, &["function", "arguments"]).map(str::to_owned),
                    arguments_complete: false,
                    index: Some(index),
                })
            })
            .collect::<Result<Vec<_>, ProtocolErrorCode>>()?;
        if !fragments.is_empty() {
            return Ok(vec![StreamEvent::ToolCallFragments { fragments }]);
        }
    }
    if let Some(usage) = usage_at(&value, &["usage"]) {
        return Ok(vec![StreamEvent::Usage { usage }]);
    }

    Err(ProtocolErrorCode::UnknownEvent)
}

pub fn replay_fixture<'a>(
    protocol: ProviderProtocolKind,
    raw_events: impl IntoIterator<Item = &'a str>,
) -> Result<FixtureReplay, ProtocolErrorCode> {
    let mut sequence = StreamSequence::new();
    let mut stream_events = Vec::new();
    let mut compatibility_events = Vec::new();
    let mut tools = ToolAccumulator::default();

    for raw in raw_events {
        let parsed = match protocol {
            ProviderProtocolKind::Responses => parse_responses_event(raw)?,
            ProviderProtocolKind::ChatCompletions => parse_chat_completions_event(raw)?,
        };
        for event in parsed {
            if !matches!(event, StreamEvent::ToolCallFragments { .. }) {
                compatibility_events.extend(tools.flush()?);
            }
            sequence.accept(event.clone())?;
            match &event {
                StreamEvent::ToolCallFragments { fragments } => tools.accept(fragments)?,
                _ => compatibility_events.push(project_event(&event)),
            }
            stream_events.push(event);
        }
    }

    compatibility_events.extend(tools.flush()?);
    sequence.finish_eof()?;
    Ok(FixtureReplay {
        stream_events,
        compatibility_events,
    })
}

#[derive(Default)]
struct ToolAccumulator {
    by_id: BTreeMap<String, ToolAssembly>,
    id_by_index: BTreeMap<u32, String>,
    id_by_stream: BTreeMap<String, String>,
    order: Vec<String>,
}

#[derive(Default)]
struct ToolAssembly {
    name: Option<String>,
    arguments: String,
    index: Option<u32>,
}

impl ToolAccumulator {
    fn accept(&mut self, fragments: &[ToolCallFragment]) -> Result<(), ProtocolErrorCode> {
        for fragment in fragments {
            let raw_id = fragment.call_id.as_str();
            let actual_id = if raw_id.starts_with("index:") {
                fragment
                    .index
                    .and_then(|index| self.id_by_index.get(&index).cloned())
                    .unwrap_or_else(|| raw_id.to_owned())
            } else if raw_id.starts_with("stream:") {
                fragment
                    .stream_id
                    .as_ref()
                    .and_then(|stream_id| self.id_by_stream.get(stream_id).cloned())
                    .or_else(|| {
                        fragment
                            .stream_id
                            .as_ref()
                            .filter(|stream_id| self.by_id.contains_key(*stream_id))
                            .cloned()
                    })
                    .unwrap_or_else(|| raw_id.to_owned())
            } else {
                if let Some(index) = fragment.index {
                    register_alias(&mut self.id_by_index, index, raw_id)?;
                    self.promote(&format!("index:{index}"), raw_id)?;
                }
                if let Some(stream_id) = &fragment.stream_id {
                    register_alias(&mut self.id_by_stream, stream_id.clone(), raw_id)?;
                    self.promote(&format!("stream:{stream_id}"), raw_id)?;
                }
                raw_id.to_owned()
            };
            if !self.by_id.contains_key(&actual_id) {
                self.order.push(actual_id.clone());
            }
            let assembly = self.by_id.entry(actual_id).or_default();
            if assembly.index.is_some()
                && fragment.index.is_some()
                && assembly.index != fragment.index
            {
                return Err(ProtocolErrorCode::DuplicateToolCallId);
            }
            assembly.index = fragment.index.or(assembly.index);
            if let Some(name) = &fragment.name {
                if assembly.name.is_some() && !fragment.arguments_complete {
                    return Err(ProtocolErrorCode::DuplicateToolCallId);
                }
                assembly.name = Some(name.clone());
            }
            if let Some(arguments) = &fragment.arguments_delta {
                if fragment.arguments_complete {
                    assembly.arguments.clone_from(arguments);
                } else {
                    assembly.arguments.push_str(arguments);
                }
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<Vec<CompatibilityEvent>, ProtocolErrorCode> {
        let pending = std::mem::take(&mut self.by_id);
        self.id_by_index.clear();
        self.id_by_stream.clear();
        let order = std::mem::take(&mut self.order);
        order
            .into_iter()
            .map(|call_id| {
                let assembly = pending
                    .get(&call_id)
                    .ok_or(ProtocolErrorCode::InvalidToolArguments)?;
                if call_id.starts_with("index:") || call_id.starts_with("stream:") {
                    return Err(ProtocolErrorCode::MissingToolCallId);
                }
                let name = assembly
                    .name
                    .clone()
                    .ok_or(ProtocolErrorCode::MalformedJson)?;
                let call = ToolCall::new(
                    ToolCallId::new(call_id)?,
                    name.clone(),
                    assembly.arguments.clone(),
                )
                .map_err(protocol_error_from_tool_validation)?;
                Ok(CompatibilityEvent::ToolCall {
                    call_id: call.call_id,
                    name,
                    arguments_json: call.arguments_json,
                })
            })
            .collect()
    }

    fn promote(&mut self, provisional: &str, actual: &str) -> Result<(), ProtocolErrorCode> {
        if provisional == actual {
            return Ok(());
        }
        if let Some(assembly) = self.by_id.remove(provisional) {
            if self.by_id.contains_key(actual) {
                return Err(ProtocolErrorCode::DuplicateToolCallId);
            }
            self.by_id.insert(actual.to_owned(), assembly);
            if let Some(position) = self.order.iter().position(|value| value == provisional) {
                self.order[position] = actual.to_owned();
            }
        }
        Ok(())
    }
}

fn register_alias<K: Ord + Clone>(
    aliases: &mut BTreeMap<K, String>,
    key: K,
    call_id: &str,
) -> Result<(), ProtocolErrorCode> {
    if aliases
        .get(&key)
        .is_some_and(|existing| existing != call_id)
    {
        return Err(ProtocolErrorCode::DuplicateToolCallId);
    }
    aliases.insert(key, call_id.to_owned());
    Ok(())
}

fn protocol_error_from_tool_validation(error: ToolValidationError) -> ProtocolErrorCode {
    match error {
        ToolValidationError::ArgumentsTooLarge => ProtocolErrorCode::ToolArgumentsTooLarge,
        ToolValidationError::ArgumentsNotObject => ProtocolErrorCode::InvalidToolArguments,
        ToolValidationError::EmptyName
        | ToolValidationError::InvalidName
        | ToolValidationError::DescriptionTooLarge
        | ToolValidationError::InvalidParametersSchema
        | ToolValidationError::UnknownArgument
        | ToolValidationError::MissingArgument
        | ToolValidationError::InvalidCode
        | ToolValidationError::ResultTooLarge => ProtocolErrorCode::MalformedJson,
    }
}

fn project_event(event: &StreamEvent) -> CompatibilityEvent {
    match event {
        StreamEvent::ReasoningFiltered => CompatibilityEvent::ReasoningFiltered,
        StreamEvent::VisibleTextDelta { delta } => CompatibilityEvent::TextDelta {
            delta: delta.clone(),
        },
        StreamEvent::Usage { usage } => CompatibilityEvent::Usage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
        },
        StreamEvent::Terminal { outcome } => match outcome {
            TerminalOutcome::Completed => CompatibilityEvent::Completed,
            TerminalOutcome::Failed { code } => CompatibilityEvent::Failed { code: *code },
            TerminalOutcome::Interrupted => CompatibilityEvent::Interrupted,
            TerminalOutcome::Stopped => CompatibilityEvent::Stopped,
        },
        StreamEvent::ToolCallFragments { .. } => {
            unreachable!("tool fragments are accumulated before compatibility projection")
        }
    }
}

fn completed() -> StreamEvent {
    StreamEvent::Terminal {
        outcome: TerminalOutcome::Completed,
    }
}

fn parse_json(raw: &str) -> Result<Value, ProtocolErrorCode> {
    serde_json::from_str(raw).map_err(|_| ProtocolErrorCode::MalformedJson)
}

fn usage_at(value: &Value, path: &[&str]) -> Option<Usage> {
    let usage = value_at(value, path)?;
    Some(Usage {
        input_tokens: number_at(usage, &["input_tokens"])
            .or_else(|| number_at(usage, &["prompt_tokens"])),
        output_tokens: number_at(usage, &["output_tokens"])
            .or_else(|| number_at(usage, &["completion_tokens"])),
        total_tokens: number_at(usage, &["total_tokens"]),
    })
}

fn string_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    value_at(value, path)?.as_str()
}

fn number_at(value: &Value, path: &[&str]) -> Option<u64> {
    value_at(value, path)?.as_u64()
}

fn value_at<'a>(mut value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for segment in path {
        value = if let Ok(index) = segment.parse::<usize>() {
            value.as_array()?.get(index)?
        } else {
            value.get(*segment)?
        };
    }
    Some(value)
}
