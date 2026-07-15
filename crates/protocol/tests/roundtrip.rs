use minimax_protocol::{
    ProtocolErrorCode, SchemaVersion, SessionId, StreamEvent, StreamEventV1, TerminalOutcome,
    ToolCallFragment, ToolCallId, TurnId, Usage, parse_stream_event_v1,
};

fn round_trip(event: StreamEvent) {
    let envelope = StreamEventV1::new(event);
    let json = serde_json::to_string(&envelope).expect("event should serialize");
    assert!(json.contains("\"schemaVersion\":1"));
    assert!(!json.contains("choices"));
    assert!(!json.contains("response"));
    let decoded = parse_stream_event_v1(&json).expect("event should deserialize");
    assert_eq!(decoded, envelope);
}

#[test]
fn every_public_event_variant_round_trips_through_schema_one() {
    round_trip(StreamEvent::ReasoningFiltered);
    round_trip(StreamEvent::VisibleTextDelta {
        delta: "visible".to_owned(),
    });
    round_trip(StreamEvent::ToolCallFragments {
        fragments: vec![ToolCallFragment {
            call_id: ToolCallId::new("call-1").expect("valid ID"),
            stream_id: None,
            name: Some("read_file".to_owned()),
            arguments_delta: Some("{}".to_owned()),
            arguments_complete: true,
            index: Some(0),
        }],
    });
    round_trip(StreamEvent::Usage {
        usage: Usage {
            input_tokens: Some(7),
            output_tokens: Some(2),
            total_tokens: Some(9),
        },
    });
    round_trip(StreamEvent::Terminal {
        outcome: TerminalOutcome::Completed,
    });
    round_trip(StreamEvent::Terminal {
        outcome: TerminalOutcome::Failed {
            code: ProtocolErrorCode::MalformedJson,
        },
    });
    round_trip(StreamEvent::Terminal {
        outcome: TerminalOutcome::Interrupted,
    });
    round_trip(StreamEvent::Terminal {
        outcome: TerminalOutcome::Stopped,
    });
}

#[test]
fn identifiers_reject_empty_or_whitespace_only_values() {
    assert_eq!(SessionId::new(""), Err(ProtocolErrorCode::MalformedJson));
    assert_eq!(TurnId::new("   "), Err(ProtocolErrorCode::MalformedJson));
    assert_eq!(ToolCallId::new("\n"), Err(ProtocolErrorCode::MalformedJson));

    let empty_id = r#"{"schemaVersion":1,"event":{"type":"tool_call_fragments","fragments":[{"call_id":""}]}}"#;
    assert_eq!(
        parse_stream_event_v1(empty_id),
        Err(ProtocolErrorCode::MalformedJson)
    );
}

#[test]
fn strict_v1_records_reject_unknown_fields_versions_and_event_types() {
    let unknown_field = r#"{"schemaVersion":1,"event":{"type":"visible_text_delta","delta":"ok","raw":"forbidden"}}"#;
    assert_eq!(
        parse_stream_event_v1(unknown_field),
        Err(ProtocolErrorCode::MalformedJson)
    );

    let unknown_version =
        r#"{"schemaVersion":2,"event":{"type":"visible_text_delta","delta":"ok"}}"#;
    assert_eq!(
        parse_stream_event_v1(unknown_version),
        Err(ProtocolErrorCode::MalformedJson)
    );

    let unknown_event = r#"{"schemaVersion":1,"event":{"type":"provider_secret_frame"}}"#;
    assert_eq!(
        parse_stream_event_v1(unknown_event),
        Err(ProtocolErrorCode::UnknownEvent)
    );
}

#[test]
fn schema_version_is_always_one() {
    let encoded = serde_json::to_string(&SchemaVersion).expect("version should serialize");
    assert_eq!(encoded, "1");
    assert!(serde_json::from_str::<SchemaVersion>("2").is_err());
}

#[test]
fn protocol_error_codes_are_typed_and_keep_the_locked_missing_id_spelling() {
    let cases = [
        (ProtocolErrorCode::MalformedJson, "malformed_json"),
        (ProtocolErrorCode::MissingToolCallId, "missing_call_id"),
        (ProtocolErrorCode::PrematureEof, "premature_eof"),
        (ProtocolErrorCode::DuplicateTerminal, "duplicate_terminal"),
        (
            ProtocolErrorCode::EventAfterTerminal,
            "event_after_terminal",
        ),
        (ProtocolErrorCode::UnknownEvent, "unknown_event"),
    ];

    for (code, expected) in cases {
        assert_eq!(
            serde_json::to_string(&code).expect("code should serialize"),
            format!("\"{expected}\"")
        );
    }

    let compatibility_alias = serde_json::from_str::<ProtocolErrorCode>("\"missing_tool_call_id\"")
        .expect("descriptive alias should deserialize");
    assert_eq!(compatibility_alias, ProtocolErrorCode::MissingToolCallId);
}
