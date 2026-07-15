use minimax_protocol::{
    ConversationItem, MessageRole, ProtocolErrorCode, StreamEvent, TurnRequest,
};
use serde_json::{Value, json};

use crate::fixture_protocol::parse_responses_event;

#[derive(Clone, Copy, Debug, Default)]
pub struct ResponsesAdapter;

impl ResponsesAdapter {
    pub const PATH: &str = "/responses";

    #[must_use]
    pub fn build_request(request: &TurnRequest) -> Value {
        let mut input = Vec::new();
        for item in &request.messages {
            match item {
                ConversationItem::Message(message) => input.push(json!({
                    "role": role_name(message.role),
                    "content": message.content,
                })),
                ConversationItem::AssistantToolCalls(batch) => {
                    input.extend(batch.tool_calls.iter().map(|call| {
                        json!({
                            "type": "function_call",
                            "call_id": call.call_id.as_str(),
                            "name": call.name,
                            "arguments": call.arguments_json,
                        })
                    }));
                }
                ConversationItem::ToolResult(message) => input.push(json!({
                    "type": "function_call_output",
                    "call_id": message.tool_result.call_id.as_str(),
                    "output": json!(message.tool_result).to_string(),
                })),
            }
        }
        let mut body = json!({
            "model": request.model_id.as_str(),
            "input": input,
            "stream": true,
            "max_output_tokens": request.output.max_output_tokens,
        });
        if !request.tools.is_empty() {
            body["tools"] = Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                            "strict": tool.strict,
                        })
                    })
                    .collect(),
            );
            body["tool_choice"] = Value::String("auto".to_owned());
        }
        body
    }

    pub fn parse_frame(raw: &str) -> Result<Vec<StreamEvent>, ProtocolErrorCode> {
        parse_responses_event(raw)
    }
}

fn role_name(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}
