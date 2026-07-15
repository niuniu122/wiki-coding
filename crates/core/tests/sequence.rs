use minimax_core::{FixedClock, FixedIdGenerator, StreamSequence, replay_stream};
use minimax_protocol::{
    ProtocolErrorCode, StreamEvent, TerminalOutcome, ToolCallFragment, ToolCallId, Usage,
};

fn terminal(outcome: TerminalOutcome) -> StreamEvent {
    StreamEvent::Terminal { outcome }
}

#[test]
fn each_terminal_outcome_closes_one_valid_sequence() {
    let outcomes = [
        TerminalOutcome::Completed,
        TerminalOutcome::Failed {
            code: ProtocolErrorCode::MalformedJson,
        },
        TerminalOutcome::Interrupted,
        TerminalOutcome::Stopped,
    ];

    for outcome in outcomes {
        let mut sequence = StreamSequence::new();
        sequence
            .accept(StreamEvent::VisibleTextDelta {
                delta: "visible".to_owned(),
            })
            .expect("data before terminal should be valid");
        sequence
            .accept(terminal(outcome.clone()))
            .expect("first terminal should be valid");
        assert_eq!(sequence.finish_eof(), Ok(&outcome));
    }
}

#[test]
fn illegal_terminal_orderings_return_exact_protocol_codes() {
    let mut no_terminal = StreamSequence::new();
    no_terminal
        .accept(StreamEvent::Usage {
            usage: Usage::default(),
        })
        .expect("nonterminal should be accepted");
    assert_eq!(
        no_terminal.finish_eof(),
        Err(ProtocolErrorCode::PrematureEof)
    );

    let mut duplicate = StreamSequence::new();
    duplicate
        .accept(terminal(TerminalOutcome::Completed))
        .expect("first terminal should be accepted");
    assert_eq!(
        duplicate.accept(terminal(TerminalOutcome::Stopped)),
        Err(ProtocolErrorCode::DuplicateTerminal)
    );

    let mut late_data = StreamSequence::new();
    late_data
        .accept(terminal(TerminalOutcome::Completed))
        .expect("first terminal should be accepted");
    assert_eq!(
        late_data.accept(StreamEvent::ReasoningFiltered),
        Err(ProtocolErrorCode::EventAfterTerminal)
    );
}

#[test]
fn fixed_ports_make_normalized_replay_byte_identical() {
    let events = vec![
        StreamEvent::ReasoningFiltered,
        StreamEvent::VisibleTextDelta {
            delta: "visible".to_owned(),
        },
        StreamEvent::ToolCallFragments {
            fragments: vec![ToolCallFragment {
                call_id: ToolCallId::new("call-1").expect("valid ID"),
                stream_id: None,
                name: Some("read_file".to_owned()),
                arguments_delta: Some("{}".to_owned()),
                arguments_complete: true,
                index: Some(0),
            }],
        },
        terminal(TerminalOutcome::Completed),
    ];
    let clock = FixedClock::new(1_700_000_000_000);
    let ids = FixedIdGenerator::new("fixed");

    let first = replay_stream(events.clone(), &clock, &ids).expect("replay should pass");
    let second = replay_stream(events, &clock, &ids).expect("replay should pass");
    let first_bytes = serde_json::to_vec(&first).expect("record should serialize");
    let second_bytes = serde_json::to_vec(&second).expect("record should serialize");

    assert_eq!(first, second);
    assert_eq!(first_bytes, second_bytes);
    assert_eq!(first.replay_id.as_str(), "replay_fixed");
    assert_eq!(first.recorded_at_unix_ms, 1_700_000_000_000);
}

#[test]
fn core_manifest_has_no_adapter_dependency() {
    let manifest = include_str!("../Cargo.toml");
    for forbidden in [
        "minimax-provider",
        "minimax-tools",
        "minimax-vault",
        "minimax-tui",
        "minimax-cli",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "forbidden dependency: {forbidden}"
        );
    }
}
