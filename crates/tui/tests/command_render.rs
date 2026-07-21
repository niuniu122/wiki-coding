use std::cell::Cell;
use std::collections::BTreeMap;
use std::io;

use minimax_protocol::{
    DiagnosticCode, IndexDomain, IndexStatusRecord, RetrievalDegradedReason, RetrievalExplanation,
    RetrievalHitRecord, RetrievalMode, RetrievalResponse, RuntimeEvent, RuntimeEventV1,
    RuntimeTerminalOutcome, SchemaVersion, ShellReceipt, ShellSessionId, ShellSessionState,
    ToolCall, ToolCallId, ToolEffect, ToolInvocation, ToolResult, ToolTerminalStatus, TraceCode,
    TraceEntry,
};
use minimax_tui::{
    CommandAvailability, CommandIntent, EventRenderer, InteractiveShell, ParsedInput,
    PermissionName, ShellMode, TerminalHooks, parse_input,
};

#[test]
fn parser_covers_every_manifest_command_alias_and_argument_shape() {
    let cases = [
        ("/interrupt", "/interrupt"),
        ("/new", "/new"),
        ("/threads", "/threads"),
        ("/resume session-1", "/resume"),
        ("/compact", "/compact"),
        ("/api", "/api"),
        ("/provider", "/provider"),
        ("/provider local", "/provider"),
        ("/continue", "/continue"),
        ("/agent inspect", "/agent"),
        ("/chat hello", "/chat"),
        ("/models", "/models"),
        ("/model provider:model", "/model"),
        ("/capabilities", "/capabilities"),
        ("/capabilities search files", "/capabilities"),
        ("/permissions", "/permissions"),
        ("/permissions confirm", "/permissions"),
        ("/permissions full-access", "/permissions"),
        ("/trace", "/trace"),
        ("/retry", "/retry"),
        ("/vault status", "/vault"),
        ("/vault gc report", "/vault"),
        ("/exit", "/exit"),
        ("/quit", "/exit"),
    ];
    for (input, canonical) in cases {
        let ParsedInput::Command(intent) = parse_input(input).expect("command") else {
            panic!("command expected for {input}");
        };
        assert_eq!(intent.canonical_name(), canonical);
    }
    assert_eq!(
        parse_input("/continue"),
        Ok(ParsedInput::Command(CommandIntent::AgentContinue))
    );
    assert_eq!(
        parse_input("/retry"),
        Ok(ParsedInput::Command(CommandIntent::RetryInitialization))
    );
    assert_eq!(
        parse_input("ordinary prompt"),
        Ok(ParsedInput::Prompt("ordinary prompt".to_owned()))
    );
    assert_matrix_responsibility(
        "test/chat-input-policy.test.ts",
        "ts-test-chat-input-policy-test-ts",
        "parser_covers_every_manifest_command_alias_and_argument_shape",
    );
}

#[test]
fn parser_rejects_unknown_arguments_and_any_third_permission_mode() {
    for input in [
        "/resume",
        "/agent",
        "/chat",
        "/model",
        "/interrupt now",
        "/capabilities search",
        "/permissions workspace-read",
        "/permissions full_access",
        "/vault",
        "/vault destroy",
        "/unknown",
    ] {
        assert!(parse_input(input).is_err(), "must reject: {input}");
    }
    assert_eq!(
        parse_input("/permissions confirm"),
        Ok(ParsedInput::Command(CommandIntent::Permissions(Some(
            PermissionName::Confirm
        ))))
    );
    for intent in [
        CommandIntent::AgentContinue,
        CommandIntent::AgentSubmit("inspect".to_owned()),
        CommandIntent::Permissions(Some(PermissionName::FullAccess)),
    ] {
        assert_eq!(intent.availability(), CommandAvailability::Available);
    }
    assert_eq!(
        CommandIntent::Capabilities(None).availability(),
        CommandAvailability::Available
    );
}

#[test]
fn retrieval_rendering_exposes_actual_mode_unknown_facts_and_stable_explanations() {
    let status = IndexStatusRecord {
        schema_version: SchemaVersion,
        domain: IndexDomain::Project,
        documents: 6,
        mode: RetrievalMode::Bm25,
        degraded_reason: Some(RetrievalDegradedReason::EmbeddingMissing),
        source: "https://example.test/catalog".into(),
        fingerprint: Some(format!("sha256:{}", "a".repeat(64))),
    };
    let status_text = EventRenderer::index_status(&status);
    assert!(status_text.contains("mode=bm25"));
    assert!(status_text.contains("degraded=embedding_missing"));

    let response = RetrievalResponse {
        schema_version: SchemaVersion,
        domain: IndexDomain::Project,
        query: "fast file search".into(),
        keywords: vec!["file".into(), "search".into()],
        mode: RetrievalMode::Bm25,
        degraded_reason: Some(RetrievalDegradedReason::EmbeddingMissing),
        results: vec![RetrievalHitRecord {
            id: "example/search".into(),
            title: "Search".into(),
            source_url: Some("https://example.test/source".into()),
            repository_url: Some("https://example.test/repo".into()),
            license: None,
            platforms: vec!["windows".into()],
            last_activity: None,
            latest_release: None,
            maintenance: Vec::new(),
            confidence_penalty: 3,
            explanation: RetrievalExplanation {
                matched_terms: vec!["search".into()],
                lexical_rank: 1,
                semantic_rank: None,
                lexical_score: 1.25,
                fused_score: None,
            },
        }],
    };
    let text = EventRenderer::retrieval(&response);
    for fact in [
        "query=fast file search",
        "mode=bm25",
        "degraded=embedding_missing",
        "license=unknown",
        "maintenance=unknown",
        "matched_terms=search",
    ] {
        assert!(text.contains(fact), "missing {fact:?} in {text:?}");
    }
    assert!(!text.contains('\u{1b}'));
}

#[test]
fn approval_and_tool_result_rendering_is_bounded_normalized_and_identified() {
    let invocation = ToolInvocation::new(
        ToolCall::new(
            ToolCallId::new("call-render").expect("call id"),
            "read_file",
            r#"{"path":"docs\\note.md"}"#,
        )
        .expect("call"),
        ToolEffect::Read,
    )
    .expect("invocation");
    let prompt = EventRenderer::approval_request(&invocation);
    assert!(prompt.contains("call=call-render"));
    assert!(prompt.contains("tool=read_file"));
    assert!(prompt.contains("effect=read"));
    assert!(prompt.contains("scope=docs/note.md"));
    assert!(prompt.contains(r#""path":"docs/note.md""#));
    assert!(prompt.contains("Type exactly yes"));

    let rendered = EventRenderer::tool_result(&ToolResult {
        schema_version: SchemaVersion,
        call_id: ToolCallId::new("call-render").expect("call id"),
        tool_name: "read_file".to_owned(),
        status: ToolTerminalStatus::Succeeded,
        code: "ok".to_owned(),
        output: Some("safe\u{1b}[31m output".to_owned()),
    });
    assert!(rendered.contains("call=call-render"));
    assert!(rendered.contains("status=Succeeded"));
    assert!(!rendered.contains('\u{1b}'));
}

#[test]
fn valid_shell_receipts_render_as_readable_terminal_summaries() {
    let running = shell_result(ShellReceipt {
        session_id: ShellSessionId::new("shell-abcd-0001").expect("session ID"),
        state: ShellSessionState::Running,
        exit_code: None,
        output: "server listening on 3000\n".to_owned(),
        output_truncated: false,
    });
    assert_eq!(
        EventRenderer::tool_result(&running),
        "shell | session=shell-abcd-0001 | state=running | exit=none | truncated=false\nserver listening on 3000\n"
    );

    let exited = shell_result(ShellReceipt {
        session_id: ShellSessionId::new("shell-abcd-0002").expect("session ID"),
        state: ShellSessionState::Exited,
        exit_code: Some(7),
        output: "failed\n".to_owned(),
        output_truncated: false,
    });
    assert!(
        EventRenderer::tool_result(&exited)
            .starts_with("shell | session=shell-abcd-0002 | state=exited | exit=7")
    );

    let truncated = shell_result(ShellReceipt {
        session_id: ShellSessionId::new("shell-abcd-0003").expect("session ID"),
        state: ShellSessionState::Running,
        exit_code: None,
        output: "latest output".to_owned(),
        output_truncated: true,
    });
    assert!(EventRenderer::tool_result(&truncated).contains("truncated=true"));
}

#[test]
fn malformed_shell_json_falls_back_to_generic_rendering_and_output_is_bounded() {
    let malformed = ToolResult {
        schema_version: SchemaVersion,
        call_id: ToolCallId::new("call-shell-malformed").expect("call ID"),
        tool_name: "shell_command".to_owned(),
        status: ToolTerminalStatus::Failed,
        code: "shell_launch_failed".to_owned(),
        output: Some("{not-json".to_owned()),
    };
    let fallback = EventRenderer::tool_result(&malformed);
    assert!(fallback.starts_with("tool result | call=call-shell-malformed"));
    assert!(fallback.contains("output={not-json"));

    let bounded = shell_result(ShellReceipt {
        session_id: ShellSessionId::new("shell-abcd-0004").expect("session ID"),
        state: ShellSessionState::Running,
        exit_code: None,
        output: "x".repeat(20_000),
        output_truncated: false,
    });
    assert!(EventRenderer::tool_result(&bounded).chars().count() <= 16_001);
}

fn shell_result(receipt: ShellReceipt) -> ToolResult {
    ToolResult {
        schema_version: SchemaVersion,
        call_id: ToolCallId::new("call-shell-render").expect("call ID"),
        tool_name: "shell_command".to_owned(),
        status: ToolTerminalStatus::Succeeded,
        code: "shell_running".to_owned(),
        output: Some(serde_json::to_string(&receipt).expect("receipt JSON")),
    }
}

#[test]
fn renderer_uses_shared_events_and_removes_terminal_control_sequences() {
    let rendered = EventRenderer::event(&RuntimeEventV1::new(RuntimeEvent::VisibleTextDelta {
        delta: "visible\u{1b}[31m red\rhidden\u{9b}tail".to_owned(),
    }));
    assert!(rendered.contains("visible"));
    assert!(!rendered.contains('\u{1b}'));
    assert!(!rendered.contains('\r'));
    assert!(!rendered.contains('\u{9b}'));
    assert!(rendered.chars().count() <= 16_001);

    assert_eq!(
        EventRenderer::event(&RuntimeEventV1::new(RuntimeEvent::Diagnostic {
            code: DiagnosticCode::NotAvailable,
        })),
        "diagnostic | NotAvailable"
    );
    assert_eq!(
        EventRenderer::event(&RuntimeEventV1::new(RuntimeEvent::Terminal {
            outcome: RuntimeTerminalOutcome::Completed,
        })),
        "terminal | completed"
    );
    assert_eq!(
        EventRenderer::not_available("/agent\u{1b}[2J", 3),
        "/agent�[2J is not available in the Rust development shell until Phase 3"
    );
}

#[test]
fn folded_and_expanded_trace_are_stable_and_safe() {
    let entries = vec![
        TraceEntry {
            recorded_at_unix_ms: 2,
            code: TraceCode::ProviderFailed,
            facts: BTreeMap::from([("kind".to_owned(), "timeout".to_owned())]),
        },
        TraceEntry {
            recorded_at_unix_ms: 1,
            code: TraceCode::TurnStarted,
            facts: BTreeMap::new(),
        },
    ];
    assert_eq!(
        EventRenderer::trace(&entries, false),
        "TurnStarted=1 | ProviderFailed=1"
    );
    assert_eq!(
        EventRenderer::trace(&entries, true),
        "2 | ProviderFailed | kind=timeout\n1 | TurnStarted | "
    );
}

struct Hooks {
    enabled: Cell<u64>,
    disabled: Cell<u64>,
    fail_if_called: bool,
}

impl TerminalHooks for Hooks {
    fn enable_raw_mode(&self) -> io::Result<()> {
        assert!(!self.fail_if_called, "raw mode must not be initialized");
        self.enabled.set(self.enabled.get() + 1);
        Ok(())
    }

    fn disable_raw_mode(&self) -> io::Result<()> {
        assert!(!self.fail_if_called, "raw mode must not be initialized");
        self.disabled.set(self.disabled.get() + 1);
        Ok(())
    }
}

#[test]
fn non_tty_never_enables_raw_mode_and_raw_guard_restores_on_drop() {
    let forbidden = Hooks {
        enabled: Cell::new(0),
        disabled: Cell::new(0),
        fail_if_called: true,
    };
    let line = InteractiveShell::with_capabilities(&forbidden, false, false)
        .begin()
        .expect("line shell");
    assert_eq!(line.mode(), ShellMode::Line);
    assert!(!line.raw_mode_is_guarded());

    let hooks = Hooks {
        enabled: Cell::new(0),
        disabled: Cell::new(0),
        fail_if_called: false,
    };
    {
        let raw = InteractiveShell::with_capabilities(&hooks, true, true)
            .begin()
            .expect("raw shell");
        assert_eq!(raw.mode(), ShellMode::Raw);
        assert!(raw.raw_mode_is_guarded());
        assert_eq!(hooks.enabled.get(), 1);
        assert_eq!(hooks.disabled.get(), 0);
    }
    assert_eq!(hooks.disabled.get(), 1);

    struct UnsupportedHooks;
    impl TerminalHooks for UnsupportedHooks {
        fn enable_raw_mode(&self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::Unsupported, "fixture"))
        }

        fn disable_raw_mode(&self) -> io::Result<()> {
            Ok(())
        }
    }
    let unsupported = UnsupportedHooks;
    let fallback = InteractiveShell::with_capabilities(&unsupported, true, true)
        .begin()
        .expect("unsupported raw mode falls back to line input");
    assert_eq!(fallback.mode(), ShellMode::Line);
}

fn assert_matrix_responsibility(source_path: &str, id: &str, test_name: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root");
    let matrix: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            root.join("fixtures/compat/verification/typescript-responsibilities.v1.json"),
        )
        .expect("coverage matrix"),
    )
    .expect("coverage matrix JSON");
    let source = matrix["sources"]
        .as_array()
        .expect("coverage sources")
        .iter()
        .find(|source| source["sourcePath"] == source_path)
        .expect("historical source");
    assert!(
        source["responsibilities"]
            .as_array()
            .expect("responsibilities")
            .iter()
            .any(|responsibility| responsibility["id"] == id
                && responsibility["evidence"]
                    .as_array()
                    .is_some_and(|evidence| evidence
                        .iter()
                        .any(|item| item["path"] == "crates/tui/tests/command_render.rs"
                            && item["test"] == test_name)))
    );
}
