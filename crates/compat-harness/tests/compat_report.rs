use std::collections::BTreeSet;
use std::fs;

use minimax_compat_harness::{
    ManifestError, ParityStatus, build_report, load_compat_manifests, report_json, repository_root,
    validate_report,
};

#[test]
fn compat_report_matches_golden_and_is_byte_identical_on_second_run() {
    let root = repository_root();
    let first_manifests = load_compat_manifests(&root).expect("strict manifests");
    let second_manifests = load_compat_manifests(&root).expect("strict manifests on second load");
    let first = build_report(&first_manifests);
    let second = build_report(&second_manifests);
    validate_report(&first, &root).expect("valid report");
    validate_report(&second, &root).expect("valid report on second run");

    let first_json = report_json(&first).expect("first JSON");
    let second_json = report_json(&second).expect("second JSON");
    assert_eq!(first_json, second_json);
    let expected = fs::read_to_string(root.join("fixtures/compat/report.expected.json"))
        .expect("golden report");
    assert_eq!(first_json, expected);
}

#[test]
fn compat_report_contains_every_baseline_item_exactly_once() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    let report = build_report(&manifests);
    let baseline_ids = manifests
        .baseline
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let report_ids = report
        .entries
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(report.entries.len(), manifests.baseline.items.len());
    assert_eq!(report_ids, baseline_ids);
    assert_eq!(manifests.commands.commands.len(), 17);
    assert_eq!(manifests.providers.profile_classes.len(), 3);
}

#[test]
fn compat_report_rejects_matched_item_without_evidence() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    let mut report = build_report(&manifests);
    let matched = report
        .entries
        .iter_mut()
        .find(|item| item.status == ParityStatus::Matched)
        .expect("matched item");
    let id = matched.id.clone();
    matched.evidence.clear();

    assert_eq!(
        validate_report(&report, &root),
        Err(ManifestError::Validation(format!(
            "matched item requires evidence: {id}"
        )))
    );
}
