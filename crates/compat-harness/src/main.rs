use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use minimax_compat_harness::{
    build_report, load_cargo_architecture, load_compat_manifests, report_json, repository_root,
    validate_architecture, validate_cli_tui_markdown_boundary, validate_core_source_boundary,
    validate_product_entry, validate_report, validate_rust_command_surface,
    validate_rust_tool_evidence, validate_rust_vault_evidence, validate_vault_source_boundary,
};
use minimax_protocol::{ProtocolErrorCode, ProviderProtocolKind, StreamEvent};
use minimax_provider::{CompatibilityEvent, replay_fixture};
use serde::Deserialize;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(Some(output)) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("compat verification failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<Option<String>, String> {
    let root = repository_root();
    match arguments.as_slice() {
        [command] if command == "verify" => {
            verify_repository(&root)?;
            Ok(None)
        }
        [command, format_flag, format]
            if command == "report" && format_flag == "--format" && format == "json" =>
        {
            let manifests = load_compat_manifests(&root).map_err(|error| error.to_string())?;
            let report = build_report(&manifests);
            validate_report(&report, &root).map_err(|error| error.to_string())?;
            report_json(&report)
                .map(Some)
                .map_err(|error| error.to_string())
        }
        _ => Err("usage: minimax-compat-harness <verify|report --format json>".to_owned()),
    }
}

fn verify_repository(root: &Path) -> Result<(), String> {
    let first_manifests = load_compat_manifests(root).map_err(|error| error.to_string())?;
    validate_rust_command_surface(&first_manifests.commands).map_err(|error| error.to_string())?;
    validate_rust_tool_evidence(root, &first_manifests.baseline)
        .map_err(|error| error.to_string())?;
    validate_rust_vault_evidence(root).map_err(|error| error.to_string())?;
    validate_product_entry(root).map_err(|error| error.to_string())?;
    verify_provider_fixtures(root)?;
    let architecture = load_cargo_architecture(root).map_err(|error| error.to_string())?;
    validate_architecture(&architecture).map_err(|error| error.to_string())?;
    validate_core_source_boundary(root).map_err(|error| error.to_string())?;
    validate_vault_source_boundary(root).map_err(|error| error.to_string())?;
    validate_cli_tui_markdown_boundary(root).map_err(|error| error.to_string())?;

    let first_report = build_report(&first_manifests);
    validate_report(&first_report, root).map_err(|error| error.to_string())?;
    let first_json = report_json(&first_report).map_err(|error| error.to_string())?;
    let second_manifests = load_compat_manifests(root).map_err(|error| error.to_string())?;
    let second_report = build_report(&second_manifests);
    validate_report(&second_report, root).map_err(|error| error.to_string())?;
    let second_json = report_json(&second_report).map_err(|error| error.to_string())?;
    if first_json != second_json {
        return Err("compatibility report is not deterministic".to_owned());
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidCase {
    case_id: String,
    protocol: ProviderProtocolKind,
    raw: Vec<String>,
    expected_events: Vec<CompatibilityEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InvalidFixture {
    schema_version: u16,
    cases: Vec<InvalidCase>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvalidCase {
    case_id: String,
    protocol: ProviderProtocolKind,
    raw: Vec<String>,
    expected_error: ExpectedError,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedError {
    code: ProtocolErrorCode,
    category: String,
    forbidden_fragments: Vec<String>,
}

fn verify_provider_fixtures(root: &Path) -> Result<(), String> {
    let fixture_root = root.join("fixtures/compat/provider-streams");
    for name in ["responses.valid.jsonl", "chat-completions.valid.jsonl"] {
        let contents = fs::read_to_string(fixture_root.join(name))
            .map_err(|_| format!("cannot read provider fixture: {name}"))?;
        for line in contents.lines().filter(|line| !line.trim().is_empty()) {
            let case: ValidCase = serde_json::from_str(line)
                .map_err(|_| format!("invalid provider fixture JSON: {name}"))?;
            let replay = replay_fixture(case.protocol, case.raw.iter().map(String::as_str))
                .map_err(|error| format!("provider fixture {} failed: {error}", case.case_id))?;
            if replay.compatibility_events != case.expected_events {
                return Err(format!(
                    "provider fixture output mismatch: {}",
                    case.case_id
                ));
            }
            let terminals = replay
                .stream_events
                .iter()
                .filter(|event| matches!(event, StreamEvent::Terminal { .. }))
                .count();
            if terminals != 1 {
                return Err(format!(
                    "provider fixture terminal count mismatch: {}",
                    case.case_id
                ));
            }
            let serialized = serde_json::to_string(&replay.compatibility_events)
                .map_err(|_| "cannot serialize provider fixture output".to_owned())?;
            if serialized.contains("PRIVATE_REASONING")
                || serialized.contains("SECRET_PROVIDER_DETAIL")
            {
                return Err(format!(
                    "provider fixture leaked filtered content: {}",
                    case.case_id
                ));
            }
        }
    }

    let invalid_name = "invalid-cases.v1.json";
    let contents = fs::read_to_string(fixture_root.join(invalid_name))
        .map_err(|_| format!("cannot read provider fixture: {invalid_name}"))?;
    let fixture: InvalidFixture = serde_json::from_str(&contents)
        .map_err(|_| format!("invalid provider fixture JSON: {invalid_name}"))?;
    if fixture.schema_version != 1 {
        return Err("invalid provider fixture schema version".to_owned());
    }
    for case in fixture.cases {
        if case.expected_error.category != "protocol" {
            return Err(format!(
                "invalid provider fixture category: {}",
                case.case_id
            ));
        }
        let error = replay_fixture(case.protocol, case.raw.iter().map(String::as_str))
            .expect_err("invalid compatibility fixture must fail");
        if error != case.expected_error.code {
            return Err(format!("provider fixture error mismatch: {}", case.case_id));
        }
        let serialized = serde_json::to_string(&error)
            .map_err(|_| "cannot serialize provider fixture error".to_owned())?;
        if case
            .expected_error
            .forbidden_fragments
            .iter()
            .any(|forbidden| serialized.contains(forbidden))
        {
            return Err(format!(
                "provider fixture error leaked filtered content: {}",
                case.case_id
            ));
        }
    }
    Ok(())
}
