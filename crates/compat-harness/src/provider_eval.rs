use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};
use std::fs;
use std::path::{Component, Path};

use minimax_protocol::{ProtocolErrorCode, ProviderProtocolKind, StreamEvent};
use minimax_provider::{CompatibilityEvent, FixtureReplay, replay_fixture};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{load_compat_manifests, validate_rust_provider_profiles};

pub const PROVIDER_EVALUATION_MANIFEST: &str = "fixtures/compat/evaluations/provider.v1.json";
pub const PROVIDER_EVALUATION_GOLDEN: &str =
    "fixtures/compat/evaluations/provider-report.expected.json";

const EVALUATION_ID: &str = "provider-conformance-v1";
const REQUIRED_CHECKS: [ProviderCheckId; 10] = [
    ProviderCheckId::ValidText,
    ProviderCheckId::Usage,
    ProviderCheckId::TerminalOrdering,
    ProviderCheckId::NativeToolCallIdentity,
    ProviderCheckId::MalformedRejected,
    ProviderCheckId::PrematureRejected,
    ProviderCheckId::SafeErrorCodes,
    ProviderCheckId::FailureRedaction,
    ProviderCheckId::UnsupportedFeatureRejection,
    ProviderCheckId::DeterministicRepeat,
];

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderEvaluationManifest {
    schema_version: u16,
    evaluation_id: String,
    provider_manifest: FixtureReference,
    invalid_fixture: FixtureReference,
    protocols: Vec<ProtocolEvaluationManifest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureReference {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProtocolEvaluationManifest {
    protocol: ProviderProtocolKind,
    fixture: FixtureReference,
    required_checks: Vec<ProviderCheckId>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProviderCheckId {
    ValidText,
    Usage,
    TerminalOrdering,
    NativeToolCallIdentity,
    MalformedRejected,
    PrematureRejected,
    SafeErrorCodes,
    FailureRedaction,
    UnsupportedFeatureRejection,
    DeterministicRepeat,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEvaluationReport {
    pub schema_version: u16,
    pub evaluation_id: String,
    pub fixture_fingerprint: String,
    pub protocols: Vec<ProviderProtocolReport>,
    pub totals: ProviderEvaluationTotals,
    pub passed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProtocolReport {
    pub protocol: ProviderProtocolKind,
    pub fixture_path: String,
    pub fixture_sha256: String,
    pub checks: Vec<ProviderCheckReport>,
    pub passed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderCheckReport {
    pub id: String,
    pub passed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderEvaluationTotals {
    pub protocols: usize,
    pub checks: usize,
    pub passed: usize,
    pub failed: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderEvaluationError {
    ManifestRead,
    ManifestParse(String),
    InvalidManifest(String),
    FixtureRead(String),
    FixtureFingerprint(String),
    FixtureParse(String),
    ProviderProfile(String),
    ReportSerialization,
    EvaluationFailed,
    GoldenRead,
    GoldenDrift,
}

impl fmt::Display for ProviderEvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestRead => formatter.write_str("cannot read Provider evaluation manifest"),
            Self::ManifestParse(message) => {
                write!(formatter, "invalid Provider evaluation manifest: {message}")
            }
            Self::InvalidManifest(message) => {
                write!(formatter, "invalid Provider evaluation manifest: {message}")
            }
            Self::FixtureRead(path) => write!(formatter, "cannot read Provider fixture: {path}"),
            Self::FixtureFingerprint(path) => {
                write!(formatter, "Provider fixture fingerprint mismatch: {path}")
            }
            Self::FixtureParse(path) => write!(formatter, "invalid Provider fixture: {path}"),
            Self::ProviderProfile(message) => {
                write!(formatter, "invalid Provider profile evidence: {message}")
            }
            Self::ReportSerialization => {
                formatter.write_str("cannot serialize Provider evaluation report")
            }
            Self::EvaluationFailed => formatter.write_str("Provider evaluation failed"),
            Self::GoldenRead => formatter.write_str("cannot read Provider evaluation golden"),
            Self::GoldenDrift => formatter.write_str("Provider evaluation golden drift"),
        }
    }
}

impl std::error::Error for ProviderEvaluationError {}

pub fn run_provider_evaluation(
    root: &Path,
) -> Result<ProviderEvaluationReport, ProviderEvaluationError> {
    let manifest = load_manifest(root)?;
    validate_manifest(&manifest)?;
    let provider_bytes = read_fingerprinted(root, &manifest.provider_manifest)?;
    let invalid_bytes = read_fingerprinted(root, &manifest.invalid_fixture)?;
    let invalid_fixture: InvalidFixture = serde_json::from_slice(&invalid_bytes).map_err(|_| {
        ProviderEvaluationError::FixtureParse(manifest.invalid_fixture.path.clone())
    })?;
    if invalid_fixture.schema_version != 1 {
        return Err(ProviderEvaluationError::FixtureParse(
            manifest.invalid_fixture.path.clone(),
        ));
    }
    validate_invalid_fixture(&invalid_fixture, &manifest.invalid_fixture.path)?;

    let manifests = load_compat_manifests(root)
        .map_err(|error| ProviderEvaluationError::ProviderProfile(error.to_string()))?;
    if sha256(&provider_bytes) != manifest.provider_manifest.sha256 {
        return Err(ProviderEvaluationError::FixtureFingerprint(
            manifest.provider_manifest.path.clone(),
        ));
    }
    let profiles_valid = validate_rust_provider_profiles(&manifests.providers).is_ok();
    let features = &manifests.providers.feature_matrix;
    let unsupported_features_rejected = profiles_valid
        && features.streaming
        && features.native_tool_calls
        && features.parallel_tool_calls
        && features.reasoning_metadata
        && features.usage
        && !features.structured_output
        && !features.prompt_caching
        && !features.image_input
        && !features.audio_input
        && !features.provider_hosted_tools;

    let mut protocols = Vec::with_capacity(manifest.protocols.len());
    for protocol in &manifest.protocols {
        let valid_bytes = read_fingerprinted(root, &protocol.fixture)?;
        let valid_cases = parse_valid_cases(&valid_bytes, protocol, &protocol.fixture.path)?;
        let first = evaluate_base_checks(
            protocol.protocol,
            &valid_cases,
            &invalid_fixture,
            unsupported_features_rejected,
        );
        let second = evaluate_base_checks(
            protocol.protocol,
            &valid_cases,
            &invalid_fixture,
            unsupported_features_rejected,
        );
        let deterministic = first == second;
        let mut values = first;
        values.insert(ProviderCheckId::DeterministicRepeat, deterministic);
        let checks = protocol
            .required_checks
            .iter()
            .map(|id| ProviderCheckReport {
                id: check_name(*id),
                passed: values.get(id).copied().unwrap_or(false),
            })
            .collect::<Vec<_>>();
        let passed = checks.iter().all(|check| check.passed);
        protocols.push(ProviderProtocolReport {
            protocol: protocol.protocol,
            fixture_path: protocol.fixture.path.clone(),
            fixture_sha256: protocol.fixture.sha256.clone(),
            checks,
            passed,
        });
    }

    let checks = protocols
        .iter()
        .map(|protocol| protocol.checks.len())
        .sum::<usize>();
    let passed_checks = protocols
        .iter()
        .flat_map(|protocol| &protocol.checks)
        .filter(|check| check.passed)
        .count();
    let passed = protocols.iter().all(|protocol| protocol.passed);
    let fixture_fingerprint = fixture_fingerprint(&manifest);
    Ok(ProviderEvaluationReport {
        schema_version: 1,
        evaluation_id: manifest.evaluation_id,
        fixture_fingerprint,
        totals: ProviderEvaluationTotals {
            protocols: protocols.len(),
            checks,
            passed: passed_checks,
            failed: checks - passed_checks,
        },
        protocols,
        passed,
    })
}

pub fn provider_report_json(
    report: &ProviderEvaluationReport,
) -> Result<String, ProviderEvaluationError> {
    let mut output = serde_json::to_string_pretty(report)
        .map_err(|_| ProviderEvaluationError::ReportSerialization)?;
    output.push('\n');
    Ok(output)
}

pub fn verify_provider_evaluation(
    root: &Path,
) -> Result<ProviderEvaluationReport, ProviderEvaluationError> {
    let report = run_provider_evaluation(root)?;
    if !report.passed {
        return Err(ProviderEvaluationError::EvaluationFailed);
    }
    let actual = provider_report_json(&report)?;
    let expected = fs::read_to_string(root.join(PROVIDER_EVALUATION_GOLDEN))
        .map_err(|_| ProviderEvaluationError::GoldenRead)?;
    if actual != normalize_newline(&expected) {
        return Err(ProviderEvaluationError::GoldenDrift);
    }
    Ok(report)
}

#[must_use]
pub fn provider_evaluation_authorizes_release(
    report: &ProviderEvaluationReport,
    package_smoke_succeeded: bool,
) -> bool {
    report.passed
        && package_smoke_succeeded
        && report.totals.failed == 0
        && report.totals.passed == report.totals.checks
        && report
            .protocols
            .iter()
            .all(|protocol| protocol.passed && protocol.checks.iter().all(|check| check.passed))
}

fn load_manifest(root: &Path) -> Result<ProviderEvaluationManifest, ProviderEvaluationError> {
    let raw = fs::read_to_string(root.join(PROVIDER_EVALUATION_MANIFEST))
        .map_err(|_| ProviderEvaluationError::ManifestRead)?;
    serde_json::from_str(&raw)
        .map_err(|error| ProviderEvaluationError::ManifestParse(error.to_string()))
}

fn validate_manifest(manifest: &ProviderEvaluationManifest) -> Result<(), ProviderEvaluationError> {
    if manifest.schema_version != 1 || manifest.evaluation_id != EVALUATION_ID {
        return invalid_manifest("schemaVersion or evaluationId is not supported");
    }
    validate_fixture_reference(&manifest.provider_manifest)?;
    validate_fixture_reference(&manifest.invalid_fixture)?;
    let expected_protocols = [
        ProviderProtocolKind::Responses,
        ProviderProtocolKind::ChatCompletions,
    ];
    if manifest.protocols.len() != expected_protocols.len() {
        return invalid_manifest("both Provider protocols must be declared exactly once");
    }
    for (protocol, expected) in manifest.protocols.iter().zip(expected_protocols) {
        if protocol.protocol != expected {
            return invalid_manifest("Provider protocols must use the stable declared order");
        }
        validate_fixture_reference(&protocol.fixture)?;
        if protocol.required_checks != REQUIRED_CHECKS {
            return invalid_manifest(
                "each protocol required check list must be complete, ordered, and duplicate-free",
            );
        }
    }
    let paths = std::iter::once(manifest.provider_manifest.path.as_str())
        .chain(std::iter::once(manifest.invalid_fixture.path.as_str()))
        .chain(
            manifest
                .protocols
                .iter()
                .map(|protocol| protocol.fixture.path.as_str()),
        )
        .collect::<BTreeSet<_>>();
    if paths.len() != manifest.protocols.len() + 2 {
        return invalid_manifest("Provider fixture paths must be unique");
    }
    Ok(())
}

fn validate_fixture_reference(reference: &FixtureReference) -> Result<(), ProviderEvaluationError> {
    validate_relative_path(&reference.path)?;
    if reference.sha256.len() != 64
        || !reference
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid_manifest("fixture sha256 must be lowercase hexadecimal");
    }
    Ok(())
}

fn read_fingerprinted(
    root: &Path,
    reference: &FixtureReference,
) -> Result<Vec<u8>, ProviderEvaluationError> {
    let bytes = fs::read(root.join(&reference.path))
        .map_err(|_| ProviderEvaluationError::FixtureRead(reference.path.clone()))?;
    if sha256(&bytes) != reference.sha256 {
        return Err(ProviderEvaluationError::FixtureFingerprint(
            reference.path.clone(),
        ));
    }
    Ok(bytes)
}

fn parse_valid_cases(
    bytes: &[u8],
    protocol: &ProtocolEvaluationManifest,
    path: &str,
) -> Result<Vec<ValidCase>, ProviderEvaluationError> {
    let raw = std::str::from_utf8(bytes)
        .map_err(|_| ProviderEvaluationError::FixtureParse(path.to_owned()))?;
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let case: ValidCase = serde_json::from_str(line)
            .map_err(|_| ProviderEvaluationError::FixtureParse(path.to_owned()))?;
        if case.protocol != protocol.protocol || !ids.insert(case.case_id.clone()) {
            return Err(ProviderEvaluationError::FixtureParse(path.to_owned()));
        }
        cases.push(case);
    }
    if cases.is_empty() {
        return Err(ProviderEvaluationError::FixtureParse(path.to_owned()));
    }
    Ok(cases)
}

fn validate_invalid_fixture(
    fixture: &InvalidFixture,
    path: &str,
) -> Result<(), ProviderEvaluationError> {
    let mut ids = BTreeSet::new();
    if fixture.cases.is_empty()
        || fixture
            .cases
            .iter()
            .any(|case| !ids.insert(case.case_id.as_str()))
    {
        return Err(ProviderEvaluationError::FixtureParse(path.to_owned()));
    }
    Ok(())
}

fn evaluate_base_checks(
    protocol: ProviderProtocolKind,
    valid_cases: &[ValidCase],
    invalid_fixture: &InvalidFixture,
    unsupported_features_rejected: bool,
) -> BTreeMap<ProviderCheckId, bool> {
    let replayed = valid_cases
        .iter()
        .map(|case| {
            (
                case,
                replay_fixture(protocol, case.raw.iter().map(String::as_str)),
            )
        })
        .collect::<Vec<_>>();
    let exact = |case: &ValidCase, replay: &FixtureReplay| {
        replay.compatibility_events == case.expected_events
    };
    let valid_text = replayed.iter().any(|(case, replay)| {
        replay.as_ref().is_ok_and(|value| {
            exact(case, value)
                && value.compatibility_events.iter().any(|event| {
                    matches!(event, CompatibilityEvent::TextDelta { delta } if !delta.is_empty())
                })
        })
    });
    let usage = replayed.iter().any(|(case, replay)| {
        replay.as_ref().is_ok_and(|value| {
            exact(case, value)
                && value
                    .compatibility_events
                    .iter()
                    .any(|event| matches!(event, CompatibilityEvent::Usage { .. }))
        })
    });
    let terminal_ordering = replayed.iter().all(|(case, replay)| {
        replay.as_ref().is_ok_and(|value| {
            exact(case, value)
                && value
                    .stream_events
                    .iter()
                    .filter(|event| matches!(event, StreamEvent::Terminal { .. }))
                    .count()
                    == 1
                && matches!(
                    value.compatibility_events.last(),
                    Some(CompatibilityEvent::Completed)
                )
        })
    });
    let native_tool_call_identity = replayed.iter().any(|(case, replay)| {
        case.case_id.ends_with("tool_identity")
            && replay.as_ref().is_ok_and(|value| {
                exact(case, value) && value.compatibility_events.iter().any(|event| {
                    matches!(event, CompatibilityEvent::ToolCall { call_id, name, arguments_json }
                            if call_id.as_str() == "call-1"
                                && name == "invoke_local_capability"
                                && arguments_json.contains("README.md"))
                })
            })
    }) && replayed.iter().any(|(case, replay)| {
        case.case_id.ends_with("two_tools_keep_emission_order")
            && replay.as_ref().is_ok_and(|value| {
                exact(case, value)
                    && value
                        .compatibility_events
                        .iter()
                        .filter_map(|event| match event {
                            CompatibilityEvent::ToolCall { call_id, .. } => Some(call_id.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        == ["call-z", "call-a"]
            })
    });

    let malformed_case = invalid_fixture
        .cases
        .iter()
        .find(|case| case.case_id == "malformed_json");
    let malformed_rejected = malformed_case.is_some_and(|case| {
        replay_fixture(protocol, case.raw.iter().map(String::as_str))
            .is_err_and(|error| error == ProtocolErrorCode::MalformedJson)
    });
    let premature_rejected = valid_cases
        .iter()
        .find(|case| case.case_id.ends_with("visible_usage_completion"))
        .is_some_and(|case| {
            let prefix = &case.raw[..case.raw.len().saturating_sub(1)];
            replay_fixture(protocol, prefix.iter().map(String::as_str))
                .is_err_and(|error| error == ProtocolErrorCode::PrematureEof)
        });
    let safe_error_codes = invalid_fixture
        .cases
        .iter()
        .filter(|case| case.protocol == protocol)
        .all(|case| {
            case.expected_error.category == "protocol"
                && replay_fixture(protocol, case.raw.iter().map(String::as_str)).is_err_and(
                    |error| {
                        error == case.expected_error.code
                            && serde_json::to_string(&error).is_ok_and(|serialized| {
                                case.expected_error
                                    .forbidden_fragments
                                    .iter()
                                    .all(|fragment| !serialized.contains(fragment))
                            })
                    },
                )
        });
    let failure_redaction = malformed_case.is_some_and(|case| {
        !case.expected_error.forbidden_fragments.is_empty()
            && replay_fixture(protocol, case.raw.iter().map(String::as_str)).is_err_and(|error| {
                serde_json::to_string(&error).is_ok_and(|serialized| {
                    case.expected_error
                        .forbidden_fragments
                        .iter()
                        .all(|fragment| !serialized.contains(fragment))
                })
            })
            && replayed.iter().all(|(_, replay)| {
                replay.as_ref().is_ok_and(|value| {
                    serde_json::to_string(&value.compatibility_events).is_ok_and(|serialized| {
                        !serialized.contains("PRIVATE_REASONING")
                            && !serialized.contains("SECRET_PROVIDER_DETAIL")
                    })
                })
            })
    });

    BTreeMap::from([
        (ProviderCheckId::ValidText, valid_text),
        (ProviderCheckId::Usage, usage),
        (ProviderCheckId::TerminalOrdering, terminal_ordering),
        (
            ProviderCheckId::NativeToolCallIdentity,
            native_tool_call_identity,
        ),
        (ProviderCheckId::MalformedRejected, malformed_rejected),
        (ProviderCheckId::PrematureRejected, premature_rejected),
        (ProviderCheckId::SafeErrorCodes, safe_error_codes),
        (ProviderCheckId::FailureRedaction, failure_redaction),
        (
            ProviderCheckId::UnsupportedFeatureRejection,
            unsupported_features_rejected,
        ),
    ])
}

fn fixture_fingerprint(manifest: &ProviderEvaluationManifest) -> String {
    let mut inputs = BTreeMap::from([
        (
            manifest.provider_manifest.path.as_str(),
            manifest.provider_manifest.sha256.as_str(),
        ),
        (
            manifest.invalid_fixture.path.as_str(),
            manifest.invalid_fixture.sha256.as_str(),
        ),
    ]);
    for protocol in &manifest.protocols {
        inputs.insert(
            protocol.fixture.path.as_str(),
            protocol.fixture.sha256.as_str(),
        );
    }
    let canonical = inputs
        .into_iter()
        .map(|(path, hash)| format!("{path}:{hash}\n"))
        .collect::<String>();
    sha256(canonical.as_bytes())
}

fn check_name(id: ProviderCheckId) -> String {
    serde_json::to_value(id)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .expect("Provider check IDs serialize as strings")
}

fn validate_relative_path(path: &str) -> Result<(), ProviderEvaluationError> {
    let parsed = Path::new(path);
    if path.is_empty()
        || path.contains('\\')
        || parsed.is_absolute()
        || parsed.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::ParentDir
                    | Component::CurDir
            )
        })
    {
        return invalid_manifest("fixture paths must be safe repository-relative paths");
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn normalize_newline(value: &str) -> String {
    value.trim_end_matches(['\r', '\n']).to_owned() + "\n"
}

fn invalid_manifest<T>(message: impl Into<String>) -> Result<T, ProviderEvaluationError> {
    Err(ProviderEvaluationError::InvalidManifest(message.into()))
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidCase {
    case_id: String,
    protocol: ProviderProtocolKind,
    raw: Vec<String>,
    expected_events: Vec<CompatibilityEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InvalidFixture {
    schema_version: u16,
    cases: Vec<InvalidCase>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvalidCase {
    case_id: String,
    protocol: ProviderProtocolKind,
    raw: Vec<String>,
    expected_error: ExpectedError,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedError {
    code: ProtocolErrorCode,
    category: String,
    forbidden_fragments: Vec<String>,
}
