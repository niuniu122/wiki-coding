use std::time::{SystemTime, UNIX_EPOCH};

use minimax_protocol::{
    GcClass, KnowledgeJobId, KnowledgeOperation, KnowledgePage, KnowledgePageStatus,
    KnowledgePatch, PageId, ProjectId, SchemaVersion, SourceCitation, TopicId,
};
use minimax_vault::{
    ProjectVault, VaultError, apply_forget_plan, apply_gc_plan, forget_confirmation,
    gc_apply_confirmation, gc_purge_confirmation, gc_report, hash_vault_bytes, import_inbox_file,
    parse_wiki_page, plan_forget, purge_gc_plan, read_gc_trash_manifest, render_wiki_page,
    undo_gc_plan,
};

const SEVEN_DAYS_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const THIRTY_ONE_DAYS_MS: u64 = 31 * 24 * 60 * 60 * 1_000;

fn vault() -> (tempfile::TempDir, tempfile::TempDir, ProjectVault) {
    let project = tempfile::tempdir().expect("project");
    let root = tempfile::tempdir().expect("vault");
    let vault = ProjectVault::bootstrap(
        project.path(),
        root.path(),
        ProjectId::new("retention-project").expect("project ID"),
        1,
    )
    .expect("bootstrap");
    (project, root, vault)
}

fn future_now() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis(),
    )
    .expect("milliseconds")
        + THIRTY_ONE_DAYS_MS
}

#[test]
fn retention_gc_reports_first_revalidates_and_never_moves_protected_raw() {
    let (_project, _root, vault) = vault();
    let raw = vault.root().join("raw/assets/protected.bin");
    std::fs::write(&raw, b"permanent evidence").expect("raw");
    let index = vault.root().join(".minimax/indexes/wiki.cache");
    std::fs::write(&index, b"derived cache").expect("index");
    let transient = vault.root().join(".minimax/transient/old.bin");
    std::fs::create_dir_all(transient.parent().expect("parent")).expect("transient directory");
    std::fs::write(&transient, b"old transient").expect("transient");
    let now = future_now();

    let plan = gc_report(&vault, now).expect("report");
    assert!(raw.exists() && index.exists() && transient.exists());
    assert!(plan.candidates.iter().any(|candidate| {
        candidate.relative_path == "raw/assets/protected.bin"
            && candidate.class == GcClass::Permanent
    }));
    assert!(plan.candidates.iter().any(|candidate| {
        candidate.relative_path == ".minimax/indexes/wiki.cache"
            && candidate.class == GcClass::Rebuildable
    }));

    std::fs::write(&index, b"external edit").expect("drift");
    assert_eq!(
        apply_gc_plan(&vault, &plan, &gc_apply_confirmation(&plan), now),
        Err(VaultError::Conflict)
    );
    assert!(raw.exists() && index.exists());

    let plan = gc_report(&vault, now).expect("fresh report");
    assert_eq!(
        apply_gc_plan(&vault, &plan, "yes", now),
        Err(VaultError::InvalidConfirmation)
    );
    let receipt = apply_gc_plan(&vault, &plan, &gc_apply_confirmation(&plan), now).expect("apply");
    assert_eq!(receipt.object_count, 2);
    assert!(raw.exists());
    assert!(!index.exists() && !transient.exists());
    let manifest = read_gc_trash_manifest(&vault, &plan.gc_id).expect("trash manifest");
    assert_eq!(manifest.expires_at_unix_ms, now + SEVEN_DAYS_MS);

    undo_gc_plan(&vault, &plan.gc_id, manifest.expires_at_unix_ms).expect("undo at expiry");
    assert!(raw.exists() && index.exists() && transient.exists());
}

#[test]
fn retention_purge_is_separate_plan_bound_and_strictly_after_expiry() {
    let (_project, _root, vault) = vault();
    let index = vault.root().join(".minimax/indexes/wiki.cache");
    std::fs::write(&index, b"derived cache").expect("index");
    let now = future_now();
    let plan = gc_report(&vault, now).expect("report");
    apply_gc_plan(&vault, &plan, &gc_apply_confirmation(&plan), now).expect("apply");
    let manifest = read_gc_trash_manifest(&vault, &plan.gc_id).expect("manifest");
    assert_eq!(
        purge_gc_plan(
            &vault,
            &plan.gc_id,
            "PURGE anything",
            manifest.expires_at_unix_ms + 1
        ),
        Err(VaultError::InvalidConfirmation)
    );
    assert_eq!(
        purge_gc_plan(
            &vault,
            &plan.gc_id,
            &gc_purge_confirmation(&manifest),
            manifest.expires_at_unix_ms,
        ),
        Err(VaultError::NotExpired)
    );
    let receipt = purge_gc_plan(
        &vault,
        &plan.gc_id,
        &gc_purge_confirmation(&manifest),
        manifest.expires_at_unix_ms + 1,
    )
    .expect("purge");
    assert_eq!(receipt.object_count, 1);
    assert!(!index.exists());
    assert!(read_gc_trash_manifest(&vault, &plan.gc_id).is_err());
}

#[test]
fn retention_forget_recrystallizes_every_claim_before_removing_sensitive_raw() {
    let (_project, _root, vault) = vault();
    let sensitive_marker = "sensitive-user-detail-97421";
    std::fs::write(
        vault.root().join("inbox/sensitive.txt"),
        sensitive_marker.as_bytes(),
    )
    .expect("sensitive inbox");
    std::fs::write(
        vault.root().join("inbox/safe.txt"),
        b"public replacement evidence",
    )
    .expect("safe inbox");
    let sensitive = import_inbox_file(&vault, "inbox/sensitive.txt", 10).expect("import sensitive");
    let safe = import_inbox_file(&vault, "inbox/safe.txt", 11).expect("import safe");
    let page = KnowledgePage {
        schema_version: SchemaVersion,
        page_id: PageId::new("privacy-page").expect("page"),
        topic_id: TopicId::new("privacy-topic").expect("topic"),
        relative_path: "wiki/decisions/privacy.md".to_owned(),
        title: "Private decision".to_owned(),
        status: KnowledgePageStatus::Current,
        superseded_by: None,
        sources: vec![SourceCitation {
            source_id: sensitive.evidence_id.clone(),
            source_hash: sensitive.content_hash.clone(),
        }],
        body: "A claim that must be re-crystallized.".to_owned(),
    };
    let page_bytes = render_wiki_page(&page).expect("render");
    std::fs::write(vault.root().join(&page.relative_path), &page_bytes).expect("page write");
    let plan = plan_forget(
        &vault,
        sensitive.evidence_id.clone(),
        sensitive.content_hash.clone(),
        20,
    )
    .expect("forget plan");
    assert_eq!(plan.affected_page_paths, vec![page.relative_path.clone()]);

    let replacement = KnowledgePage {
        sources: vec![SourceCitation {
            source_id: safe.evidence_id,
            source_hash: safe.content_hash,
        }],
        body: "A non-sensitive claim grounded in replacement evidence.".to_owned(),
        ..page
    };
    let patch = KnowledgePatch {
        schema_version: SchemaVersion,
        job_id: KnowledgeJobId::new("forget-job").expect("job"),
        operations: vec![KnowledgeOperation::Replace {
            page: replacement,
            expected_hash: hash_vault_bytes(&page_bytes),
        }],
    };
    assert_eq!(
        apply_forget_plan(&vault, &plan, &patch, "yes", 21),
        Err(VaultError::InvalidConfirmation)
    );
    assert!(
        vault
            .root()
            .join(&sensitive.imported_relative_path)
            .exists()
    );
    let receipt =
        apply_forget_plan(&vault, &plan, &patch, &forget_confirmation(&plan), 21).expect("forget");
    assert!(
        !vault
            .root()
            .join(&sensitive.imported_relative_path)
            .exists()
    );
    let rebuilt = parse_wiki_page(
        "wiki/decisions/privacy.md",
        &std::fs::read(vault.root().join("wiki/decisions/privacy.md")).expect("page"),
    )
    .expect("parse page");
    assert!(
        rebuilt
            .sources
            .iter()
            .all(|source| source.source_id != sensitive.evidence_id)
    );
    let tombstone =
        std::fs::read(vault.root().join(&receipt.tombstone_relative_path)).expect("tombstone");
    let text = String::from_utf8(tombstone).expect("UTF-8");
    assert!(!text.contains(sensitive_marker));
    assert!(!text.contains(sensitive.evidence_id.as_str()));
    assert!(text.contains("forgotten"));
}
