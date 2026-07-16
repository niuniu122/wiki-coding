use clap::Parser as _;
use minimax_cli::{Cli, CliCommand, GcPlanOutput, JsonlWriter, VaultAction, VaultGcAction};
use minimax_protocol::{GcClass, ProjectId};
use minimax_tui::EventRenderer;
use minimax_vault::{ProjectVault, gc_apply_confirmation, gc_report, hash_vault_bytes, lint_vault};

fn base() -> [&'static str; 8] {
    [
        "minimax-codex-rust",
        "vault",
        "--project",
        ".",
        "--vault",
        "project.vault",
        "--project-id",
        "project-1",
    ]
}

#[test]
fn vault_cli_routes_every_report_and_destructive_action_without_force() {
    for suffix in [
        vec!["bootstrap".to_owned()],
        vec!["status".to_owned()],
        vec!["lint".to_owned()],
        vec!["repair".to_owned()],
        vec!["rebuild".to_owned()],
        vec!["import".to_owned(), "inbox/note.md".to_owned()],
        vec!["gc".to_owned(), "report".to_owned()],
        vec!["gc".to_owned(), "undo".to_owned(), "gc:fixture".to_owned()],
        vec![
            "gc",
            "apply",
            "--plan",
            "gc-plan.json",
            "--confirmation",
            "APPLY gc:fixture abc",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        vec![
            "gc",
            "purge",
            "gc:fixture",
            "--confirmation",
            "PURGE gc:fixture abc",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        vec![
            "forget".to_owned(),
            "plan".to_owned(),
            "evidence:1".to_owned(),
            "a".repeat(64),
        ],
        vec![
            "forget",
            "apply",
            "--plan",
            "forget-plan.json",
            "--patch",
            "patch.json",
            "--confirmation",
            "FORGET forget:fixture abc",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    ] {
        let mut arguments = base().into_iter().map(str::to_owned).collect::<Vec<_>>();
        arguments.extend(suffix);
        assert!(Cli::try_parse_from(arguments).is_ok());
    }
    let mut force = base().into_iter().map(str::to_owned).collect::<Vec<_>>();
    force.extend(["gc".to_owned(), "report".to_owned(), "--force".to_owned()]);
    assert!(Cli::try_parse_from(force).is_err());

    let mut missing = base().into_iter().map(str::to_owned).collect::<Vec<_>>();
    missing.extend([
        "gc".to_owned(),
        "apply".to_owned(),
        "--plan".to_owned(),
        "gc-plan.json".to_owned(),
    ]);
    assert!(Cli::try_parse_from(missing).is_err());
}

#[test]
fn vault_report_has_the_same_stable_facts_in_text_and_jsonl() {
    let project = tempfile::tempdir().expect("project");
    let root = tempfile::tempdir().expect("vault");
    let vault = ProjectVault::bootstrap(
        project.path(),
        root.path(),
        ProjectId::new("project-1").expect("project ID"),
        1,
    )
    .expect("vault");
    std::fs::write(vault.root().join(".minimax/indexes/wiki.cache"), b"derived").expect("cache");
    let plan = gc_report(&vault, 2).expect("plan");
    let output = GcPlanOutput {
        confirmation: gc_apply_confirmation(&plan),
        plan,
    };
    let text = EventRenderer::gc_plan(&output.plan, &output.confirmation);
    assert!(text.contains(output.plan.gc_id.as_str()));
    assert!(text.contains(&output.confirmation));
    assert!(text.contains("eligible=1"));
    assert!(output.plan.candidates.iter().any(|candidate| {
        candidate.class == GcClass::Rebuildable
            && candidate.content_hash == hash_vault_bytes(b"derived")
    }));

    let mut writer = JsonlWriter::new(Vec::new());
    writer.write_json(&output).expect("JSONL");
    let line = String::from_utf8(writer.into_inner()).expect("UTF-8");
    let decoded: GcPlanOutput = serde_json::from_str(line.trim()).expect("decode");
    assert_eq!(decoded, output);
    assert!(lint_vault(&vault).is_clean());
}

#[test]
fn vault_lint_rendering_sanitizes_controls_and_clap_preserves_typed_action() {
    let arguments = base().into_iter().chain(["lint"]);
    let command = Cli::try_parse_from(arguments).expect("lint route").command;
    assert!(matches!(
        command,
        CliCommand::Vault(args) if matches!(args.action, VaultAction::Lint)
    ));

    let project = tempfile::tempdir().expect("project");
    let root = tempfile::tempdir().expect("vault");
    let vault = ProjectVault::bootstrap(
        project.path(),
        root.path(),
        ProjectId::new("project-1").expect("project ID"),
        1,
    )
    .expect("vault");
    std::fs::write(vault.root().join("wiki/decisions/bad.md"), b"bad").expect("bad page");
    let rendered = EventRenderer::vault_lint(&lint_vault(&vault));
    assert!(rendered.contains("WikiPageInvalid"));
    assert!(!rendered.contains('\u{1b}'));

    let arguments = base().into_iter().chain(["gc", "report"]);
    assert!(matches!(
        Cli::try_parse_from(arguments).expect("gc route").command,
        CliCommand::Vault(args)
            if matches!(args.action, VaultAction::Gc { action: VaultGcAction::Report })
    ));
}
