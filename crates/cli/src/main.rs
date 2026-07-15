use std::collections::BTreeMap;
use std::io::{self, IsTerminal as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser as _;
use minimax_cli::{
    ChatArgs, Cli, CliCommand, CommonArgs, DoctorArgs, DriverIds, ExitClass, HeadlessApprovalPort,
    HttpProviderPort, InteractiveApprovalPort, JsonlWriter, MaintenanceRoute, RunArgs,
    RuntimeDriver, exit_for_error, exit_for_report, inspect,
};
use minimax_core::{CompactionBudget, PermissionMode};
use minimax_protocol::{ModelId, SessionId};
use minimax_provider::{
    CredentialMode, CredentialResolver, HttpProviderClient, OsKeyringBackend, ResolvedConfig,
};
use minimax_tui::{
    CommandAvailability, CommandIntent, CrosstermTerminalHooks, EventRenderer, InteractiveShell,
    ParsedInput, StdioApprovalInput, parse_input,
};

#[tokio::main]
async fn main() -> ExitCode {
    let exit = match Cli::try_parse() {
        Ok(cli) => execute(cli).await,
        Err(error) => {
            let _ = error.print();
            ExitClass::Usage
        }
    };
    ExitCode::from(u8::try_from(exit.code()).unwrap_or(ExitClass::Workspace.code() as u8))
}

async fn execute(cli: Cli) -> ExitClass {
    match cli.command {
        CliCommand::Run(args) => execute_run(args).await,
        CliCommand::Chat(args) => execute_chat(args).await,
        CliCommand::Doctor(args) => execute_doctor(args),
        CliCommand::Migrate => unavailable(MaintenanceRoute::Migrate),
        CliCommand::Vault => unavailable(MaintenanceRoute::Vault),
        CliCommand::Index => unavailable(MaintenanceRoute::Index),
    }
}

async fn execute_run(args: RunArgs) -> ExitClass {
    let Some((config, provider)) = prepare_provider(&args.common, CredentialMode::Headless) else {
        return ExitClass::Usage;
    };
    let driver = if args.agent {
        RuntimeDriver::open_with_builtin_tools(
            &args.common.project,
            config.binding(),
            provider,
            DriverIds::system(),
            Box::new(HeadlessApprovalPort),
        )
    } else {
        RuntimeDriver::open(
            &args.common.project,
            config.binding(),
            provider,
            DriverIds::system(),
        )
    };
    let mut driver = match driver {
        Ok(driver) => driver,
        Err(error) => {
            eprintln!("run failed: {error}");
            return exit_for_error(&error);
        }
    };
    if args.agent {
        driver.set_permission_mode(args.permission.into());
    }
    let report = if args.jsonl {
        let mut writer = JsonlWriter::new(io::stdout().lock());
        let mut output_failed = false;
        let publish = |event: &minimax_protocol::RuntimeEventV1| {
            if !output_failed && writer.write_event(event).is_err() {
                output_failed = true;
            }
        };
        let report = if args.agent {
            run_agent_with_shutdown_with(
                &mut driver,
                args.prompt,
                config.max_output_tokens,
                publish,
            )
            .await
        } else {
            run_with_shutdown_with(&mut driver, args.prompt, config.max_output_tokens, publish)
                .await
        };
        if output_failed {
            eprintln!("run failed: output stream is unavailable");
            return ExitClass::Workspace;
        }
        report
    } else {
        let report = if args.agent {
            run_agent_with_shutdown_with(
                &mut driver,
                args.prompt,
                config.max_output_tokens,
                |event| println!("{}", EventRenderer::event(event)),
            )
            .await
        } else {
            run_with_shutdown_with(
                &mut driver,
                args.prompt,
                config.max_output_tokens,
                |event| println!("{}", EventRenderer::event(event)),
            )
            .await
        };
        if let Ok(report) = &report {
            render_tool_results(report);
        }
        report
    };
    match report {
        Ok(report) => exit_for_report(&report),
        Err(error) => {
            eprintln!("run failed: {error}");
            exit_for_error(&error)
        }
    }
}

async fn execute_chat(args: ChatArgs) -> ExitClass {
    let Some((config, provider)) = prepare_provider(&args.common, CredentialMode::Interactive)
    else {
        return ExitClass::Usage;
    };
    if let Some(prompt) = args.prompt {
        let mut driver = match RuntimeDriver::open(
            &args.common.project,
            config.binding(),
            provider,
            DriverIds::system(),
        ) {
            Ok(driver) => driver,
            Err(error) => {
                eprintln!("chat failed: {error}");
                return exit_for_error(&error);
            }
        };
        return render_chat_turn(&mut driver, prompt, config.max_output_tokens).await;
    }

    let hooks = CrosstermTerminalHooks;
    let shell = InteractiveShell::from_stdio(&hooks);
    let session = match shell.begin() {
        Ok(session) => session,
        Err(_) => {
            eprintln!("chat failed: terminal initialization is unavailable");
            return ExitClass::Workspace;
        }
    };
    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    let approval = InteractiveApprovalPort::new(Box::new(StdioApprovalInput::new(
        session.mode(),
        interactive,
    )));
    let mut driver = match RuntimeDriver::open_with_builtin_tools(
        &args.common.project,
        config.binding(),
        provider,
        DriverIds::system(),
        Box::new(approval),
    ) {
        Ok(driver) => driver,
        Err(error) => {
            eprintln!("chat failed: {error}");
            return exit_for_error(&error);
        }
    };
    println!("MiniMax Codex Rust development shell. Use /exit to leave.");
    let mut trace_expanded = false;
    loop {
        print!("> ");
        if io::stdout().flush().is_err() {
            return ExitClass::Workspace;
        }
        let line = match session.read_line() {
            Ok(Some(line)) => line,
            Ok(None) => return ExitClass::Completed,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                return ExitClass::Interrupted;
            }
            Err(_) => return ExitClass::Workspace,
        };
        let parsed = match parse_input(&line) {
            Ok(parsed) => parsed,
            Err(error) => {
                eprintln!("{error}");
                continue;
            }
        };
        match parsed {
            ParsedInput::Prompt(prompt) => {
                let exit = render_chat_turn(&mut driver, prompt, config.max_output_tokens).await;
                if exit != ExitClass::Completed {
                    return exit;
                }
            }
            ParsedInput::Command(intent) => {
                if let CommandAvailability::NotAvailable { owning_phase } = intent.availability() {
                    println!(
                        "{}",
                        EventRenderer::not_available(intent.canonical_name(), owning_phase)
                    );
                    continue;
                }
                match intent {
                    CommandIntent::Exit => return ExitClass::Completed,
                    CommandIntent::ChatSubmit(prompt) => {
                        let exit =
                            render_chat_turn(&mut driver, prompt, config.max_output_tokens).await;
                        if exit != ExitClass::Completed {
                            return exit;
                        }
                    }
                    CommandIntent::AgentSubmit(prompt) => {
                        let exit =
                            render_agent_turn(&mut driver, prompt, config.max_output_tokens).await;
                        if exit != ExitClass::Completed {
                            return exit;
                        }
                    }
                    CommandIntent::AgentContinue => {
                        let exit = render_agent_turn(
                            &mut driver,
                            "Continue the previous agent task from the durable session context."
                                .to_owned(),
                            config.max_output_tokens,
                        )
                        .await;
                        if exit != ExitClass::Completed {
                            return exit;
                        }
                    }
                    CommandIntent::Permissions(None) => println!(
                        "permission mode: {}",
                        permission_name(driver.permission_mode())
                    ),
                    CommandIntent::Permissions(Some(mode)) => {
                        let mode = match mode {
                            minimax_tui::PermissionName::Confirm => PermissionMode::Confirm,
                            minimax_tui::PermissionName::FullAccess => PermissionMode::FullAccess,
                        };
                        driver.set_permission_mode(mode);
                        println!(
                            "permission mode: {} | applies only to this process; workspace, secret, command, size, timeout, and cancellation gates remain enforced",
                            permission_name(mode)
                        );
                    }
                    CommandIntent::NewSession => match driver.create_session(config.binding()) {
                        Ok(id) => println!("new session: {}", id.as_str()),
                        Err(error) => eprintln!("new session failed: {error}"),
                    },
                    CommandIntent::ListSessions => match driver.list_sessions() {
                        Ok(sessions) => {
                            let rows = sessions
                                .iter()
                                .map(|session| {
                                    (
                                        session.session_id.as_str(),
                                        session.status,
                                        session.updated_at_unix_ms,
                                        session.turn_count,
                                    )
                                })
                                .collect::<Vec<_>>();
                            println!("{}", EventRenderer::sessions(&rows));
                        }
                        Err(error) => eprintln!("session listing failed: {error}"),
                    },
                    CommandIntent::Resume(raw) => match SessionId::new(raw) {
                        Ok(id) => match driver.resume(id) {
                            Ok(()) => println!("session resumed"),
                            Err(error) => eprintln!("session resume failed: {error}"),
                        },
                        Err(_) => eprintln!("session resume failed: invalid session id"),
                    },
                    CommandIntent::Compact => match driver.compact_active(CompactionBudget {
                        max_record_bytes: 256 * 1024,
                        retain_recent_turns: 4,
                    }) {
                        Ok(record) => println!(
                            "compaction stored through turn {}",
                            record.covered_through_turn_id.as_str()
                        ),
                        Err(error) => eprintln!("compaction failed: {error}"),
                    },
                    CommandIntent::Provider(None) => println!(
                        "provider: {} | protocol: {:?}",
                        config.provider_id.as_str(),
                        config.protocol
                    ),
                    CommandIntent::Provider(Some(_)) => println!(
                        "provider switching requires a validated config and a new shell process"
                    ),
                    CommandIntent::ListModels => {
                        println!("selected model: {}", config.model_id.as_str());
                    }
                    CommandIntent::SwitchModel(raw) => match ModelId::new(raw) {
                        Ok(model_id) => {
                            let mut binding = config.binding();
                            binding.model_id = model_id;
                            match driver.create_session(binding) {
                                Ok(id) => println!("model session created: {}", id.as_str()),
                                Err(error) => eprintln!("model switch failed: {error}"),
                            }
                        }
                        Err(_) => eprintln!("model switch failed: invalid model id"),
                    },
                    CommandIntent::ToggleTrace => {
                        trace_expanded = !trace_expanded;
                        println!(
                            "safe trace display: {}",
                            if trace_expanded { "expanded" } else { "folded" }
                        );
                    }
                    CommandIntent::ApiSetup => println!(
                        "credential setup uses the configured environment key or OS keyring; plaintext files are disabled"
                    ),
                    CommandIntent::RetryInitialization => {
                        println!("configuration and runtime initialization are already valid");
                    }
                    CommandIntent::Interrupt => println!(
                        "press Ctrl-C during a turn to persist an interrupted terminal outcome"
                    ),
                    CommandIntent::Capabilities(_) => unreachable!("availability checked above"),
                }
            }
        }
    }
}

async fn render_chat_turn<P: minimax_cli::ProviderPort>(
    driver: &mut RuntimeDriver<P>,
    prompt: String,
    max_output_tokens: u32,
) -> ExitClass {
    match run_with_shutdown_with(driver, prompt, max_output_tokens, |event| {
        println!("{}", EventRenderer::event(event));
    })
    .await
    {
        Ok(report) => exit_for_report(&report),
        Err(error) => {
            eprintln!("turn failed: {error}");
            exit_for_error(&error)
        }
    }
}

async fn render_agent_turn<P: minimax_cli::ProviderPort>(
    driver: &mut RuntimeDriver<P>,
    prompt: String,
    max_output_tokens: u32,
) -> ExitClass {
    match run_agent_with_shutdown_with(driver, prompt, max_output_tokens, |event| {
        println!("{}", EventRenderer::event(event));
    })
    .await
    {
        Ok(report) => {
            render_tool_results(&report);
            exit_for_report(&report)
        }
        Err(error) => {
            eprintln!("agent turn failed: {error}");
            exit_for_error(&error)
        }
    }
}

fn render_tool_results(report: &minimax_cli::RunReport) {
    for result in &report.tool_results {
        println!("{}", EventRenderer::tool_result(result));
    }
}

const fn permission_name(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Confirm => "confirm",
        PermissionMode::FullAccess => "full-access",
    }
}

async fn run_with_shutdown_with<P, F>(
    driver: &mut RuntimeDriver<P>,
    prompt: String,
    max_output_tokens: u32,
    publish: F,
) -> Result<minimax_cli::RunReport, minimax_cli::DriverError>
where
    P: minimax_cli::ProviderPort,
    F: FnMut(&minimax_protocol::RuntimeEventV1),
{
    let cancellation = driver.cancellation_token();
    let run = driver.run_prompt_with(prompt, max_output_tokens, publish);
    tokio::pin!(run);
    wait_for_run_or_signal(&mut run, cancellation).await
}

async fn run_agent_with_shutdown_with<P, F>(
    driver: &mut RuntimeDriver<P>,
    prompt: String,
    max_output_tokens: u32,
    publish: F,
) -> Result<minimax_cli::RunReport, minimax_cli::DriverError>
where
    P: minimax_cli::ProviderPort,
    F: FnMut(&minimax_protocol::RuntimeEventV1),
{
    let cancellation = driver.cancellation_token();
    let run = driver.run_agent_with(prompt, max_output_tokens, publish);
    tokio::pin!(run);
    wait_for_run_or_signal(&mut run, cancellation).await
}

#[cfg(not(target_abi = "llvm"))]
async fn wait_for_run_or_signal<F>(
    run: &mut std::pin::Pin<&mut F>,
    cancellation: tokio_util::sync::CancellationToken,
) -> F::Output
where
    F: std::future::Future,
{
    tokio::select! {
        result = run.as_mut() => result,
        signal = tokio::signal::ctrl_c() => {
            if signal.is_ok() {
                cancellation.cancel();
            }
            run.as_mut().await
        }
    }
}

#[cfg(target_abi = "llvm")]
async fn wait_for_run_or_signal<F>(
    run: &mut std::pin::Pin<&mut F>,
    _cancellation: tokio_util::sync::CancellationToken,
) -> F::Output
where
    F: std::future::Future,
{
    run.await
}

fn execute_doctor(args: DoctorArgs) -> ExitClass {
    let environment = environment();
    let config = resolve_common(&args.common, &environment);
    let keyring = OsKeyringBackend;
    let credential_result = match config.as_ref() {
        Ok(config) => CredentialResolver::new(&environment, Some(&keyring))
            .resolve(config, CredentialMode::Interactive)
            .map(|credential| credential.source()),
        Err(_) => Err(minimax_provider::CredentialError::Missing),
    };
    let terminal_capable = io::stdin().is_terminal() && io::stdout().is_terminal();
    let report = inspect(
        &args.common.project,
        config.as_ref().map_err(|error| *error),
        credential_result,
        terminal_capable,
    );
    if args.json {
        match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(_) => return ExitClass::Workspace,
        }
    } else {
        for check in &report.checks {
            println!("{:?} | {} | {}", check.status, check.name, check.detail);
        }
    }
    if report.healthy {
        ExitClass::Completed
    } else {
        ExitClass::Usage
    }
}

fn prepare_provider(
    common: &CommonArgs,
    mode: CredentialMode,
) -> Option<(ResolvedConfig, HttpProviderPort)> {
    let environment = environment();
    let config = match resolve_common(common, &environment) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("configuration failed: {error}");
            return None;
        }
    };
    let keyring = OsKeyringBackend;
    let resolver = CredentialResolver::new(&environment, Some(&keyring));
    let credential = match resolver.resolve(&config, mode) {
        Ok(credential) => credential,
        Err(error) => {
            eprintln!("credential resolution failed: {error}");
            return None;
        }
    };
    let client = match HttpProviderClient::new(&config.endpoint, Some(config.timeout())) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("provider initialization failed: {error}");
            return None;
        }
    };
    Some((config, HttpProviderPort::new(client, credential)))
}

fn resolve_common(
    common: &CommonArgs,
    environment: &BTreeMap<String, String>,
) -> Result<ResolvedConfig, minimax_protocol::RuntimeErrorCode> {
    let user_path = common
        .user_config
        .clone()
        .unwrap_or_else(default_user_config_path);
    let project_path = common
        .project_config
        .clone()
        .unwrap_or_else(|| common.project.join(".minimax/config.json"));
    minimax_cli::config::resolve_from_files(
        &user_path,
        &project_path,
        environment,
        common.config_layer(),
    )
}

fn default_user_config_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("minimax-codex/config.json")
}

fn environment() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

fn unavailable(route: MaintenanceRoute) -> ExitClass {
    eprintln!("{}", route.not_available());
    ExitClass::Usage
}
