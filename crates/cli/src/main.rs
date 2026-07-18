use std::collections::BTreeMap;
use std::io::{self, IsTerminal as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser as _;
use minimax_cli::{
    CapabilityIndexAction, ChatArgs, Cli, CliCommand, CommonArgs, DoctorArgs, DriverIds, ExitClass,
    ForgetPlanOutput, GcPlanOutput, HeadlessApprovalPort, HttpProviderPort, IndexAction, IndexArgs,
    InteractiveApprovalPort, JsonlWriter, MigrateAction, MigrateArgs, ProjectIndexAction, RunArgs,
    RuntimeDriver, VaultAction, VaultArgs, VaultForgetAction, VaultGcAction, VaultStatusOutput,
    WikiIndexAction, WikiRunReport, WorkspaceIndexAction, apply_migration, augment_agent_prompt,
    build_migration_plan, capability_search, capability_status, capability_workspace_search,
    capability_workspace_status, exit_for_error, exit_for_report, finalize_active_session_wiki,
    inspect, inventory_migration, is_capability_discovery_intent, permission_status,
    project_search, project_status, resolve_project_vault, rollback_migration, verify_migration,
    wiki_search, wiki_status,
};
use minimax_core::{CompactionBudget, PermissionMode, WikiGenerationPort};
use minimax_protocol::{
    ContentHash, EvidenceId, GcId, KnowledgePatch, ModelId, ProjectId, SessionId,
};
use minimax_provider::{
    CredentialMode, CredentialResolver, HttpProviderClient, OsKeyringBackend, ResolvedConfig,
};
use minimax_tools::SandboxCapability;
use minimax_tui::{
    CommandAvailability, CommandIntent, CrosstermTerminalHooks, EventRenderer, InteractiveShell,
    ParsedInput, StdioApprovalInput, parse_input,
};
use minimax_vault::{
    ProjectVault, apply_forget_plan, apply_gc_plan, forget_confirmation, gc_apply_confirmation,
    gc_report, import_inbox_file, lint_vault, plan_forget, purge_gc_plan, read_gc_trash_manifest,
    rebuild_compiled_wiki, repair_vault, undo_gc_plan,
};

#[tokio::main]
async fn main() -> ExitCode {
    let exit = match Cli::try_parse() {
        Ok(cli) => execute(cli).await,
        Err(error) => {
            let exit = if error.exit_code() == 0 {
                ExitClass::Completed
            } else {
                ExitClass::Usage
            };
            let _ = error.print();
            exit
        }
    };
    ExitCode::from(u8::try_from(exit.code()).unwrap_or(ExitClass::Workspace.code() as u8))
}

async fn execute(cli: Cli) -> ExitClass {
    match cli.command {
        CliCommand::Run(args) => execute_run(args).await,
        CliCommand::Chat(args) => execute_chat(args).await,
        CliCommand::Doctor(args) => execute_doctor(args),
        CliCommand::Migrate(args) => execute_migrate(args),
        CliCommand::Vault(args) => execute_vault(args),
        CliCommand::Index(args) => execute_index(args).await,
        CliCommand::ReleaseProbe { hold_ms } => execute_release_probe(hold_ms),
    }
}

fn execute_release_probe(hold_ms: u64) -> ExitClass {
    println!("release-probe-ready:{}", std::process::id());
    if io::stdout().flush().is_err() {
        return ExitClass::Workspace;
    }
    std::thread::sleep(std::time::Duration::from_millis(hold_ms));
    ExitClass::Completed
}

fn execute_migrate(args: MigrateArgs) -> ExitClass {
    let result = match args.action {
        MigrateAction::Inventory { source, target } => inventory_migration(&source, &target)
            .and_then(|value| render_migration(args.json, &value, value.summary())),
        MigrateAction::DryRun { source, target } => build_migration_plan(&source, &target)
            .and_then(|value| render_migration(args.json, &value, value.summary())),
        MigrateAction::Apply { plan, confirmation } => apply_migration(&plan, &confirmation)
            .and_then(|value| render_migration(args.json, &value, value.summary())),
        MigrateAction::Verify { receipt } => verify_migration(&receipt)
            .and_then(|value| render_migration(args.json, &value, value.summary())),
        MigrateAction::Rollback {
            receipt,
            confirmation,
        } => rollback_migration(&receipt, &confirmation)
            .and_then(|value| render_migration(args.json, &value, value.summary())),
    };
    match result {
        Ok(()) => ExitClass::Completed,
        Err(error) => {
            eprintln!("migration failed: {error}");
            if error.is_usage() {
                ExitClass::Usage
            } else {
                ExitClass::Workspace
            }
        }
    }
}

fn render_migration<T: serde::Serialize>(
    json: bool,
    value: &T,
    summary: String,
) -> Result<(), minimax_cli::MigrationError> {
    if json {
        let output = serde_json::to_string_pretty(value)
            .map_err(|_| minimax_cli::MigrationError::Serialization)?;
        println!("{output}");
    } else {
        println!("{summary}");
    }
    Ok(())
}

async fn execute_index(args: IndexArgs) -> ExitClass {
    match args.action {
        IndexAction::Capabilities { action } => match action {
            CapabilityIndexAction::Status => render_index_status(args.jsonl, capability_status()),
            CapabilityIndexAction::Search { query, limit } => {
                render_index_search(args.jsonl, capability_search(&query, limit))
            }
        },
        IndexAction::Projects { action } => match action {
            ProjectIndexAction::Status {
                catalog,
                embedding_resource,
            } => match project_status(catalog.as_deref(), embedding_resource.as_deref()) {
                Ok(status) => render_index_status(args.jsonl, status),
                Err(error) => index_error(error),
            },
            ProjectIndexAction::Search {
                query,
                catalog,
                embedding_resource,
                limit,
            } => match project_search(
                catalog.as_deref(),
                embedding_resource.as_deref(),
                &query,
                limit,
            )
            .await
            {
                Ok(response) => render_index_search(args.jsonl, response),
                Err(error) => index_error(error),
            },
        },
        IndexAction::Workspace { action } => match action {
            WorkspaceIndexAction::Status {
                catalog_root,
                embedding_resource,
            } => match capability_workspace_status(
                catalog_root.as_deref(),
                embedding_resource.as_deref(),
            ) {
                Ok(status) => render_capability_workspace_status(args.jsonl, status),
                Err(error) => index_error(error),
            },
            WorkspaceIndexAction::Search {
                query,
                kind,
                catalog_root,
                inventory,
                embedding_resource,
                limit,
            } => match capability_workspace_search(
                catalog_root.as_deref(),
                inventory.as_deref(),
                embedding_resource.as_deref(),
                &query,
                kind.selected_kind(),
                limit,
            )
            .await
            {
                Ok(response) => render_capability_workspace_search(args.jsonl, response),
                Err(error) => index_error(error),
            },
        },
        IndexAction::Wiki { action } => match action {
            WikiIndexAction::Status {
                project,
                vault,
                project_id,
            } => {
                let Some(project_id) = parse_index_project_id(project_id) else {
                    return ExitClass::Usage;
                };
                match wiki_status(&project, &vault, project_id) {
                    Ok(status) => render_index_status(args.jsonl, status),
                    Err(error) => index_error(error),
                }
            }
            WikiIndexAction::Search {
                query,
                project,
                vault,
                project_id,
                limit,
            } => {
                let Some(project_id) = parse_index_project_id(project_id) else {
                    return ExitClass::Usage;
                };
                match wiki_search(&project, &vault, project_id, &query, limit) {
                    Ok(response) => render_index_search(args.jsonl, response),
                    Err(error) => index_error(error),
                }
            }
        },
    }
}

fn parse_index_project_id(value: String) -> Option<ProjectId> {
    match ProjectId::new(value) {
        Ok(project_id) => Some(project_id),
        Err(_) => {
            eprintln!("index failed: invalid project ID");
            None
        }
    }
}

fn render_index_status(jsonl: bool, status: minimax_protocol::IndexStatusRecord) -> ExitClass {
    let text = EventRenderer::index_status(&status);
    render_index_value(jsonl, &status, text)
}

fn render_index_search(jsonl: bool, response: minimax_protocol::RetrievalResponse) -> ExitClass {
    let text = EventRenderer::retrieval(&response);
    render_index_value(jsonl, &response, text)
}

fn render_capability_workspace_status(
    jsonl: bool,
    status: minimax_protocol::CapabilityWorkspaceStatusRecord,
) -> ExitClass {
    let text = EventRenderer::capability_workspace_status(&status);
    render_index_value(jsonl, &status, text)
}

fn render_capability_workspace_search(
    jsonl: bool,
    response: minimax_protocol::CapabilityWorkspaceResponse,
) -> ExitClass {
    let text = EventRenderer::capability_workspace(&response);
    render_index_value(jsonl, &response, text)
}

fn render_index_value<T: serde::Serialize>(jsonl: bool, value: &T, text: String) -> ExitClass {
    if jsonl {
        let mut writer = JsonlWriter::new(io::stdout().lock());
        if writer.write_json(value).is_err() {
            eprintln!("index failed: output stream is unavailable");
            return ExitClass::Workspace;
        }
    } else {
        println!("{text}");
    }
    ExitClass::Completed
}

fn index_error(error: minimax_cli::IndexError) -> ExitClass {
    eprintln!("index failed: {error}");
    match error {
        minimax_cli::IndexError::Read
        | minimax_cli::IndexError::Catalog(_)
        | minimax_cli::IndexError::CapabilityCatalog(_) => ExitClass::Usage,
        minimax_cli::IndexError::Vault(_) => ExitClass::Workspace,
    }
}

fn execute_vault(args: VaultArgs) -> ExitClass {
    let project_id = match ProjectId::new(args.project_id) {
        Ok(project_id) => project_id,
        Err(error) => {
            eprintln!("vault failed: {error}");
            return ExitClass::Usage;
        }
    };
    let now = unix_time_ms();
    let vault = match ProjectVault::bootstrap(&args.project, &args.vault, project_id, now) {
        Ok(vault) => vault,
        Err(error) => {
            eprintln!("vault failed: {error}");
            return ExitClass::Workspace;
        }
    };
    let result = match args.action {
        VaultAction::Bootstrap => render_maintenance(
            args.jsonl,
            vault.manifest(),
            format!(
                "vault bootstrapped | project={} | root={} | warnings={:?}",
                vault.manifest().project_id.as_str(),
                vault.root().display(),
                vault.warnings()
            ),
        ),
        VaultAction::Status => {
            let output = VaultStatusOutput {
                manifest: vault.manifest().clone(),
                lint: lint_vault(&vault),
            };
            let text = EventRenderer::vault_lint(&output.lint);
            render_maintenance(args.jsonl, &output, text)
        }
        VaultAction::Lint => {
            let report = lint_vault(&vault);
            let text = EventRenderer::vault_lint(&report);
            render_maintenance(args.jsonl, &report, text)
        }
        VaultAction::Repair => match repair_vault(&vault, now) {
            Ok(receipt) => render_maintenance(
                args.jsonl,
                &receipt,
                format!(
                    "vault repair | operation={} | transactions={} | fragments={} | remaining={}",
                    receipt.operation_id,
                    receipt.recovered_transactions.len(),
                    receipt.quarantined_fragments.len(),
                    receipt.remaining_issues.len()
                ),
            ),
            Err(error) => maintenance_error("repair", error),
        },
        VaultAction::Rebuild => match rebuild_compiled_wiki(&vault, now) {
            Ok(receipt) => render_maintenance(
                args.jsonl,
                &receipt,
                format!(
                    "vault rebuild | operation={} | code={} | raw={} | pages={}",
                    receipt.operation_id,
                    receipt.code,
                    receipt.raw_object_count,
                    receipt.page_count
                ),
            ),
            Err(error) => maintenance_error("rebuild", error),
        },
        VaultAction::Import { relative_path } => {
            match import_inbox_file(&vault, &relative_path, now) {
                Ok(receipt) => render_maintenance(
                    args.jsonl,
                    &receipt,
                    format!(
                        "vault import | evidence={} | code={} | bytes={} | source={}",
                        receipt.evidence_id.as_str(),
                        receipt.code,
                        receipt.bytes,
                        receipt.origin_relative_path
                    ),
                ),
                Err(error) => maintenance_error("import", error),
            }
        }
        VaultAction::Gc { action } => execute_vault_gc(&vault, action, args.jsonl, now),
        VaultAction::Forget { action } => execute_vault_forget(&vault, action, args.jsonl, now),
    };
    drop(vault);
    result
}

fn execute_vault_gc(
    vault: &ProjectVault,
    action: VaultGcAction,
    jsonl: bool,
    now: u64,
) -> ExitClass {
    match action {
        VaultGcAction::Report => match gc_report(vault, now) {
            Ok(plan) => {
                let output = GcPlanOutput {
                    confirmation: gc_apply_confirmation(&plan),
                    plan,
                };
                let text = EventRenderer::gc_plan(&output.plan, &output.confirmation);
                render_maintenance(jsonl, &output, text)
            }
            Err(error) => maintenance_error("gc report", error),
        },
        VaultGcAction::Apply { plan, confirmation } => {
            let output = match read_json::<GcPlanOutput>(&plan) {
                Ok(output) => output,
                Err(exit) => return exit,
            };
            match apply_gc_plan(vault, &output.plan, &confirmation, now) {
                Ok(receipt) => render_maintenance(
                    jsonl,
                    &receipt,
                    format!(
                        "gc applied | id={} | objects={} | bytes={}",
                        receipt.gc_id.as_str(),
                        receipt.object_count,
                        receipt.bytes
                    ),
                ),
                Err(error) => maintenance_error("gc apply", error),
            }
        }
        VaultGcAction::Undo { gc_id } => {
            let gc_id = match GcId::new(gc_id) {
                Ok(gc_id) => gc_id,
                Err(_) => return usage_error("gc undo", "invalid GC ID"),
            };
            match undo_gc_plan(vault, &gc_id, now) {
                Ok(receipt) => render_maintenance(
                    jsonl,
                    &receipt,
                    format!(
                        "gc undone | id={} | objects={}",
                        gc_id.as_str(),
                        receipt.object_count
                    ),
                ),
                Err(error) => maintenance_error("gc undo", error),
            }
        }
        VaultGcAction::Purge {
            gc_id,
            confirmation,
        } => {
            let gc_id = match GcId::new(gc_id) {
                Ok(gc_id) => gc_id,
                Err(_) => return usage_error("gc purge", "invalid GC ID"),
            };
            let manifest = match read_gc_trash_manifest(vault, &gc_id) {
                Ok(manifest) => manifest,
                Err(error) => return maintenance_error("gc purge", error),
            };
            if confirmation != minimax_vault::gc_purge_confirmation(&manifest) {
                return maintenance_error(
                    "gc purge",
                    minimax_vault::VaultError::InvalidConfirmation,
                );
            }
            match purge_gc_plan(vault, &gc_id, &confirmation, now) {
                Ok(receipt) => render_maintenance(
                    jsonl,
                    &receipt,
                    format!(
                        "gc purged | id={} | objects={}",
                        gc_id.as_str(),
                        receipt.object_count
                    ),
                ),
                Err(error) => maintenance_error("gc purge", error),
            }
        }
    }
}

fn execute_vault_forget(
    vault: &ProjectVault,
    action: VaultForgetAction,
    jsonl: bool,
    now: u64,
) -> ExitClass {
    match action {
        VaultForgetAction::Plan {
            evidence_id,
            expected_hash,
        } => {
            let evidence_id = match EvidenceId::new(evidence_id) {
                Ok(value) => value,
                Err(_) => return usage_error("forget plan", "invalid evidence ID"),
            };
            let expected_hash = match ContentHash::new(expected_hash) {
                Ok(value) => value,
                Err(_) => return usage_error("forget plan", "invalid evidence hash"),
            };
            match plan_forget(vault, evidence_id, expected_hash, now) {
                Ok(plan) => {
                    let output = ForgetPlanOutput {
                        confirmation: forget_confirmation(&plan),
                        plan,
                    };
                    let text = EventRenderer::forget_plan(&output.plan, &output.confirmation);
                    render_maintenance(jsonl, &output, text)
                }
                Err(error) => maintenance_error("forget plan", error),
            }
        }
        VaultForgetAction::Apply {
            plan,
            patch,
            confirmation,
        } => {
            let output = match read_json::<ForgetPlanOutput>(&plan) {
                Ok(output) => output,
                Err(exit) => return exit,
            };
            let patch = match read_json::<KnowledgePatch>(&patch) {
                Ok(patch) => patch,
                Err(exit) => return exit,
            };
            match apply_forget_plan(vault, &output.plan, &patch, &confirmation, now) {
                Ok(receipt) => render_maintenance(
                    jsonl,
                    &receipt,
                    format!(
                        "evidence forgotten | plan={} | code={} | tombstone={}",
                        receipt.forget_id.as_str(),
                        receipt.code,
                        receipt.tombstone_relative_path
                    ),
                ),
                Err(error) => maintenance_error("forget apply", error),
            }
        }
    }
}

fn render_maintenance<T: serde::Serialize>(jsonl: bool, value: &T, text: String) -> ExitClass {
    if jsonl {
        let mut writer = JsonlWriter::new(io::stdout().lock());
        if writer.write_json(value).is_err() {
            eprintln!("vault failed: output stream is unavailable");
            return ExitClass::Workspace;
        }
    } else {
        println!("{text}");
    }
    ExitClass::Completed
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ExitClass> {
    let bytes = std::fs::read(path).map_err(|_| {
        eprintln!("vault failed: cannot read {}", path.display());
        ExitClass::Usage
    })?;
    serde_json::from_slice(&bytes).map_err(|_| {
        eprintln!("vault failed: invalid JSON in {}", path.display());
        ExitClass::Usage
    })
}

fn maintenance_error(operation: &str, error: minimax_vault::VaultError) -> ExitClass {
    eprintln!("{operation} failed: {error}");
    if error == minimax_vault::VaultError::InvalidConfirmation {
        ExitClass::Usage
    } else {
        ExitClass::Workspace
    }
}

fn usage_error(operation: &str, message: &str) -> ExitClass {
    eprintln!("{operation} failed: {message}");
    ExitClass::Usage
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn prepare_project_vault(common: &CommonArgs) -> Option<minimax_cli::ProjectVaultBinding> {
    match resolve_project_vault(
        &common.project,
        common.vault.as_deref(),
        common.project_id.as_deref(),
        unix_time_ms(),
    ) {
        Ok(resolved) => {
            if resolved.created {
                eprintln!(
                    "Vault bound: {} | project={} | plaintext local files; override only before first binding with --vault and --project-id",
                    resolved.binding.vault_root.display(),
                    resolved.binding.project_id.as_str()
                );
            }
            Some(resolved.binding)
        }
        Err(error) => {
            eprintln!("Vault binding failed: {error}");
            None
        }
    }
}

async fn finalize_active_wiki<P>(
    driver: &RuntimeDriver<P>,
    binding: &minimax_cli::ProjectVaultBinding,
) -> Result<Option<WikiRunReport>, minimax_cli::WikiDriverError>
where
    P: minimax_cli::ProviderPort + WikiGenerationPort,
{
    finalize_active_session_wiki(driver, binding, unix_time_ms()).await
}

fn render_wiki_report(jsonl: bool, report: &WikiRunReport) -> ExitClass {
    if jsonl {
        let mut writer = JsonlWriter::new(io::stdout().lock());
        for event in &report.events {
            if writer.write_json(event).is_err() {
                return ExitClass::Workspace;
            }
        }
        if writer.write_json(&report.receipt).is_err() {
            return ExitClass::Workspace;
        }
    } else {
        println!(
            "Wiki workflow | outcome={:?} | code={} | model={}/{} | usage={:?}",
            report.receipt.outcome,
            report.receipt.code,
            report.receipt.model_binding.provider_id.as_str(),
            report.receipt.model_binding.model_id.as_str(),
            report.receipt.usage
        );
    }
    ExitClass::Completed
}

async fn finish_chat_session<P>(
    driver: &RuntimeDriver<P>,
    binding: &minimax_cli::ProjectVaultBinding,
    base_exit: ExitClass,
) -> ExitClass
where
    P: minimax_cli::ProviderPort + WikiGenerationPort,
{
    match finalize_active_wiki(driver, binding).await {
        Ok(Some(report)) => {
            if render_wiki_report(false, &report) == ExitClass::Completed {
                base_exit
            } else {
                ExitClass::Workspace
            }
        }
        Ok(None) => base_exit,
        Err(error) => {
            eprintln!("Wiki workflow failed: {error}");
            ExitClass::Workspace
        }
    }
}

async fn execute_run(args: RunArgs) -> ExitClass {
    let Some((config, provider)) = prepare_provider(&args.common, CredentialMode::Headless) else {
        return ExitClass::Usage;
    };
    let Some(vault_binding) = prepare_project_vault(&args.common) else {
        return ExitClass::Workspace;
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
    let prompt = if args.agent {
        match augment_agent_prompt(
            args.common.capability_root.as_deref(),
            args.common.capability_inventory.as_deref(),
            args.common.embedding_resource.as_deref(),
            args.prompt,
        )
        .await
        {
            Ok(prompt) => prompt,
            Err(error) => {
                eprintln!("project discovery failed: {error}");
                return ExitClass::Workspace;
            }
        }
    } else {
        args.prompt
    };
    let report = if args.jsonl {
        let mut writer = JsonlWriter::new(io::stdout().lock());
        let mut output_failed = false;
        let publish = |event: &minimax_protocol::RuntimeEventV1| {
            if !output_failed && writer.write_event(event).is_err() {
                output_failed = true;
            }
        };
        let report = if args.agent {
            run_agent_with_shutdown_with(&mut driver, prompt, config.max_output_tokens, publish)
                .await
        } else {
            run_with_shutdown_with(&mut driver, prompt, config.max_output_tokens, publish).await
        };
        if output_failed {
            eprintln!("run failed: output stream is unavailable");
            return ExitClass::Workspace;
        }
        report
    } else {
        let report = if args.agent {
            run_agent_with_shutdown_with(&mut driver, prompt, config.max_output_tokens, |event| {
                println!("{}", EventRenderer::event(event))
            })
            .await
        } else {
            run_with_shutdown_with(&mut driver, prompt, config.max_output_tokens, |event| {
                println!("{}", EventRenderer::event(event))
            })
            .await
        };
        if let Ok(report) = &report {
            render_tool_results(report);
        }
        report
    };
    match report {
        Ok(report) => {
            let run_exit = exit_for_report(&report);
            match finalize_active_wiki(&driver, &vault_binding).await {
                Ok(Some(wiki)) => {
                    if render_wiki_report(args.jsonl, &wiki) == ExitClass::Completed {
                        run_exit
                    } else {
                        ExitClass::Workspace
                    }
                }
                Ok(None) => run_exit,
                Err(error) => {
                    eprintln!("Wiki workflow failed: {error}");
                    ExitClass::Workspace
                }
            }
        }
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
    let Some(vault_binding) = prepare_project_vault(&args.common) else {
        return ExitClass::Workspace;
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
        let exit = render_chat_turn(&mut driver, prompt, config.max_output_tokens).await;
        return finish_chat_session(&driver, &vault_binding, exit).await;
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
    println!("MiniMax Codex Rust shell. Use /exit to leave.");
    let mut trace_expanded = false;
    loop {
        print!("> ");
        if io::stdout().flush().is_err() {
            return ExitClass::Workspace;
        }
        let line = match session.read_line() {
            Ok(Some(line)) => line,
            Ok(None) => {
                return finish_chat_session(&driver, &vault_binding, ExitClass::Completed).await;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                return finish_chat_session(&driver, &vault_binding, ExitClass::Interrupted).await;
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
                    return finish_chat_session(&driver, &vault_binding, exit).await;
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
                    CommandIntent::Exit => {
                        return finish_chat_session(&driver, &vault_binding, ExitClass::Completed)
                            .await;
                    }
                    CommandIntent::ChatSubmit(prompt) => {
                        let exit =
                            render_chat_turn(&mut driver, prompt, config.max_output_tokens).await;
                        if exit != ExitClass::Completed {
                            return finish_chat_session(&driver, &vault_binding, exit).await;
                        }
                    }
                    CommandIntent::AgentSubmit(prompt) => {
                        let prompt = match augment_agent_prompt(
                            args.common.capability_root.as_deref(),
                            args.common.capability_inventory.as_deref(),
                            args.common.embedding_resource.as_deref(),
                            prompt,
                        )
                        .await
                        {
                            Ok(prompt) => prompt,
                            Err(error) => {
                                eprintln!("project discovery failed: {error}");
                                continue;
                            }
                        };
                        let exit =
                            render_agent_turn(&mut driver, prompt, config.max_output_tokens).await;
                        if exit != ExitClass::Completed {
                            return finish_chat_session(&driver, &vault_binding, exit).await;
                        }
                    }
                    CommandIntent::AgentContinue => {
                        let prompt =
                            "Continue the previous agent task from the durable session context."
                                .to_owned();
                        let exit =
                            render_agent_turn(&mut driver, prompt, config.max_output_tokens).await;
                        if exit != ExitClass::Completed {
                            return finish_chat_session(&driver, &vault_binding, exit).await;
                        }
                    }
                    CommandIntent::Permissions(None) => println!(
                        "{}",
                        permission_status(
                            driver.permission_mode(),
                            SandboxCapability::detect(&args.common.project),
                        )
                    ),
                    CommandIntent::Permissions(Some(mode)) => {
                        let mode = match mode {
                            minimax_tui::PermissionName::Confirm => PermissionMode::Confirm,
                            minimax_tui::PermissionName::FullAccess => PermissionMode::FullAccess,
                        };
                        driver.set_permission_mode(mode);
                        println!(
                            "{}",
                            permission_status(
                                mode,
                                SandboxCapability::detect(&args.common.project),
                            )
                        );
                    }
                    CommandIntent::NewSession => {
                        match finalize_active_wiki(&driver, &vault_binding).await {
                            Ok(Some(wiki)) => {
                                let _ = render_wiki_report(false, &wiki);
                            }
                            Ok(None) => {}
                            Err(error) => {
                                eprintln!("new session failed: Wiki workflow failed: {error}");
                                continue;
                            }
                        }
                        match driver.create_session(config.binding()) {
                            Ok(id) => println!("new session: {}", id.as_str()),
                            Err(error) => eprintln!("new session failed: {error}"),
                        }
                    }
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
                        if let Some(turn_id) = driver.latest_retryable_turn_id() {
                            match driver.retry_turn(turn_id, config.max_output_tokens).await {
                                Ok(report) => {
                                    for event in &report.events {
                                        println!("{}", EventRenderer::event(event));
                                    }
                                    render_tool_results(&report);
                                }
                                Err(error) => eprintln!("retry failed: {error}"),
                            }
                        } else {
                            println!(
                                "configuration and runtime initialization are valid; no terminal turn is available to retry"
                            );
                        }
                    }
                    CommandIntent::Vault(command) => println!(
                        "vault maintenance is process-isolated; run the top-level command: minimax-codex-rust vault --project <project> --vault <vault> --project-id <id> {command}"
                    ),
                    CommandIntent::Interrupt => println!(
                        "press Ctrl-C during a turn to persist an interrupted terminal outcome"
                    ),
                    CommandIntent::Capabilities(query) => match query {
                        Some(query) => {
                            println!(
                                "{}",
                                EventRenderer::retrieval(&capability_search(&query, 5))
                            );
                            if is_capability_discovery_intent(&query) {
                                match capability_workspace_search(
                                    args.common.capability_root.as_deref(),
                                    args.common.capability_inventory.as_deref(),
                                    args.common.embedding_resource.as_deref(),
                                    &query,
                                    None,
                                    5,
                                )
                                .await
                                {
                                    Ok(response) => {
                                        println!(
                                            "{}",
                                            EventRenderer::capability_workspace(&response)
                                        );
                                    }
                                    Err(error) => {
                                        eprintln!("capability discovery failed: {error}");
                                    }
                                }
                            }
                        }
                        None => println!("{}", EventRenderer::index_status(&capability_status())),
                    },
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
    let binding = config.binding();
    Some((config, HttpProviderPort::new(client, credential, binding)))
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
