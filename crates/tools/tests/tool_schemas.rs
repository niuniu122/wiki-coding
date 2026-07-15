use minimax_protocol::{ToolCall, ToolEffect, ToolInvocation, V1_TOOL_NAMES};
use minimax_tools::{NeverCancelled, Preflight, ToolRegistry};
use serde_json::{Value, json};

const SCHEMA_FIXTURE: &str = include_str!("../../../fixtures/compat/tools/v1-schemas.json");
const DENIAL_FIXTURE: &str = include_str!("../../../fixtures/compat/tools/denial-matrix.v1.json");

#[test]
fn tool_schemas_match_the_finite_v1_fixture() {
    let specs = must(ToolRegistry::specs());
    let names: Vec<_> = specs
        .iter()
        .map(|spec| spec.definition.name.as_str())
        .collect();
    assert_eq!(names, V1_TOOL_NAMES);
    assert_eq!(specs.len(), 8);
    assert!(specs.iter().all(|spec| {
        spec.definition.strict
            && spec.definition.parameters["additionalProperties"] == Value::Bool(false)
    }));

    let definitions: Vec<_> = specs.into_iter().map(|spec| spec.definition).collect();
    let actual = json!({"schema_version": 1, "tools": definitions});
    let expected: Value = must(serde_json::from_str(SCHEMA_FIXTURE));
    assert_eq!(actual, expected);
    let second_definitions: Vec<_> = must(ToolRegistry::specs())
        .into_iter()
        .map(|spec| spec.definition)
        .collect();
    let second = json!({"schema_version": 1, "tools": second_definitions});
    assert_eq!(
        must(serde_json::to_vec(&actual)),
        must(serde_json::to_vec(&second))
    );
}

#[test]
fn denial_matrix_is_identical_without_a_permission_input() {
    let fixture: Value = must(serde_json::from_str(DENIAL_FIXTURE));
    let cases = must_option(fixture["cases"].as_array());
    let modes = must_option(fixture["permission_modes"].as_array());
    assert_eq!(
        modes,
        &[
            Value::String("confirm".into()),
            Value::String("full-access".into())
        ]
    );
    for case in cases {
        let name = must_option(case["tool"].as_str());
        let mut effect = must_option(must(ToolRegistry::find(name))).effect;
        if let Some(override_effect) = case.get("effect").and_then(Value::as_str) {
            effect = match override_effect {
                "read" => ToolEffect::Read,
                "write" => ToolEffect::Write,
                "process" => ToolEffect::Process,
                unexpected => panic!("unexpected effect {unexpected}"),
            };
        }
        let invocation = invocation(name, effect, case["arguments"].clone());
        let expected_code = must_option(case["code"].as_str());
        for _mode in modes {
            let error = match Preflight::check(&invocation, &NeverCancelled) {
                Ok(_) => panic!("fixture case unexpectedly passed"),
                Err(error) => error,
            };
            assert_eq!(error.code().as_str(), expected_code);
        }
    }
}

#[test]
fn unknown_fields_and_names_fail_closed() {
    let extra = invocation(
        "read_file",
        ToolEffect::Read,
        json!({"path": "README.md", "surprise": true}),
    );
    assert_eq!(
        must_error(Preflight::check(&extra, &NeverCancelled))
            .code()
            .as_str(),
        "invalid_arguments"
    );

    let unknown = invocation("delete_file", ToolEffect::Write, json!({}));
    assert_eq!(
        must_error(Preflight::check(&unknown, &NeverCancelled))
            .code()
            .as_str(),
        "unknown_tool"
    );

    let wrong_type = invocation(
        "write_file",
        ToolEffect::Write,
        json!({"path": "safe.txt", "mode": "create", "content": 42}),
    );
    assert_eq!(
        must_error(Preflight::check(&wrong_type, &NeverCancelled))
            .code()
            .as_str(),
        "invalid_arguments"
    );
}

#[test]
fn cancellation_and_secret_markers_stop_before_dispatch() {
    let invocation = invocation(
        "write_file",
        ToolEffect::Write,
        json!({"path": "safe.txt", "mode": "create", "content": "password=abcdefghijklmnop"}),
    );
    assert_eq!(
        must_error(Preflight::check(&invocation, &NeverCancelled))
            .code()
            .as_str(),
        "secret_content"
    );
    assert_eq!(
        must_error(Preflight::check(&invocation, &true))
            .code()
            .as_str(),
        "cancelled"
    );
}

fn invocation(name: &str, effect: ToolEffect, arguments: Value) -> ToolInvocation {
    let call = must(ToolCall::new(
        must(minimax_protocol::ToolCallId::new("call-schema")),
        name,
        must(serde_json::to_string(&arguments)),
    ));
    must(ToolInvocation::new(call, effect))
}

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

fn must_error<T, E: std::fmt::Debug>(result: Result<T, E>) -> E {
    match result {
        Ok(_) => panic!("expected error"),
        Err(error) => error,
    }
}

fn must_option<T>(value: Option<T>) -> T {
    match value {
        Some(value) => value,
        None => panic!("expected value"),
    }
}
