use minimax_protocol::{MessageRole, ProtocolErrorCode, StreamEvent, TurnRequest};
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
            .map(|message| {
                json!({
                    "role": role_name(message.role),
                    "content": message.content,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "model": request.model_id.as_str(),
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": request.output.max_output_tokens,
        })
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
