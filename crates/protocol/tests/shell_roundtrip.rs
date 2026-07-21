use minimax_protocol::{
    MAX_SHELL_OUTPUT_BYTES, MAX_SHELL_SESSION_ID_BYTES, MAX_TOOL_RESULT_BYTES, ShellReceipt,
    ShellSessionId, ShellSessionState, ToolValidationError,
};
use serde_json::json;

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    result.expect("operation succeeds")
}

#[test]
fn shell_receipt_round_trips_with_exact_shape() {
    let receipt = ShellReceipt::new(
        must(ShellSessionId::new("shell-abcd-0001")),
        ShellSessionState::Running,
        None,
        "ready\n".to_owned(),
        false,
    )
    .expect("valid receipt");
    let value = must(serde_json::to_value(&receipt));
    assert_eq!(
        value,
        json!({
            "session_id": "shell-abcd-0001",
            "state": "running",
            "output": "ready\n",
            "output_truncated": false
        })
    );
    assert_eq!(must(serde_json::from_value::<ShellReceipt>(value)), receipt);
}

#[test]
fn shell_states_use_the_exact_snake_case_values() {
    for (state, expected) in [
        (ShellSessionState::Running, "running"),
        (ShellSessionState::Exited, "exited"),
        (ShellSessionState::Stopped, "stopped"),
        (ShellSessionState::Failed, "failed"),
    ] {
        assert_eq!(must(serde_json::to_value(state)), json!(expected));
    }

    let receipt = ShellReceipt::new(
        must(ShellSessionId::new("shell-abcd-0002")),
        ShellSessionState::Exited,
        Some(17),
        String::new(),
        true,
    )
    .expect("valid terminal receipt");
    assert_eq!(
        must(serde_json::to_value(receipt)),
        json!({
            "session_id": "shell-abcd-0002",
            "state": "exited",
            "exit_code": 17,
            "output": "",
            "output_truncated": true
        })
    );
}

#[test]
fn shell_session_ids_reject_invalid_wire_values() {
    for invalid in [
        "",
        "shell-",
        "session-abcd-0001",
        "SHELL-abcd-0001",
        "shell-under_score",
        "shell-with space",
        "shell-with/slash",
        "shell-你好",
    ] {
        assert!(
            ShellSessionId::new(invalid).is_err(),
            "accepted invalid ID: {invalid:?}"
        );
        assert!(
            serde_json::from_value::<ShellSessionId>(json!(invalid)).is_err(),
            "deserialized invalid ID: {invalid:?}"
        );
    }

    let oversized = format!("shell-{}", "a".repeat(MAX_SHELL_SESSION_ID_BYTES));
    assert_eq!(
        ShellSessionId::new(oversized),
        Err(ToolValidationError::InvalidShellReceipt)
    );
    assert_eq!(
        must(ShellSessionId::new("shell-Az09-valid".to_owned())).as_str(),
        "shell-Az09-valid"
    );
}

#[test]
fn receipt_rejects_unknown_fields_and_invalid_session_ids() {
    let valid = json!({
        "session_id": "shell-abcd-0001",
        "state": "running",
        "output": "ready\n",
        "output_truncated": false
    });
    let mut unknown = valid.clone();
    unknown
        .as_object_mut()
        .expect("object")
        .insert("unexpected".to_owned(), json!(true));
    assert!(serde_json::from_value::<ShellReceipt>(unknown).is_err());

    let mut invalid_id = valid;
    invalid_id["session_id"] = json!("not-a-shell-id");
    assert!(serde_json::from_value::<ShellReceipt>(invalid_id).is_err());
}

#[test]
fn receipt_rejects_nul_and_output_above_the_shell_limit() {
    let session_id = must(ShellSessionId::new("shell-abcd-0003"));
    assert_eq!(
        ShellReceipt::new(
            session_id.clone(),
            ShellSessionState::Running,
            None,
            "bad\0output".to_owned(),
            false,
        ),
        Err(ToolValidationError::InvalidShellReceipt)
    );
    assert_eq!(
        ShellReceipt::new(
            session_id.clone(),
            ShellSessionState::Running,
            None,
            "x".repeat(MAX_SHELL_OUTPUT_BYTES + 1),
            false,
        ),
        Err(ToolValidationError::InvalidShellReceipt)
    );

    for output in [
        "bad\0output".to_owned(),
        "x".repeat(MAX_SHELL_OUTPUT_BYTES + 1),
    ] {
        assert!(
            serde_json::from_value::<ShellReceipt>(json!({
                "session_id": "shell-abcd-0003",
                "state": "running",
                "output": output,
                "output_truncated": false
            }))
            .is_err()
        );
    }

    let largest = ShellReceipt::new(
        session_id,
        ShellSessionState::Failed,
        Some(i32::MIN),
        "x".repeat(MAX_SHELL_OUTPUT_BYTES),
        true,
    )
    .expect("maximum Shell output remains valid");
    assert!(must(serde_json::to_vec(&largest)).len() <= MAX_TOOL_RESULT_BYTES);
}
