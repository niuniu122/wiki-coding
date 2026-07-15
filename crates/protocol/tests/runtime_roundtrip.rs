use minimax_protocol::{
    DiagnosticCode, MessageRole, ModelId, ModelMessage, OutputSettings, ProviderId,
    ProviderProtocolKind, RequestId, RuntimeErrorCode, RuntimeEvent, RuntimeEventV1,
    RuntimeFailure, RuntimeTerminalOutcome, SessionId, ToolCallId, TurnId, TurnReceipt,
    TurnRequest, Usage, parse_runtime_event_v1,
};

fn runtime_round_trip(event: RuntimeEvent) {
    let envelope = RuntimeEventV1::new(event);
    let json = serde_json::to_string(&envelope).expect("event should serialize");
    let decoded = parse_runtime_event_v1(&json).expect("event should deserialize");
    assert_eq!(decoded, envelope);
}

fn session_id() -> SessionId {
    SessionId::new("session-1").expect("valid session ID")
}

fn turn_id() -> TurnId {
    TurnId::new("turn-1").expect("valid turn ID")
}

fn request_id() -> RequestId {
    RequestId::new("request-1").expect("valid request ID")
}

#[test]
fn every_runtime_event_variant_round_trips_at_schema_one() {
    runtime_round_trip(RuntimeEvent::TurnStarted {
        session_id: session_id(),
        turn_id: turn_id(),
        request_id: request_id(),
    });
    runtime_round_trip(RuntimeEvent::VisibleTextDelta {
        delta: "visible".to_owned(),
    });
    runtime_round_trip(RuntimeEvent::ReasoningFiltered);
    runtime_round_trip(RuntimeEvent::ToolCallObserved {
        call_id: ToolCallId::new("call-1").expect("valid call ID"),
        name: Some("read_file".to_owned()),
    });
    runtime_round_trip(RuntimeEvent::Usage {
        usage: Usage {
            input_tokens: Some(3),
            output_tokens: Some(2),
            total_tokens: Some(5),
        },
    });
    runtime_round_trip(RuntimeEvent::Diagnostic {
        code: DiagnosticCode::ProviderConnected,
    });
    runtime_round_trip(RuntimeEvent::Terminal {
        outcome: RuntimeTerminalOutcome::Completed,
    });
    runtime_round_trip(RuntimeEvent::Terminal {
        outcome: RuntimeTerminalOutcome::Failed {
            failure: RuntimeFailure::http(429).expect("valid HTTP status"),
        },
    });
    runtime_round_trip(RuntimeEvent::Terminal {
        outcome: RuntimeTerminalOutcome::Interrupted,
    });
    runtime_round_trip(RuntimeEvent::Terminal {
        outcome: RuntimeTerminalOutcome::Stopped,
    });
}

#[test]
fn request_and_receipt_are_strict_and_bounded() {
    let request = TurnRequest {
        session_id: session_id(),
        turn_id: turn_id(),
        request_id: request_id(),
        provider_id: ProviderId::new("provider:minimax/official").expect("valid provider"),
        model_id: ModelId::new("MiniMax-M2").expect("valid model"),
        protocol: ProviderProtocolKind::Responses,
        messages: vec![ModelMessage {
            role: MessageRole::User,
            content: "hello".to_owned(),
        }],
        output: OutputSettings::new(512).expect("valid output settings"),
    }
    .validate()
    .expect("valid request");
    let encoded = serde_json::to_string(&request).expect("request should serialize");
    assert_eq!(
        serde_json::from_str::<TurnRequest>(&encoded)
            .expect("request should deserialize")
            .validate(),
        Ok(request.clone())
    );

    let receipt = TurnReceipt {
        session_id: request.session_id.clone(),
        turn_id: request.turn_id.clone(),
        request_id: request.request_id.clone(),
        outcome: RuntimeTerminalOutcome::Completed,
        usage: None,
    };
    let encoded = serde_json::to_string(&receipt).expect("receipt should serialize");
    assert_eq!(
        serde_json::from_str::<TurnReceipt>(&encoded).expect("receipt should deserialize"),
        receipt
    );

    assert!(OutputSettings::new(0).is_err());
    assert!(OutputSettings::new(OutputSettings::MAX_OUTPUT_TOKENS + 1).is_err());
    assert!(
        TurnRequest {
            messages: Vec::new(),
            ..request
        }
        .validate()
        .is_err()
    );
}

#[test]
fn runtime_contract_rejects_unknown_fields_versions_events_and_ids() {
    let unknown_field = r#"{"schemaVersion":1,"event":{"type":"visible_text_delta","delta":"ok","raw":"forbidden"}}"#;
    assert_eq!(
        parse_runtime_event_v1(unknown_field),
        Err(RuntimeErrorCode::ProtocolMalformedJson)
    );
    let unknown_version =
        r#"{"schemaVersion":2,"event":{"type":"visible_text_delta","delta":"ok"}}"#;
    assert_eq!(
        parse_runtime_event_v1(unknown_version),
        Err(RuntimeErrorCode::ProtocolMalformedJson)
    );
    let unknown_event = r#"{"schemaVersion":1,"event":{"type":"raw_provider_frame"}}"#;
    assert_eq!(
        parse_runtime_event_v1(unknown_event),
        Err(RuntimeErrorCode::ProtocolUnknownEvent)
    );
    let empty_id = r#"{"schemaVersion":1,"event":{"type":"turn_started","session_id":"","turn_id":"turn-1","request_id":"request-1"}}"#;
    assert_eq!(
        parse_runtime_event_v1(empty_id),
        Err(RuntimeErrorCode::ProtocolMalformedJson)
    );
    assert!(ProviderId::new(" ").is_err());
    assert!(ModelId::new("\n").is_err());
    assert!(RequestId::new("").is_err());
}

#[test]
fn failures_expose_only_fixed_codes_and_allowlisted_status() {
    let marker = "sk-secret private chain of thought raw-provider-frame";
    let failure = RuntimeFailure::http(503).expect("valid status");
    let json = serde_json::to_string(&failure).expect("failure should serialize");
    let display = failure.to_string();
    assert_eq!(json, r#"{"code":"http_status","http_status":503}"#);
    assert_eq!(display, "http_status:503");
    assert!(!json.contains(marker));
    assert!(!display.contains(marker));
    assert!(RuntimeFailure::http(42).is_err());
}
