use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use minimax_compat_harness::{
    build_report, load_compat_manifests, load_coverage_matrix, load_source_authority,
    provider_evaluation_authorizes_release, provider_report_json, report_json, repository_root,
    retrieval_report_json, run_provider_evaluation, run_retrieval_evaluation,
    validate_coverage_matrix, validate_migration_fixture_manifest,
    validate_migration_support_window, validate_report, validate_source_authority,
    verify_fixture_compatibility, verify_fixture_compatibility_strict_precondition,
};
use minimax_protocol::{ProtocolErrorCode, ProviderProtocolKind, StreamEvent};
use minimax_provider::{CompatibilityEvent, replay_fixture};
use serde::Deserialize;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(result) => {
            if let Some(output) = result.output {
                print!("{output}");
            }
            if result.passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("compat verification failed: {error}");
            ExitCode::FAILURE
        }
    }
}

struct CommandResult {
    output: Option<String>,
    passed: bool,
}

impl CommandResult {
    const fn passed(output: Option<String>) -> Self {
        Self {
            output,
            passed: true,
        }
    }
}

fn run(arguments: Vec<String>) -> Result<CommandResult, String> {
    let root = repository_root();
    match arguments.as_slice() {
        [command] if command == "verify" => {
            verify_repository(&root, HostedEvidenceMode::Final)?;
            Ok(CommandResult::passed(None))
        }
        [command] if command == "verify-strict-precondition" => {
            verify_repository(&root, HostedEvidenceMode::CandidatePrecondition)?;
            Ok(CommandResult::passed(None))
        }
        [command] if command == "verify-candidate" => {
            verify_repository(&root, HostedEvidenceMode::None)?;
            Ok(CommandResult::passed(None))
        }
        [command, format_flag, format]
            if command == "report" && format_flag == "--format" && format == "json" =>
        {
            let manifests = load_compat_manifests(&root).map_err(|error| error.to_string())?;
            let report = build_report(&manifests, &root).map_err(|error| error.to_string())?;
            validate_report(&report, &manifests, &root).map_err(|error| error.to_string())?;
            report_json(&report)
                .map(|output| CommandResult::passed(Some(output)))
                .map_err(|error| error.to_string())
        }
        [command, format_flag, format]
            if command == "provider-eval" && format_flag == "--format" && format == "json" =>
        {
            let report = run_provider_evaluation(&root).map_err(|error| error.to_string())?;
            let output = provider_report_json(&report).map_err(|error| error.to_string())?;
            let passed = provider_evaluation_authorizes_release(&report, true);
            Ok(CommandResult {
                output: Some(output),
                passed,
            })
        }
        [command, format_flag, format]
            if command == "retrieval-eval" && format_flag == "--format" && format == "json" =>
        {
            let report = run_retrieval_evaluation(&root).map_err(|error| error.to_string())?;
            let output = retrieval_report_json(&report).map_err(|error| error.to_string())?;
            Ok(CommandResult {
                output: Some(output),
                passed: report.passed,
            })
        }
        _ => Err(
            "usage: minimax-compat-harness <verify|verify-strict-precondition|verify-candidate|report --format json|provider-eval --format json|retrieval-eval --format json>".to_owned(),
        ),
    }
}

#[derive(Clone, Copy)]
enum HostedEvidenceMode {
    None,
    CandidatePrecondition,
    Final,
}

fn verify_repository(root: &Path, hosted_evidence_mode: HostedEvidenceMode) -> Result<(), String> {
    let source_authority = load_source_authority(root).map_err(|error| error.to_string())?;
    validate_source_authority(root, &source_authority).map_err(|error| error.to_string())?;
    let coverage = load_coverage_matrix(root).map_err(|error| error.to_string())?;
    validate_coverage_matrix(root, &coverage, &source_authority)
        .map_err(|error| error.to_string())?;
    validate_migration_fixture_manifest(root).map_err(|error| error.to_string())?;
    validate_migration_support_window(root).map_err(|error| error.to_string())?;
    match hosted_evidence_mode {
        HostedEvidenceMode::None => verify_fixture_compatibility(root, false)?,
        HostedEvidenceMode::CandidatePrecondition => {
            verify_fixture_compatibility_strict_precondition(root)?;
        }
        HostedEvidenceMode::Final => verify_fixture_compatibility(root, true)?,
    }
    verify_provider_fixtures(root)?;
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
