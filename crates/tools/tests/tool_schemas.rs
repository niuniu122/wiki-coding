use minimax_core::{CancellationFuture, CancellationPort, PermissionMode, ToolExecutionContext};
use minimax_protocol::{
    FULL_ACCESS_TOOL_NAMES, SHELL_TOOL_NAMES, ToolCall, ToolEffect, ToolInvocation, V1_TOOL_NAMES,
};
use minimax_tools::{BuiltinToolPort, NeverCancelled, Preflight, ToolRegistry};
use serde_json::{Value, json};

const SCHEMA_FIXTURE: &str = include_str!("../../../fixtures/compat/tools/v1-schemas.json");
const FULL_ACCESS_SCHEMA_FIXTURE: &str =
    include_str!("../../../fixtures/compat/tools/full-access-schemas.v1.json");
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
fn full_access_schemas_match_the_versioned_ten_tool_fixture() {
    let specs = must(ToolRegistry::specs_for(PermissionMode::FullAccess));
    let names: Vec<_> = specs
        .iter()
        .map(|spec| spec.definition.name.as_str())
        .collect();
    assert_eq!(names, FULL_ACCESS_TOOL_NAMES);
    assert_eq!(specs.len(), 10);

    for name in SHELL_TOOL_NAMES {
        let spec = must_option(
            specs
                .iter()
                .find(|spec| spec.definition.name.as_str() == name),
        );
        assert!(spec.definition.strict);
        assert_eq!(
            spec.definition.parameters["additionalProperties"],
            Value::Bool(false)
        );
    }

    let definitions: Vec<_> = specs.into_iter().map(|spec| spec.definition).collect();
    let actual = json!({"schema_version": 1, "tools": definitions});
    let expected: Value = must(serde_json::from_str(FULL_ACCESS_SCHEMA_FIXTURE));
    assert_eq!(actual, expected);
}

#[test]
fn registry_compatibility_wrappers_are_confirm_safe_and_find_both_shell_tools() {
    let all_names: Vec<_> = must(ToolRegistry::all_specs())
        .into_iter()
        .map(|spec| spec.definition.name)
        .collect();
    assert_eq!(all_names, V1_TOOL_NAMES);

    let confirm_names: Vec<_> = must(BuiltinToolPort::definitions())
        .into_iter()
        .map(|definition| definition.name)
        .collect();
    assert_eq!(confirm_names, V1_TOOL_NAMES);
    let full_access_names: Vec<_> =
        must(BuiltinToolPort::definitions_for(PermissionMode::FullAccess))
            .into_iter()
            .map(|definition| definition.name)
            .collect();
    assert_eq!(full_access_names, FULL_ACCESS_TOOL_NAMES);

    for name in SHELL_TOOL_NAMES {
        let spec = must_option(must(ToolRegistry::find(name)));
        assert_eq!(spec.definition.name, name);
        assert_eq!(spec.effect, ToolEffect::Process);
    }
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
        for mode in modes {
            let permission_mode = match must_option(mode.as_str()) {
                "confirm" => PermissionMode::Confirm,
                "full-access" => PermissionMode::FullAccess,
                unexpected => panic!("unexpected permission mode {unexpected}"),
            };
            let error = match Preflight::check_with_context(
                &invocation,
                ToolExecutionContext::for_permission_mode(permission_mode),
                &NeverCancelled,
            ) {
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
        must_error(Preflight::check_with_context(
            &extra,
            ToolExecutionContext::for_permission_mode(PermissionMode::Confirm),
            &NeverCancelled,
        ))
        .code()
        .as_str(),
        "invalid_arguments"
    );

    let unknown = invocation("delete_file", ToolEffect::Write, json!({}));
    assert_eq!(
        must_error(Preflight::check_with_context(
            &unknown,
            ToolExecutionContext::for_permission_mode(PermissionMode::Confirm),
            &NeverCancelled,
        ))
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
        must_error(Preflight::check_with_context(
            &wrong_type,
            ToolExecutionContext::for_permission_mode(PermissionMode::Confirm),
            &NeverCancelled,
        ))
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
        must_error(Preflight::check_with_context(
            &invocation,
            ToolExecutionContext::for_permission_mode(PermissionMode::Confirm),
            &NeverCancelled,
        ))
        .code()
        .as_str(),
        "secret_content"
    );
    assert_eq!(
        must_error(Preflight::check_with_context(
            &invocation,
            ToolExecutionContext::for_permission_mode(PermissionMode::Confirm),
            &AlwaysCancelled,
        ))
        .code()
        .as_str(),
        "cancelled"
    );
}

struct AlwaysCancelled;

impl CancellationPort for AlwaysCancelled {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn cancelled<'a>(&'a self) -> CancellationFuture<'a> {
        Box::pin(std::future::ready(()))
    }
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
