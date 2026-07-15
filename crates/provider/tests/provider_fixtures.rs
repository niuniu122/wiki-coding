use std::fs;
use std::path::{Path, PathBuf};

use minimax_protocol::{ProtocolErrorCode, ProviderProtocolKind, StreamEvent};
use minimax_provider::{CompatibilityEvent, replay_fixture};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidCase {
    case_id: String,
    protocol: ProviderProtocolKind,
    raw: Vec<String>,
    expected_events: Vec<CompatibilityEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InvalidFixture {
    schema_version: u16,
    cases: Vec<InvalidCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvalidCase {
    case_id: String,
    protocol: ProviderProtocolKind,
    raw: Vec<String>,
    expected_error: ExpectedError,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedError {
    code: ProtocolErrorCode,
    category: String,
    forbidden_fragments: Vec<String>,
}

#[test]
fn both_provider_protocols_match_every_valid_compatibility_fixture() {
    for fixture in [
        fixture_path("responses.valid.jsonl"),
        fixture_path("chat-completions.valid.jsonl"),
    ] {
        let contents = fs::read_to_string(&fixture).expect("fixture should be readable");
        for line in contents.lines().filter(|line| !line.trim().is_empty()) {
            let case: ValidCase = serde_json::from_str(line).expect("fixture should be valid");
            let replay = replay_fixture(case.protocol, case.raw.iter().map(String::as_str))
                .unwrap_or_else(|code| panic!("{} failed with {code}", case.case_id));

            assert_eq!(
                replay.compatibility_events, case.expected_events,
                "{}",
                case.case_id
            );
            assert_eq!(
                replay
                    .stream_events
                    .iter()
                    .filter(|event| matches!(event, StreamEvent::Terminal { .. }))
                    .count(),
                1,
                "{}",
                case.case_id
            );
            let serialized = serde_json::to_string(&replay.compatibility_events)
                .expect("events should serialize");
            assert!(
                !serialized.contains("PRIVATE_REASONING"),
                "{}",
                case.case_id
            );
            assert!(
                !serialized.contains("SECRET_PROVIDER_DETAIL"),
                "{}",
                case.case_id
            );
        }
    }
}

#[test]
fn every_invalid_fixture_returns_its_declared_safe_code() {
    let contents = fs::read_to_string(fixture_path("invalid-cases.v1.json"))
        .expect("fixture should be readable");
    let fixture: InvalidFixture = serde_json::from_str(&contents).expect("fixture should be valid");
    assert_eq!(fixture.schema_version, 1);

    for case in fixture.cases {
        assert_eq!(case.expected_error.category, "protocol", "{}", case.case_id);
        let error = replay_fixture(case.protocol, case.raw.iter().map(String::as_str))
            .expect_err("invalid fixture should fail");
        assert_eq!(error, case.expected_error.code, "{}", case.case_id);
        let serialized = serde_json::to_string(&error).expect("error code should serialize");
        for forbidden in case.expected_error.forbidden_fragments {
            assert!(!serialized.contains(&forbidden), "{}", case.case_id);
        }
    }
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/compat/provider-streams")
        .join(name)
}
