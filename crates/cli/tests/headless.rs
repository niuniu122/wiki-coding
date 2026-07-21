use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

use clap::Parser as _;
use minimax_cli::{
    Cli, CliCommand, DriverError, DriverIds, ExitClass, JsonlWriter, MigrateAction, PermissionArg,
    ProviderPort, RuntimeDriver, exit_for_error, exit_for_report, inspect, permission_status,
};
use minimax_core::{PermissionMode, ToolLifecycleError};
use minimax_protocol::{
    ModelBinding, ModelId, ProviderId, ProviderProtocolKind, RuntimeErrorCode, RuntimeEvent,
    RuntimeEventV1, RuntimeFailure, RuntimeTerminalOutcome, StreamEvent, TerminalOutcome,
    TraceCode, Usage, parse_runtime_event_v1,
};
use minimax_provider::{ConfigLayer, CredentialError, CredentialSource, resolve_config};
use minimax_tools::SandboxCapability;
use minimax_tui::{EventRenderer, TerminalHooks};
use minimax_vault::RuntimeStoreError;
use tokio_util::sync::CancellationToken;

struct MockProvider {
    runs: VecDeque<Result<Vec<StreamEvent>, RuntimeFailure>>,
}

impl MockProvider {
    fn completed() -> Self {
        Self {
            runs: VecDeque::from([Ok(vec![
                StreamEvent::VisibleTextDelta {
                    delta: "hello ".to_owned(),
                },
                StreamEvent::VisibleTextDelta {
                    delta: "world".to_owned(),
                },
                StreamEvent::Usage {
                    usage: Usage {
                        input_tokens: Some(4),
                        output_tokens: Some(2),
                        total_tokens: Some(6),
                    },
                },
                StreamEvent::Terminal {
                    outcome: TerminalOutcome::Completed,
                },
            ])]),
        }
    }
}

impl ProviderPort for MockProvider {
    fn rebind(&mut self, _binding: &ModelBinding) {}

    fn stream<'a>(
        &'a mut self,
        _request: &'a minimax_protocol::TurnRequest,
        _cancellation: &'a CancellationToken,
        emit: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<(), RuntimeFailure>> + Send + 'a>> {
        Box::pin(async move {
            let run = self
                .runs
                .pop_front()
                .ok_or_else(|| RuntimeFailure::new(RuntimeErrorCode::ProtocolPrematureEof))?;
            for event in run? {
                emit(event);
                tokio::task::yield_now().await;
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn mock_run_projects_byte_stable_schema_v1_jsonl_without_terminal_hooks() {
    let project = tempfile::tempdir().expect("temporary project");
    let mut driver = RuntimeDriver::open(
        project.path(),
        binding(),
        MockProvider::completed(),
        DriverIds::new("headless", 1_000),
    )
    .expect("driver");
    let mut observed = Vec::new();
    let report = driver
        .run_prompt_with("say hello", 128, |event| observed.push(event.clone()))
        .await
        .expect("run");
    assert_eq!(exit_for_report(&report), ExitClass::Completed);
    assert_eq!(observed, report.events);

    let hooks = BombHooks::default();
    let mut first = JsonlWriter::new(Vec::new());
    first.write_report(&report).expect("first JSONL");
    let first = first.into_inner();
    let mut second = JsonlWriter::new(Vec::new());
    second.write_report(&report).expect("second JSONL");
    let second = second.into_inner();
    assert_eq!(first, second);
    assert_eq!(hooks.calls.load(Ordering::SeqCst), 0);

    let raw = String::from_utf8(first).expect("UTF-8 JSONL");
    let parsed = raw
        .lines()
        .map(|line| parse_runtime_event_v1(line).expect("schema-v1 event"))
        .collect::<Vec<_>>();
    assert_eq!(parsed, report.events);
    assert_eq!(parsed.len(), 5);
    assert_eq!(EventRenderer::event(&parsed[1]), "hello ");
    assert_eq!(EventRenderer::event(&parsed[2]), "world");
    assert!(!raw.contains("schemaVersion\":0"));
}

#[tokio::test]
async fn safe_trace_is_allowlisted_durable_and_excludes_conversation_content() {
    let project = tempfile::tempdir().expect("temporary project");
    let expected;
    {
        let mut driver = RuntimeDriver::open(
            project.path(),
            binding(),
            MockProvider::completed(),
            DriverIds::new("safe-trace", 2_000),
        )
        .expect("driver");
        driver
            .run_prompt("DO_NOT_PERSIST_PROMPT", 128)
            .await
            .expect("run");
        expected = driver.active_trace_entries();
        assert!(
            expected
                .iter()
                .any(|entry| entry.code == TraceCode::TurnStarted)
        );
        assert!(
            expected
                .iter()
                .any(|entry| entry.code == TraceCode::ProviderConnected)
        );
        let serialized = serde_json::to_string(&expected).expect("trace JSON");
        for prohibited in [
            "DO_NOT_PERSIST_PROMPT",
            "hello world",
            "synthetic-secret",
            "raw_frame",
            "tool_body",
            "<think>",
        ] {
            assert!(
                !serialized.contains(prohibited),
                "trace leaked {prohibited}"
            );
        }
    }

    let reopened = RuntimeDriver::open(
        project.path(),
        binding(),
        MockProvider {
            runs: VecDeque::new(),
        },
        DriverIds::new("safe-trace-reopen", 3_000),
    )
    .expect("reopened driver");
    assert_eq!(reopened.active_trace_entries(), expected);
}

#[test]
fn exit_classes_are_exactly_zero_two_three_four_five() {
    assert_eq!(ExitClass::Completed.code(), 0);
    assert_eq!(ExitClass::Usage.code(), 2);
    assert_eq!(ExitClass::Provider.code(), 3);
    assert_eq!(ExitClass::Interrupted.code(), 4);
    assert_eq!(ExitClass::Workspace.code(), 5);
    assert_eq!(
        exit_for_error(&DriverError::Runtime(RuntimeErrorCode::Configuration)),
        ExitClass::Usage
    );
    assert_eq!(
        exit_for_error(&DriverError::Runtime(RuntimeErrorCode::TransportNetwork)),
        ExitClass::Provider
    );
    assert_eq!(
        exit_for_error(&DriverError::Runtime(RuntimeErrorCode::Interrupted)),
        ExitClass::Interrupted
    );
    assert_eq!(
        exit_for_error(&DriverError::Store(RuntimeStoreError::Busy)),
        ExitClass::Workspace
    );
    assert_eq!(
        exit_for_error(&DriverError::ToolLifecycle(ToolLifecycleError {
            code: "shell_stop_indeterminate",
            session_ids: vec!["shell-failed-0001".to_owned()],
        })),
        ExitClass::Workspace
    );
}

#[test]
fn clap_routes_all_phase_two_and_later_maintenance_commands() {
    assert!(matches!(
        Cli::try_parse_from(["minimax-codex-rust", "run", "--jsonl", "--prompt", "hello"])
            .expect("run route")
            .command,
        CliCommand::Run(args) if args.jsonl && args.prompt == "hello"
    ));
    assert!(matches!(
        Cli::try_parse_from([
            "minimax-codex-rust",
            "run",
            "--agent",
            "--permission",
            "full-access",
            "--prompt",
            "inspect",
        ])
        .expect("agent run route")
        .command,
        CliCommand::Run(args)
            if args.agent
                && args.permission == PermissionArg::FullAccess
                && args.prompt == "inspect"
    ));
    assert!(matches!(
        Cli::try_parse_from(["minimax-codex-rust", "chat"])
            .expect("chat route")
            .command,
        CliCommand::Chat(_)
    ));
    assert!(matches!(
        Cli::try_parse_from(["minimax-codex-rust", "doctor"])
            .expect("doctor route")
            .command,
        CliCommand::Doctor(_)
    ));
    let migrate =
        Cli::try_parse_from(["minimax-codex-rust", "migrate", "inventory"]).expect("migrate route");
    assert!(matches!(
        migrate.command,
        CliCommand::Migrate(args)
            if matches!(
                &args.action,
                MigrateAction::Inventory { source, target }
                    if source == std::path::Path::new(".mini-codex")
                        && target == std::path::Path::new(".")
            )
    ));
}

#[test]
fn empty_cli_arguments_select_the_default_chat_route() {
    let project = tempfile::tempdir().expect("temporary project");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_minimax-cli"))
        .current_dir(project.path())
        .env_remove("MINIMAX_API_KEY")
        .output()
        .expect("default chat route");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr UTF-8");
    assert!(stderr.contains("MINIMAX_API_KEY"), "{stderr}");
    assert!(!stderr.contains("Usage:"), "{stderr}");
}

#[test]
fn doctor_is_actionable_and_never_serializes_secret_material() {
    let project = tempfile::tempdir().expect("temporary project");
    let environment = BTreeMap::new();
    let config =
        resolve_config(None, None, &environment, ConfigLayer::default()).expect("default config");
    let report = inspect(
        project.path(),
        Ok(&config),
        Ok(CredentialSource::Environment),
        false,
    );
    let json = serde_json::to_string(&report).expect("doctor JSON");
    assert!(report.healthy);
    assert!(json.contains("runtime_journal"));
    assert!(json.contains("runtime_index"));
    assert!(json.contains("subprocess_sandbox"));
    assert!(json.contains("confirm-mode process"));
    assert!(json.contains("credentialSource\":\"environment"));
    assert!(!json.contains("DO_NOT_PERSIST_SECRET"));
    assert!(!json.contains("api.minimax.io"));
    assert!(
        !project.path().join(".minimax").exists(),
        "doctor must not initialize or repair runtime state"
    );

    let missing = inspect(
        project.path(),
        Ok(&config),
        Err(CredentialError::Missing),
        true,
    );
    assert!(!missing.healthy);
}

#[test]
fn missing_credential_error_names_the_environment_key_and_doctor() {
    let project = tempfile::tempdir().expect("temporary project");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_minimax-cli"))
        .args(["chat", "--prompt", "hello", "--project"])
        .arg(project.path())
        .env_remove("MINIMAX_API_KEY")
        .output()
        .expect("Rust CLI credential guidance");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr UTF-8");
    assert!(stderr.contains("MINIMAX_API_KEY"), "{stderr}");
    assert!(stderr.contains("minimax-codex doctor"), "{stderr}");
    assert!(!stderr.contains("synthetic-secret"));
}

#[test]
fn permission_status_separates_approval_from_subprocess_isolation() {
    let project = tempfile::tempdir().expect("temporary project");
    let capability = SandboxCapability::detect(project.path());
    let confirm = permission_status(PermissionMode::Confirm, capability);
    assert!(confirm.contains("approval: required"));
    assert!(confirm.contains(&format!(
        "subprocess sandbox: {}",
        capability.state().as_str()
    )));
    assert!(confirm.contains(capability.backend()));
    assert!(confirm.contains("arbitrary Shell: disabled"));

    let full_access = permission_status(PermissionMode::FullAccess, capability);
    assert_eq!(
        full_access,
        "permission mode: full-access | approval: skipped | subprocess sandbox: disabled-by-full-access | arbitrary Shell: enabled for this process | commands can access host files, network, and environment credentials; tool output is persisted locally and sent to the configured Provider"
    );
}

#[test]
fn npm_product_entry_uses_only_rust_launcher() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root");
    let package: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("package.json")).expect("package.json"),
    )
    .expect("package JSON");
    let bins = package["bin"].as_object().expect("package bin object");
    assert_eq!(bins.len(), 1);
    assert_eq!(bins["minimax-codex"], "bin/minimax-codex.cjs");
    let scripts = package["scripts"]
        .as_object()
        .expect("package scripts object");
    assert_eq!(
        scripts["test:package"],
        "node --test scripts/release/package-contract.test.mjs"
    );
    for legacy in ["dev", "start", "start:legacy", "build", "test"] {
        assert!(
            !scripts.contains_key(legacy),
            "legacy npm script survived: {legacy}"
        );
    }
    assert!(scripts.values().all(|script| {
        script.as_str().is_none_or(|script| {
            !script.contains("dist/cli.js")
                && !script.contains("minimax-codex-legacy")
                && !script.contains("tsx src/cli.tsx")
        })
    }));
}

#[test]
fn version_flag_reports_the_rust_package_identity_and_succeeds() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_minimax-cli"))
        .arg("--version")
        .output()
        .expect("Rust CLI version command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("version output UTF-8"),
        format!("minimax-codex-rust {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn text_and_jsonl_terminal_outcomes_match_public_exit_contract() {
    for (outcome, expected_text) in [
        (RuntimeTerminalOutcome::Completed, "terminal | completed"),
        (
            RuntimeTerminalOutcome::Interrupted,
            "terminal | interrupted",
        ),
    ] {
        let event = RuntimeEventV1::new(RuntimeEvent::Terminal { outcome });
        let json = serde_json::to_string(&event).expect("terminal JSONL");
        let reparsed = parse_runtime_event_v1(&json).expect("schema-v1 terminal JSONL");
        assert_eq!(reparsed, event);
        assert_eq!(EventRenderer::event(&reparsed), expected_text);
        assert!(!expected_text.contains('\u{1b}'));
    }
    assert_eq!(ExitClass::Completed.code(), 0);
    assert_eq!(ExitClass::Interrupted.code(), 4);
    assert_matrix_responsibility(
        "test/ui-status.test.ts",
        "ts-cli-terminal-output-parity",
        "text_and_jsonl_terminal_outcomes_match_public_exit_contract",
    );
}

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("fixture").expect("provider id"),
        model_id: ModelId::new("fixture-model").expect("model id"),
        protocol: ProviderProtocolKind::Responses,
    }
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
                        .any(|item| item["path"] == "crates/cli/tests/headless.rs"
                            && item["test"] == test_name)))
    );
}

#[derive(Default)]
struct BombHooks {
    calls: AtomicUsize,
}

impl TerminalHooks for BombHooks {
    fn enable_raw_mode(&self) -> std::io::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        panic!("headless JSONL must never initialize raw mode")
    }

    fn disable_raw_mode(&self) -> std::io::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        panic!("headless JSONL must never touch raw mode")
    }
}
