use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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
fn full_access_agent_run_enables_shell_and_discloses_risk_on_jsonl_stderr() {
    let project = tempfile::tempdir().expect("temporary project");
    let vault = tempfile::tempdir().expect("temporary Vault");
    let (endpoint, requests, server) = provider_fixture_server(vec![
        concat!(
            "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"id\":\"item-shell\",\"call_id\":\"call-shell\",\"name\":\"shell_session\",\"arguments\":\"{\\\"session_id\\\":\\\"shell-fixture-missing\\\",\\\"action\\\":\\\"write\\\",\\\"input\\\":\\\"x\\\"}\"}}\n\n",
            "data: {\"type\":\"response.completed\"}\n\n"
        ),
        concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
            "data: {\"type\":\"response.completed\"}\n\n"
        ),
    ]);

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_minimax-cli"))
        .args([
            "run",
            "--agent",
            "--permission",
            "full-access",
            "--jsonl",
            "--prompt",
            "execute the fixture command",
            "--project",
        ])
        .arg(project.path())
        .arg("--vault")
        .arg(vault.path())
        .args([
            "--project-id",
            "project-full-access-entry",
            "--provider-id",
            "provider-fixture",
            "--endpoint",
            &endpoint,
            "--protocol",
            "responses",
            "--model",
            "model-fixture",
            "--environment-key",
            "MINIMAX_ENTRY_TEST_KEY",
            "--allow-insecure-loopback",
            "--timeout-ms",
            "5000",
        ])
        .env("MINIMAX_ENTRY_TEST_KEY", "fixture-key")
        .output()
        .expect("full-access agent run");
    let stdout = String::from_utf8(output.stdout).expect("stdout UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr UTF-8");
    let server_result = server.join();
    assert!(
        server_result.is_ok(),
        "fixture server failed\nstatus={:?}\nstdout={stdout}\nstderr={stderr}",
        output.status.code()
    );
    assert!(
        output.status.success(),
        "status={:?}\nstdout={stdout}\nstderr={stderr}",
        output.status.code()
    );
    let expected_disclosure = permission_status(
        PermissionMode::FullAccess,
        SandboxCapability::detect(project.path()),
    );
    assert!(stderr.contains(&expected_disclosure), "{stderr}");
    assert!(!stdout.contains(&expected_disclosure), "{stdout}");
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|error| panic!("non-JSON stdout line {line:?}: {error}"));
    }

    let requests = requests.lock().expect("fixture requests");
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains("shell_session"), "{}", requests[0]);
    assert!(
        requests[1].contains("shell_session_not_found"),
        "{}",
        requests[1]
    );
    assert!(!requests[1].contains("shell_requires_full_access"));
}

#[test]
fn provider_fixture_waits_for_delayed_request_bytes() {
    let (endpoint, requests, server) = provider_fixture_server(vec!["fixture-response"]);
    let address = endpoint
        .strip_prefix("http://")
        .expect("loopback fixture endpoint");
    let mut client = TcpStream::connect(address).expect("connect fixture");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("client read timeout");

    std::thread::sleep(Duration::from_millis(100));
    let write_result = write!(
        client,
        "POST /v1/responses HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\n\r\n"
    )
    .and_then(|()| client.flush());
    let mut response = String::new();
    let read_result = write_result
        .as_ref()
        .map_err(std::io::Error::kind)
        .and_then(|()| {
            client
                .read_to_string(&mut response)
                .map_err(|error| error.kind())
        });
    let server_result = server.join();

    assert!(
        server_result.is_ok(),
        "fixture server failed; client write={:?}, read={read_result:?}",
        write_result.as_ref().map_err(std::io::Error::kind)
    );
    write_result.expect("write delayed fixture request");
    read_result.expect("read fixture response");
    assert!(response.contains("fixture-response"), "{response}");
    assert_eq!(requests.lock().expect("fixture requests").len(), 1);
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

fn provider_fixture_server(
    responses: Vec<&'static str>,
) -> (String, Arc<Mutex<Vec<String>>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    listener
        .set_nonblocking(true)
        .expect("nonblocking fixture listener");
    let address = listener.local_addr().expect("fixture address");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&requests);
    let server = std::thread::spawn(move || {
        for (response_index, body) in responses.into_iter().enumerate() {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "fixture accept {response_index} timed out"
                        );
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("fixture accept failed: {error}"),
                }
            };
            socket
                .set_nonblocking(false)
                .expect("blocking fixture socket");
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("fixture read timeout");
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("fixture write timeout");
            let request = read_http_request(&mut socket);
            captured.lock().expect("fixture requests").push(request);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("fixture response");
            socket.flush().expect("flush fixture response");
        }
    });
    (format!("http://{address}"), requests, server)
}

fn read_http_request(socket: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut expected_len = None;
    loop {
        let mut chunk = [0_u8; 4096];
        let read = socket.read(&mut chunk).expect("read fixture request");
        assert!(read > 0, "fixture request ended before body was complete");
        request.extend_from_slice(&chunk[..read]);
        if expected_len.is_none()
            && let Some(header_end) = request.windows(4).position(|value| value == b"\r\n\r\n")
        {
            let body_start = header_end + 4;
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                .expect("fixture Content-Length");
            expected_len = Some(body_start + content_length);
        }
        if expected_len.is_some_and(|length| request.len() >= length) {
            return String::from_utf8(request).expect("fixture request UTF-8");
        }
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
