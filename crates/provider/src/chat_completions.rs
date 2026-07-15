use minimax_protocol::{
    ConversationItem, MessageRole, ProtocolErrorCode, StreamEvent, TurnRequest,
};
use serde_json::{Value, json};

use crate::fixture_protocol::parse_chat_completions_event;

#[derive(Clone, Copy, Debug, Default)]
pub struct ChatCompletionsAdapter;

impl ChatCompletionsAdapter {
    pub const PATH: &str = "/chat/completions";

    #[must_use]
    pub fn build_request(request: &TurnRequest) -> Value {
        let messages = request
            .messages
            .iter()
            .map(|item| match item {
                ConversationItem::Message(message) => json!({
                    "role": role_name(message.role),
                    "content": message.content,
                }),
                ConversationItem::AssistantToolCalls(batch) => json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": batch.tool_calls.iter().map(|call| json!({
                        "id": call.call_id.as_str(),
                        "type": "function",
                        "function": {
                            "name": call.name,
                            "arguments": call.arguments_json,
                        }
                    })).collect::<Vec<_>>(),
                }),
                ConversationItem::ToolResult(message) => json!({
                    "role": "tool",
                    "tool_call_id": message.tool_result.call_id.as_str(),
                    "content": json!(message.tool_result).to_string(),
                }),
            })
            .collect::<Vec<_>>();
        let mut body = json!({
            "model": request.model_id.as_str(),
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": request.output.max_output_tokens,
        });
        if !request.tools.is_empty() {
            body["tools"] = Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.parameters,
                                "strict": tool.strict,
                            }
                        })
                    })
                    .collect(),
            );
            body["tool_choice"] = Value::String("auto".to_owned());
        }
        body
    }

    pub fn parse_frame(raw: &str) -> Result<Vec<StreamEvent>, ProtocolErrorCode> {
        parse_chat_completions_event(raw)
    }
}

fn role_name(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}
