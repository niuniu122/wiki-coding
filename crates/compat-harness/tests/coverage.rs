use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use minimax_compat_harness::{
    CoverageDisposition, CoverageMatrix, load_coverage_matrix, load_source_authority,
    validate_coverage_matrix,
};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .to_path_buf()
}

#[test]
fn historical_coverage_matrix_is_sealed_after_authority_reaches_zero() {
    let root = repository_root();
    let authority: Value = serde_json::from_str(
        &fs::read_to_string(root.join("fixtures/compat/source-authority.v1.json"))
            .expect("source authority manifest"),
    )
    .expect("source authority JSON");
    let matrix: Value = serde_json::from_str(
        &fs::read_to_string(
            root.join("fixtures/compat/verification/typescript-responsibilities.v1.json"),
        )
        .expect("TypeScript responsibility matrix"),
    )
    .expect("coverage matrix JSON");

    assert_eq!(
        authority["transitionalTypeScript"]["entries"]
            .as_array()
            .expect("transitional TypeScript entries")
            .len(),
        0
    );
    assert_eq!(
        authority["transitionalLegacyTestFixtures"]["entries"]
            .as_array()
            .expect("legacy test fixtures")
            .len(),
        0
    );
    assert_eq!(
        matrix["sources"]
            .as_array()
            .expect("sealed historical coverage sources")
            .len(),
        97
    );
}

#[test]
fn both_compatibility_verification_modes_are_coverage_gated() {
    let main = fs::read_to_string(repository_root().join("crates/compat-harness/src/main.rs"))
        .expect("compatibility main source");
    let verify_repository = main
        .find("fn verify_repository")
        .expect("repository verification function");
    let verification = &main[verify_repository..];
    let source_authority = verification
        .find("validate_source_authority")
        .expect("source authority preflight");
    let coverage = verification
        .find("validate_coverage_matrix")
        .expect("coverage preflight");
    let compatibility = verification
        .find("verify_fixture_compatibility")
        .expect("fixture compatibility verification");

    assert!(source_authority < coverage);
    assert!(coverage < compatibility);
}

#[test]
fn repository_matrix_validates_with_no_unresolved_responsibility() {
    let root = repository_root();
    let authority = load_source_authority(&root).expect("source authority");
    let matrix = load_coverage_matrix(&root).expect("coverage matrix");

    validate_coverage_matrix(&root, &matrix, &authority).expect("valid coverage matrix");
    assert_eq!(matrix.sources.len(), 97);
    assert!(matrix.sources.iter().all(|source| {
        !source.responsibilities.is_empty()
            && source
                .responsibilities
                .iter()
                .all(|responsibility| !responsibility.evidence.is_empty())
    }));
}

#[test]
fn missing_source_hash_drift_and_duplicate_responsibility_are_rejected() {
    let root = repository_root();
    let authority = load_source_authority(&root).expect("source authority");
    let matrix = load_coverage_matrix(&root).expect("coverage matrix");

    let mut missing = matrix.clone();
    missing.sources.remove(0);
    assert!(
        validate_coverage_matrix(&root, &missing, &authority)
            .expect_err("missing source must fail")
            .to_string()
            .contains("sealed Phase 10 source inventory drift")
    );

    let mut drifted = matrix.clone();
    drifted.sources[0].source_sha256 = "0".repeat(64);
    assert!(
        validate_coverage_matrix(&root, &drifted, &authority)
            .expect_err("hash drift must fail")
            .to_string()
            .contains("sealed Phase 10 source inventory drift")
    );

    let mut duplicate = matrix.clone();
    duplicate.sources[1].responsibilities[0].id =
        duplicate.sources[0].responsibilities[0].id.clone();
    assert!(
        validate_coverage_matrix(&root, &duplicate, &authority)
            .expect_err("duplicate responsibility must fail")
            .to_string()
            .contains("duplicate responsibility")
    );
}

#[test]
fn unresolved_unknown_fields_and_typescript_evidence_are_rejected() {
    let root = repository_root();
    let raw = fs::read_to_string(
        root.join("fixtures/compat/verification/typescript-responsibilities.v1.json"),
    )
    .expect("coverage matrix");
    let unknown = raw.replacen(
        "\"schemaVersion\": 2,",
        "\"schemaVersion\": 2, \"unknown\": true,",
        1,
    );
    assert!(serde_json::from_str::<CoverageMatrix>(&unknown).is_err());
    let unresolved = raw.replacen("\"rust_covered\"", "\"requires_port\"", 1);
    assert!(serde_json::from_str::<CoverageMatrix>(&unresolved).is_err());

    let authority = load_source_authority(&root).expect("source authority");
    let mut matrix = load_coverage_matrix(&root).expect("coverage matrix");
    let responsibility = &mut matrix.sources[0].responsibilities[0];
    responsibility.evidence[0].path = "README.md".to_owned();
    responsibility.evidence[0].test = None;
    assert!(
        validate_coverage_matrix(&root, &matrix, &authority)
            .expect_err("TypeScript evidence must fail")
            .to_string()
            .contains("not Rust or allowed package orchestration")
    );
}

#[test]
fn retirement_of_a_locked_public_contract_and_missing_evidence_are_rejected() {
    let root = repository_root();
    let authority = load_source_authority(&root).expect("source authority");
    let matrix = load_coverage_matrix(&root).expect("coverage matrix");
    let public_index = matrix
        .sources
        .iter()
        .position(|source| source.source_path == "test/chat-input-policy.test.ts")
        .expect("public command responsibility");

    let mut retired_public = matrix.clone();
    retired_public.sources[public_index].responsibilities[0].disposition =
        CoverageDisposition::Retired;
    retired_public.sources[public_index].responsibilities[0].rationale =
        "Dormant internal command behavior was retired despite its locked public contract."
            .to_owned();
    assert!(
        validate_coverage_matrix(&root, &retired_public, &authority)
            .expect_err("public retirement must fail")
            .to_string()
            .contains("locked public contract")
    );

    let mut missing_evidence = matrix;
    missing_evidence.sources[public_index].responsibilities[0]
        .evidence
        .clear();
    assert!(
        validate_coverage_matrix(&root, &missing_evidence, &authority)
            .expect_err("missing evidence must fail")
            .to_string()
            .contains("no replacement evidence")
    );
}

#[test]
fn semantic_audit_rejects_collapsed_unrelated_and_false_retirement_evidence() {
    let root = repository_root();
    let authority = load_source_authority(&root).expect("source authority");
    let matrix = load_coverage_matrix(&root).expect("coverage matrix");
    let mut failures = Vec::new();

    if let Err(error) = validate_coverage_matrix(&root, &matrix, &authority) {
        failures.push(format!(
            "baseline semantic evidence matrix is invalid: {error}"
        ));
    }

    let collapsed_retrieval_sources = [
        "src/eval/capability-retrieval-report.ts",
        "test/capability-bm25.test.ts",
        "test/capability-catalog.test.ts",
        "test/capability-commands.test.ts",
        "test/capability-dispatcher.test.ts",
        "test/capability-exact-resolution.test.ts",
        "test/capability-facet-index.test.ts",
        "test/capability-hybrid-retrieval.test.ts",
        "test/capability-manifest.test.ts",
        "test/capability-policy-engine.test.ts",
        "test/capability-query-normalizer.test.ts",
        "test/capability-refresh.test.ts",
        "test/capability-retrieval-eval.test.ts",
        "test/capability-retrieval-report.test.ts",
        "test/capability-rrf.test.ts",
        "test/capability-snapshot.test.ts",
        "test/capability-source-adapters.test.ts",
        "test/support/capability-fixtures.ts",
    ];
    let retrieval_responsibility_ids = matrix
        .sources
        .iter()
        .filter(|source| collapsed_retrieval_sources.contains(&source.source_path.as_str()))
        .flat_map(|source| {
            source
                .responsibilities
                .iter()
                .map(|responsibility| responsibility.id.as_str())
        })
        .collect::<BTreeSet<_>>();
    let retrieval_contracts = matrix
        .evidence_contracts
        .iter()
        .filter(|contract| {
            contract
                .responsibility_ids
                .iter()
                .any(|id| retrieval_responsibility_ids.contains(id.as_str()))
        })
        .map(|contract| contract.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected_retrieval_contracts = [
        "capability-catalog-policy-contract",
        "capability-command-dispatch-contract",
        "capability-corpus-manifest-contract",
        "capability-exact-bm25-contract",
        "capability-hybrid-candidate-contract",
        "capability-snapshot-refresh-contract",
        "retrieval-evaluation-authority",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if retrieval_contracts != expected_retrieval_contracts {
        failures.push(format!(
            "unrelated retrieval responsibilities are not separated into the seven closed semantic contracts: {retrieval_contracts:?}"
        ));
    }

    let shared_lexical_owner_ids = matrix
        .sources
        .iter()
        .flat_map(|source| source.responsibilities.iter())
        .filter(|responsibility| retrieval_responsibility_ids.contains(responsibility.id.as_str()))
        .filter(|responsibility| {
            responsibility.evidence.iter().any(|evidence| {
                evidence.path == "crates/retrieval/tests/lexical.rs"
                    && evidence.test.as_deref()
                        == Some("existing_typescript_175_case_fixture_meets_capability_gates")
            })
        })
        .map(|responsibility| responsibility.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected_lexical_owner_ids = [
        "ts-test-capability-bm25-test-ts",
        "ts-test-capability-exact-resolution-test-ts",
        "ts-test-capability-query-normalizer-test-ts",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if shared_lexical_owner_ids != expected_lexical_owner_ids {
        failures.push(format!(
            "shared lexical owner must be limited to its compatible ranking contract: {shared_lexical_owner_ids:?}"
        ));
    }

    let retry_continue = matrix
        .sources
        .iter()
        .flat_map(|source| source.responsibilities.iter())
        .find(|responsibility| responsibility.id == "ts-command-retry-continue-outcomes")
        .expect("retry/continue responsibility");
    if retry_continue.evidence.iter().any(|evidence| {
        evidence.path == "crates/tui/tests/command_render.rs"
            && evidence.test.as_deref()
                == Some("parser_covers_every_manifest_command_alias_and_argument_shape")
    }) {
        failures.push(
            "retry/continue outcome contract cites parser-only owner parser_covers_every_manifest_command_alias_and_argument_shape"
                .to_owned(),
        );
    }

    let retirement_families = [
        (
            "agent/kernel",
            &[
                "test/agent-item-storage.test.ts",
                "test/agent-route-cutover.test.ts",
                "test/agent-run-engine.test.ts",
                "test/application-kernel.test.ts",
            ][..],
        ),
        (
            "permission/tool/budget/fail-closed",
            &[
                "test/agent-budget.test.ts",
                "test/agent-run-recovery.test.ts",
                "test/feature-flags.test.ts",
            ][..],
        ),
        (
            "model/profile/credential",
            &[
                "test/credential-consent.test.ts",
                "test/model-profile-registry.test.ts",
                "test/model-profile.test.ts",
                "test/model-selection-persistence.test.ts",
                "test/model-selection-service.test.ts",
                "test/model-state-store.test.ts",
                "test/secret-store.test.ts",
                "test/user-profile-store.test.ts",
            ][..],
        ),
        ("summary/redaction", &["test/summary-generator.test.ts"][..]),
    ];
    for (family, paths) in retirement_families {
        let retired = matrix
            .sources
            .iter()
            .filter(|source| paths.contains(&source.source_path.as_str()))
            .filter(|source| {
                source.responsibilities.iter().any(|responsibility| {
                    responsibility.disposition == CoverageDisposition::Retired
                })
            })
            .map(|source| source.source_path.as_str())
            .collect::<Vec<_>>();
        if !retired.is_empty() {
            failures.push(format!(
                "public/safety {family} family is retired with generic no-shipped-outcome boilerplate: {}",
                retired.join(", ")
            ));
        }
    }

    let responsibility_ids = matrix
        .sources
        .iter()
        .flat_map(|source| source.responsibilities.iter())
        .map(|responsibility| responsibility.id.as_str())
        .collect::<BTreeSet<_>>();
    let assigned_responsibility_ids = matrix
        .evidence_contracts
        .iter()
        .flat_map(|contract| contract.responsibility_ids.iter())
        .map(String::as_str)
        .collect::<Vec<_>>();
    let assigned_responsibility_set = assigned_responsibility_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if assigned_responsibility_ids.len() != responsibility_ids.len()
        || assigned_responsibility_set != responsibility_ids
    {
        failures.push(format!(
            "semantic evidence contracts are not a one-to-one closure over responsibilities: rows={}, assignments={}, unique_assignments={}",
            responsibility_ids.len(),
            assigned_responsibility_ids.len(),
            assigned_responsibility_set.len()
        ));
    }

    let mut unrelated_owner = matrix.clone();
    let retry_continue = unrelated_owner
        .sources
        .iter_mut()
        .flat_map(|source| source.responsibilities.iter_mut())
        .find(|responsibility| responsibility.id == "ts-command-retry-continue-outcomes")
        .expect("retry/continue responsibility");
    retry_continue.evidence = vec![minimax_compat_harness::CoverageEvidence {
        path: "crates/retrieval/tests/lexical.rs".to_owned(),
        test: Some("existing_typescript_175_case_fixture_meets_capability_gates".to_owned()),
    }];
    if validate_coverage_matrix(&root, &unrelated_owner, &authority).is_ok() {
        failures.push(
            "validator accepts an existing lexical ranking function as retry/continue outcome evidence without checking semantic compatibility"
                .to_owned(),
        );
    }

    assert!(failures.is_empty(), "{}", failures.join("\n- "));
}
