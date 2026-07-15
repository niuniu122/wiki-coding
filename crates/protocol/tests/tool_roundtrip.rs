use minimax_protocol::{
    MAX_TOOL_ARGUMENT_BYTES, SchemaVersion, ToolCall, ToolCallId, ToolDecision, ToolDecisionKind,
    ToolDefinition, ToolEffect, ToolInvocation, ToolResult, ToolTerminalStatus,
    ToolValidationError, validate_unique_call_ids,
};
use serde_json::json;

fn definition() -> ToolDefinition {
    ToolDefinition::new(
        "read_file",
        "Read one UTF-8 project file.",
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            },
            "required": ["path"],
            "additionalProperties": false
        }),
    )
    .expect("strict definition")
}

fn call(id: &str, arguments: &str) -> ToolCall {
    ToolCall::new(
        ToolCallId::new(id).expect("call ID"),
        "read_file",
        arguments,
    )
    .expect("valid call")
}

#[test]
fn schema_one_tool_records_round_trip_strictly() {
    let definition = definition();
    let call = call("call-1", r#"{"path":"README.md"}"#);
    let invocation = ToolInvocation::new(call.clone(), ToolEffect::Read).expect("invocation");
    let decision = ToolDecision {
        schema_version: SchemaVersion,
        call_id: call.call_id.clone(),
        decision: ToolDecisionKind::Approved,
        code: "approved".to_owned(),
    }
    .validate()
    .expect("decision");
    let result = ToolResult {
        schema_version: SchemaVersion,
        call_id: call.call_id.clone(),
        tool_name: call.name.clone(),
        status: ToolTerminalStatus::Succeeded,
        code: "ok".to_owned(),
        output: Some("README".to_owned()),
    }
    .validate()
    .expect("result");

    definition.validate_call(&call).expect("schema match");
    for encoded in [
        serde_json::to_string(&definition).expect("definition JSON"),
        serde_json::to_string(&call).expect("call JSON"),
        serde_json::to_string(&invocation).expect("invocation JSON"),
        serde_json::to_string(&decision).expect("decision JSON"),
        serde_json::to_string(&result).expect("result JSON"),
    ] {
        assert!(encoded.contains("\"schema_version\":1"));
        assert!(!encoded.contains("provider"));
    }
}

#[test]
fn arguments_must_be_one_bounded_json_object() {
    assert_eq!(
        ToolCall::new(
            ToolCallId::new("call-array").expect("ID"),
            "read_file",
            "[]",
        ),
        Err(ToolValidationError::ArgumentsNotObject)
    );
    assert_eq!(
        ToolCall::new(
            ToolCallId::new("call-incomplete").expect("ID"),
            "read_file",
            r#"{"path":"README.md""#,
        ),
        Err(ToolValidationError::ArgumentsNotObject)
    );
    let oversized = format!(r#"{{"path":"{}"}}"#, "x".repeat(MAX_TOOL_ARGUMENT_BYTES));
    assert_eq!(
        ToolCall::new(
            ToolCallId::new("call-large").expect("ID"),
            "read_file",
            oversized,
        ),
        Err(ToolValidationError::ArgumentsTooLarge)
    );
}

#[test]
fn definition_rejects_unknown_and_missing_arguments() {
    let definition = definition();
    assert_eq!(
        definition.validate_call(&call("call-extra", r#"{"path":"README.md","raw":true}"#)),
        Err(ToolValidationError::UnknownArgument)
    );
    assert_eq!(
        definition.validate_call(&call("call-missing", "{}")),
        Err(ToolValidationError::MissingArgument)
    );
    assert!(
        ToolDefinition::new(
            "read_file",
            "bad schema",
            json!({"type":"object","properties":{}}),
        )
        .is_err()
    );
}

#[test]
fn duplicate_ids_and_unknown_record_fields_fail_closed() {
    let first = call("call-1", "{}");
    let duplicate = call("call-1", "{}");
    assert_eq!(
        validate_unique_call_ids([&first, &duplicate]),
        Err(first.call_id.clone())
    );
    let raw = r#"{"schema_version":1,"call_id":"call-1","name":"read_file","arguments_json":"{}","unexpected":true}"#;
    assert!(serde_json::from_str::<ToolCall>(raw).is_err());
    let invalid: ToolCall = serde_json::from_str(
        r#"{"schema_version":1,"call_id":"call-1","name":"read_file","arguments_json":"[]"}"#,
    )
    .expect("wire shape parses before semantic validation");
    assert_eq!(
        invalid.validate(),
        Err(ToolValidationError::ArgumentsNotObject)
    );
}
