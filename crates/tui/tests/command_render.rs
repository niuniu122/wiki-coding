use std::cell::Cell;
use std::collections::BTreeMap;
use std::io;

use minimax_protocol::{
    DiagnosticCode, RuntimeEvent, RuntimeEventV1, RuntimeTerminalOutcome, SchemaVersion, ToolCall,
    ToolCallId, ToolEffect, ToolInvocation, ToolResult, ToolTerminalStatus, TraceCode, TraceEntry,
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
        parse_input("ordinary prompt"),
        Ok(ParsedInput::Prompt("ordinary prompt".to_owned()))
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
        CommandAvailability::NotAvailable { owning_phase: 5 }
    );
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
