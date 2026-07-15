use minimax_protocol::{MessageRole, ProtocolErrorCode, StreamEvent, TurnRequest};
use serde_json::{Value, json};

use crate::fixture_protocol::parse_responses_event;

#[derive(Clone, Copy, Debug, Default)]
pub struct ResponsesAdapter;

impl ResponsesAdapter {
    pub const PATH: &str = "/responses";

    #[must_use]
    pub fn build_request(request: &TurnRequest) -> Value {
        let input = request
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
            "input": input,
            "stream": true,
            "max_output_tokens": request.output.max_output_tokens,
        })
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
