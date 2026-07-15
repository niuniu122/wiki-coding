use std::fs;
use std::path::{Path, PathBuf};

use minimax_protocol::{
    AgentLimits, AssistantToolCallBatch, ConversationItem, MessageRole, ModelId, ModelMessage,
    OutputSettings, ProtocolErrorCode, ProviderId, ProviderProtocolKind, RequestId, SchemaVersion,
    SessionId, StreamEvent, ToolCall, ToolCallId, ToolDefinition, ToolResult, ToolResultMessage,
    ToolTerminalStatus, TurnId, TurnRequest,
};
use minimax_provider::{
    ChatCompletionsAdapter, CompatibilityEvent, ResponsesAdapter, replay_fixture,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidCase {
    case_id: String,
    protocol: ProviderProtocolKind,
    raw: Vec<String>,
    expected_events: Vec<CompatibilityEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InvalidFixture {
    schema_version: u16,
    cases: Vec<InvalidCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvalidCase {
    case_id: String,
    protocol: ProviderProtocolKind,
    raw: Vec<String>,
    expected_error: ExpectedError,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedError {
    code: ProtocolErrorCode,
    category: String,
    forbidden_fragments: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolRoundTripFixture {
    schema_version: u16,
    calls: Vec<ToolCallFixture>,
    results: Vec<ToolResultFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCallFixture {
    call_id: String,
    name: String,
    arguments_json: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolResultFixture {
    call_id: String,
    tool_name: String,
    status: ToolTerminalStatus,
    code: String,
    #[serde(default)]
    output: Option<String>,
}

#[test]
fn both_provider_protocols_match_every_valid_compatibility_fixture() {
    for fixture in [
        fixture_path("responses.valid.jsonl"),
        fixture_path("chat-completions.valid.jsonl"),
    ] {
        let contents = fs::read_to_string(&fixture).expect("fixture should be readable");
        for line in contents.lines().filter(|line| !line.trim().is_empty()) {
            let case: ValidCase = serde_json::from_str(line).expect("fixture should be valid");
            let replay = replay_fixture(case.protocol, case.raw.iter().map(String::as_str))
                .unwrap_or_else(|code| panic!("{} failed with {code}", case.case_id));

            assert_eq!(
                replay.compatibility_events, case.expected_events,
                "{}",
                case.case_id
            );
            assert_eq!(
                replay
                    .stream_events
                    .iter()
                    .filter(|event| matches!(event, StreamEvent::Terminal { .. }))
                    .count(),
                1,
                "{}",
                case.case_id
            );
            let serialized = serde_json::to_string(&replay.compatibility_events)
                .expect("events should serialize");
            assert!(
                !serialized.contains("PRIVATE_REASONING"),
                "{}",
                case.case_id
            );
            assert!(
                !serialized.contains("SECRET_PROVIDER_DETAIL"),
                "{}",
                case.case_id
            );
        }
    }
}

#[test]
fn every_invalid_fixture_returns_its_declared_safe_code() {
    let contents = fs::read_to_string(fixture_path("invalid-cases.v1.json"))
        .expect("fixture should be readable");
    let fixture: InvalidFixture = serde_json::from_str(&contents).expect("fixture should be valid");
    assert_eq!(fixture.schema_version, 1);

    for case in fixture.cases {
        assert_eq!(case.expected_error.category, "protocol", "{}", case.case_id);
        let error = replay_fixture(case.protocol, case.raw.iter().map(String::as_str))
            .expect_err("invalid fixture should fail");
        assert_eq!(error, case.expected_error.code, "{}", case.case_id);
        let serialized = serde_json::to_string(&error).expect("error code should serialize");
        for forbidden in case.expected_error.forbidden_fragments {
            assert!(!serialized.contains(&forbidden), "{}", case.case_id);
        }
    }
}

#[test]
fn native_requests_preserve_complete_ordered_tool_history() {
    let raw = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures/compat/tools/provider-roundtrip.v1.json"),
    )
    .expect("tool fixture should be readable");
    let fixture: ToolRoundTripFixture = serde_json::from_str(&raw).expect("strict fixture");
    assert_eq!(fixture.schema_version, 1);
    let calls = fixture
        .calls
        .into_iter()
        .map(|call| {
            ToolCall::new(
                ToolCallId::new(call.call_id).expect("call ID"),
                call.name,
                call.arguments_json,
            )
            .expect("complete object arguments")
        })
        .collect::<Vec<_>>();
    let results = fixture
        .results
        .into_iter()
        .map(|result| ToolResult {
            schema_version: SchemaVersion,
            call_id: ToolCallId::new(result.call_id).expect("result call ID"),
            tool_name: result.tool_name,
            status: result.status,
            code: result.code,
            output: result.output,
        })
        .collect::<Vec<_>>();
    let request = tool_request(calls.clone(), results.clone());

    let responses = ResponsesAdapter::build_request(&request);
    assert_eq!(responses["input"][1]["type"], "function_call");
    assert_eq!(responses["input"][1]["call_id"], "call-1");
    assert_eq!(responses["input"][2]["call_id"], "call-2");
    assert_eq!(responses["input"][3]["type"], "function_call_output");
    assert_eq!(responses["input"][3]["call_id"], "call-1");
    assert_eq!(responses["input"][4]["call_id"], "call-2");
    assert_eq!(responses["tools"][0]["strict"], true);
    assert_eq!(responses["tool_choice"], "auto");

    let chat = ChatCompletionsAdapter::build_request(&request);
    assert_eq!(chat["messages"][1]["role"], "assistant");
    assert_eq!(
        chat["messages"][1]["tool_calls"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(chat["messages"][1]["tool_calls"][0]["id"], "call-1");
    assert_eq!(chat["messages"][1]["tool_calls"][1]["id"], "call-2");
    assert_eq!(chat["messages"][2]["role"], "tool");
    assert_eq!(chat["messages"][2]["tool_call_id"], "call-1");
    assert_eq!(chat["messages"][3]["tool_call_id"], "call-2");
    assert_eq!(chat["tools"][0]["function"]["strict"], true);
}

#[test]
fn ordinary_chat_request_wire_shape_remains_unchanged() {
    let request = base_request(vec![
        ModelMessage {
            role: MessageRole::User,
            content: "hello".to_owned(),
        }
        .into(),
    ]);
    assert_eq!(
        ResponsesAdapter::build_request(&request),
        json!({
            "model": "model-test",
            "input": [{"role":"user","content":"hello"}],
            "stream": true,
            "max_output_tokens": 128
        })
    );
    assert_eq!(
        ChatCompletionsAdapter::build_request(&request),
        json!({
            "model": "model-test",
            "messages": [{"role":"user","content":"hello"}],
            "stream": true,
            "stream_options": {"include_usage":true},
            "max_tokens": 128
        })
    );
}

fn tool_request(calls: Vec<ToolCall>, results: Vec<ToolResult>) -> TurnRequest {
    let mut messages = vec![
        ModelMessage {
            role: MessageRole::User,
            content: "inspect the project".to_owned(),
        }
        .into(),
    ];
    messages.push(ConversationItem::AssistantToolCalls(
        AssistantToolCallBatch { tool_calls: calls },
    ));
    messages.extend(
        results
            .into_iter()
            .map(|tool_result| ConversationItem::ToolResult(ToolResultMessage { tool_result })),
    );
    let mut request = base_request(messages);
    request.tools = vec![
        tool_definition("read_file"),
        tool_definition("list_directory"),
    ];
    request.agent_limits = Some(AgentLimits::default());
    request.validate().expect("valid tool request")
}

fn base_request(messages: Vec<ConversationItem>) -> TurnRequest {
    TurnRequest {
        session_id: SessionId::new("session-1").expect("session"),
        turn_id: TurnId::new("turn-1").expect("turn"),
        request_id: RequestId::new("request-1").expect("request"),
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model-test").expect("model"),
        protocol: ProviderProtocolKind::Responses,
        messages,
        tools: Vec::new(),
        agent_limits: None,
        output: OutputSettings::new(128).expect("output"),
    }
}

fn tool_definition(name: &str) -> ToolDefinition {
    ToolDefinition::new(
        name,
        "A bounded fixture tool.",
        json!({
            "type":"object",
            "properties":{"path":{"type":"string"}},
            "required":["path"],
            "additionalProperties":false
        }),
    )
    .expect("strict definition")
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/compat/provider-streams")
        .join(name)
}
