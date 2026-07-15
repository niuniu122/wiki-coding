use std::fmt::Write as _;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use minimax_core::CancellationFuture;
use minimax_protocol::{ToolCall, ToolEffect, ToolInvocation, ToolTerminalStatus};
use minimax_tools::{
    ApplyPatchTool, CancellationSignal, ListDirectoryTool, NeverCancelled, ReadFileTool,
    WorkspaceRoot, WriteFileTool,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tempfile::TempDir;

#[test]
fn workspace_tools_read_utf8_and_sort_unicode_entries() {
    let fixture = Fixture::new();
    must(fs::write(fixture.path("龙族.txt"), "路明非"));
    must(fs::write(fixture.path("alpha.txt"), "zero"));

    let read = ReadFileTool::execute(
        &fixture.workspace,
        &invocation("read_file", ToolEffect::Read, json!({"path": "龙族.txt"})),
        &NeverCancelled,
    );
    assert_eq!(read.status, ToolTerminalStatus::Succeeded);
    let read_json: Value = must(serde_json::from_str(must_option(read.output.as_deref())));
    assert_eq!(read_json["content"], "路明非");
    assert_eq!(read_json["bytes"], 9);

    let list = ListDirectoryTool::execute(
        &fixture.workspace,
        &invocation("list_directory", ToolEffect::Read, json!({"path": "."})),
        &NeverCancelled,
    );
    assert_eq!(list.status, ToolTerminalStatus::Succeeded);
    let list_json: Value = must(serde_json::from_str(must_option(list.output.as_deref())));
    let names: Vec<_> = must_option(list_json["entries"].as_array())
        .iter()
        .map(|entry| must_option(entry["name"].as_str()))
        .collect();
    assert_eq!(names, ["alpha.txt", "龙族.txt"]);
}

#[test]
fn read_rejects_binary_nul_secret_and_byte_limit() {
    let fixture = Fixture::new();
    for (name, bytes, code) in [
        ("invalid.bin", vec![0xff, 0xfe], "binary_file"),
        ("contains-nul.txt", b"a\0b".to_vec(), "binary_file"),
        (
            "secret.txt",
            b"-----BEGIN PRIVATE KEY-----\nabc".to_vec(),
            "secret_content",
        ),
        ("large.txt", vec![b'x'; 64 * 1_024 + 1], "input_limit"),
    ] {
        must(fs::write(fixture.path(name), bytes));
        let result = ReadFileTool::execute(
            &fixture.workspace,
            &invocation("read_file", ToolEffect::Read, json!({"path": name})),
            &NeverCancelled,
        );
        assert_eq!(result.code, code);
        assert_ne!(result.status, ToolTerminalStatus::Succeeded);
        assert!(result.output.is_none());
    }
}

#[test]
fn list_rejects_entry_limit_and_secret_names() {
    let fixture = Fixture::new();
    let crowded = fixture.path("crowded");
    must(fs::create_dir(&crowded));
    for index in 0..=500 {
        must(fs::write(crowded.join(format!("entry-{index:03}")), []));
    }
    let crowded_result = ListDirectoryTool::execute(
        &fixture.workspace,
        &invocation(
            "list_directory",
            ToolEffect::Read,
            json!({"path": "crowded"}),
        ),
        &NeverCancelled,
    );
    assert_eq!(crowded_result.code, "entry_limit");

    let secrets = fixture.path("secrets");
    must(fs::create_dir(&secrets));
    must(fs::write(secrets.join(".env"), "x"));
    let secret_result = ListDirectoryTool::execute(
        &fixture.workspace,
        &invocation(
            "list_directory",
            ToolEffect::Read,
            json!({"path": "secrets"}),
        ),
        &NeverCancelled,
    );
    assert_eq!(secret_result.code, "secret_path");
    assert!(secret_result.output.is_none());
}

#[test]
fn write_create_and_replace_are_hash_guarded() {
    let fixture = Fixture::new();
    let create = WriteFileTool::execute(
        &fixture.workspace,
        &invocation(
            "write_file",
            ToolEffect::Write,
            json!({"path": "story.txt", "mode": "create", "content": "alpha"}),
        ),
        &NeverCancelled,
    );
    assert_eq!(create.status, ToolTerminalStatus::Succeeded);
    assert_eq!(must(fs::read_to_string(fixture.path("story.txt"))), "alpha");

    let duplicate_create = WriteFileTool::execute(
        &fixture.workspace,
        &invocation(
            "write_file",
            ToolEffect::Write,
            json!({"path": "story.txt", "mode": "create", "content": "changed"}),
        ),
        &NeverCancelled,
    );
    assert_eq!(duplicate_create.code, "already_exists");
    assert_eq!(must(fs::read_to_string(fixture.path("story.txt"))), "alpha");

    let missing_hash = WriteFileTool::execute(
        &fixture.workspace,
        &invocation(
            "write_file",
            ToolEffect::Write,
            json!({"path": "story.txt", "mode": "replace", "content": "changed"}),
        ),
        &NeverCancelled,
    );
    assert_eq!(missing_hash.code, "invalid_arguments");
    assert_eq!(must(fs::read_to_string(fixture.path("story.txt"))), "alpha");

    let conflict = WriteFileTool::execute(
        &fixture.workspace,
        &invocation(
            "write_file",
            ToolEffect::Write,
            json!({"path": "story.txt", "mode": "replace", "content": "beta", "expected_sha256": "0000000000000000000000000000000000000000000000000000000000000000"}),
        ),
        &NeverCancelled,
    );
    assert_eq!(conflict.code, "hash_conflict");
    assert_eq!(must(fs::read_to_string(fixture.path("story.txt"))), "alpha");

    let replace = WriteFileTool::execute(
        &fixture.workspace,
        &invocation(
            "write_file",
            ToolEffect::Write,
            json!({"path": "story.txt", "mode": "replace", "content": "beta", "expected_sha256": sha256(b"alpha")}),
        ),
        &NeverCancelled,
    );
    assert_eq!(replace.status, ToolTerminalStatus::Succeeded);
    assert_eq!(must(fs::read_to_string(fixture.path("story.txt"))), "beta");
    let receipt: Value = must(serde_json::from_str(must_option(replace.output.as_deref())));
    assert_eq!(receipt["sha256"], sha256(b"beta"));
}

#[test]
fn patch_validates_every_edit_before_atomic_persistence() {
    let fixture = Fixture::new();
    must(fs::write(fixture.path("story.txt"), "路明非 alpha alpha"));
    let original_hash = sha256("路明非 alpha alpha".as_bytes());

    let empty = ApplyPatchTool::execute(
        &fixture.workspace,
        &invocation(
            "apply_patch",
            ToolEffect::Write,
            json!({"path": "story.txt", "expected_sha256": original_hash, "edits": []}),
        ),
        &NeverCancelled,
    );
    assert_eq!(empty.code, "invalid_arguments");

    let duplicate = ApplyPatchTool::execute(
        &fixture.workspace,
        &invocation(
            "apply_patch",
            ToolEffect::Write,
            json!({
                "path": "story.txt",
                "expected_sha256": sha256("路明非 alpha alpha".as_bytes()),
                "edits": [
                    {"old_text": "alpha", "new_text": "beta", "expected_occurrences": 2},
                    {"old_text": "alpha", "new_text": "gamma", "expected_occurrences": 2}
                ]
            }),
        ),
        &NeverCancelled,
    );
    assert_eq!(duplicate.code, "occurrence_conflict");
    assert_eq!(
        must(fs::read_to_string(fixture.path("story.txt"))),
        "路明非 alpha alpha"
    );

    let applied = ApplyPatchTool::execute(
        &fixture.workspace,
        &invocation(
            "apply_patch",
            ToolEffect::Write,
            json!({
                "path": "story.txt",
                "expected_sha256": sha256("路明非 alpha alpha".as_bytes()),
                "edits": [
                    {"old_text": "路明非", "new_text": "楚子航", "expected_occurrences": 1},
                    {"old_text": "alpha", "new_text": "beta", "expected_occurrences": 2}
                ]
            }),
        ),
        &NeverCancelled,
    );
    assert_eq!(applied.status, ToolTerminalStatus::Succeeded);
    assert_eq!(
        must(fs::read_to_string(fixture.path("story.txt"))),
        "楚子航 beta beta"
    );
}

#[test]
fn overlapping_patch_matches_fail_without_mutation() {
    let fixture = Fixture::new();
    must(fs::write(fixture.path("overlap.txt"), "aaa"));
    let result = ApplyPatchTool::execute(
        &fixture.workspace,
        &invocation(
            "apply_patch",
            ToolEffect::Write,
            json!({
                "path": "overlap.txt",
                "expected_sha256": sha256(b"aaa"),
                "edits": [{"old_text": "aa", "new_text": "b", "expected_occurrences": 2}]
            }),
        ),
        &NeverCancelled,
    );
    assert_eq!(result.code, "overlapping_matches");
    assert_eq!(must(fs::read_to_string(fixture.path("overlap.txt"))), "aaa");
}

#[test]
fn cancellation_before_atomic_replace_preserves_original_and_cleans_temp() {
    let fixture = Fixture::new();
    must(fs::write(fixture.path("story.txt"), "alpha"));
    for boundary in 1..=4 {
        let result = WriteFileTool::execute(
            &fixture.workspace,
            &invocation(
                "write_file",
                ToolEffect::Write,
                json!({"path": "story.txt", "mode": "replace", "content": "beta", "expected_sha256": sha256(b"alpha")}),
            ),
            &CancelOnCall::new(boundary),
        );
        assert_eq!(result.status, ToolTerminalStatus::Cancelled);
        assert_eq!(must(fs::read_to_string(fixture.path("story.txt"))), "alpha");
    }
    let names: Vec<_> = must(fs::read_dir(fixture.workspace.as_path()))
        .map(|entry| must(entry).file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["story.txt"]);
}

#[test]
fn cancellation_during_read_returns_no_content() {
    let fixture = Fixture::new();
    must(fs::write(fixture.path("story.txt"), "alpha"));
    for boundary in 1..=4 {
        let result = ReadFileTool::execute(
            &fixture.workspace,
            &invocation("read_file", ToolEffect::Read, json!({"path": "story.txt"})),
            &CancelOnCall::new(boundary),
        );
        assert_eq!(result.status, ToolTerminalStatus::Cancelled);
        assert!(result.output.is_none());
    }
}

#[test]
fn cancellation_during_patch_and_listing_has_no_partial_effect_or_output() {
    let fixture = Fixture::new();
    must(fs::write(fixture.path("story.txt"), "alpha"));
    for boundary in 1..=5 {
        let result = ApplyPatchTool::execute(
            &fixture.workspace,
            &invocation(
                "apply_patch",
                ToolEffect::Write,
                json!({
                    "path": "story.txt",
                    "expected_sha256": sha256(b"alpha"),
                    "edits": [{"old_text": "alpha", "new_text": "beta", "expected_occurrences": 1}]
                }),
            ),
            &CancelOnCall::new(boundary),
        );
        assert_eq!(result.status, ToolTerminalStatus::Cancelled);
        assert_eq!(must(fs::read_to_string(fixture.path("story.txt"))), "alpha");
    }

    let list = ListDirectoryTool::execute(
        &fixture.workspace,
        &invocation("list_directory", ToolEffect::Read, json!({"path": "."})),
        &CancelOnCall::new(3),
    );
    assert_eq!(list.status, ToolTerminalStatus::Cancelled);
    assert!(list.output.is_none());
}

#[test]
fn path_and_content_guards_prevent_final_targets() {
    let fixture = Fixture::new();
    for (path, content, code) in [
        ("../escape.txt", "safe", "invalid_path"),
        (".git/config", "safe", "protected_path"),
        ("credentials.json", "safe", "secret_path"),
        ("safe.txt", "password=abcdefghijklmnop", "secret_content"),
    ] {
        let result = WriteFileTool::execute(
            &fixture.workspace,
            &invocation(
                "write_file",
                ToolEffect::Write,
                json!({"path": path, "mode": "create", "content": content}),
            ),
            &NeverCancelled,
        );
        assert_eq!(result.code, code);
        if !path.contains('/') && path != "safe.txt" {
            assert!(!fixture.path(path).exists());
        }
    }
}

#[test]
fn symlink_escape_is_rejected_when_the_platform_allows_the_fixture() {
    let fixture = Fixture::new();
    let outside = must(TempDir::new());
    must(fs::write(outside.path().join("outside.txt"), "outside"));
    let link = fixture.path("escape.txt");
    if create_file_symlink(&outside.path().join("outside.txt"), &link).is_err() {
        return;
    }
    let result = ReadFileTool::execute(
        &fixture.workspace,
        &invocation("read_file", ToolEffect::Read, json!({"path": "escape.txt"})),
        &NeverCancelled,
    );
    assert_eq!(result.code, "outside_workspace");
    assert!(result.output.is_none());
}

#[cfg(unix)]
#[test]
fn permission_denied_write_preserves_original() {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = Fixture::new();
    let locked = fixture.path("locked");
    must(fs::create_dir(&locked));
    must(fs::write(locked.join("story.txt"), "alpha"));
    must(fs::set_permissions(
        &locked,
        fs::Permissions::from_mode(0o500),
    ));
    let result = WriteFileTool::execute(
        &fixture.workspace,
        &invocation(
            "write_file",
            ToolEffect::Write,
            json!({"path": "locked/story.txt", "mode": "replace", "content": "beta", "expected_sha256": sha256(b"alpha")}),
        ),
        &NeverCancelled,
    );
    must(fs::set_permissions(
        &locked,
        fs::Permissions::from_mode(0o700),
    ));
    assert_eq!(result.code, "io_denied");
    assert_eq!(must(fs::read_to_string(locked.join("story.txt"))), "alpha");
}

struct Fixture {
    _directory: TempDir,
    workspace: WorkspaceRoot,
}

impl Fixture {
    fn new() -> Self {
        let directory = must(TempDir::new());
        let workspace = must(WorkspaceRoot::new(directory.path()));
        Self {
            _directory: directory,
            workspace,
        }
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.workspace.as_path().join(relative)
    }
}

struct CancelOnCall {
    cancel_on: usize,
    calls: AtomicUsize,
}

impl CancelOnCall {
    const fn new(cancel_on: usize) -> Self {
        Self {
            cancel_on,
            calls: AtomicUsize::new(0),
        }
    }
}

impl CancellationSignal for CancelOnCall {
    fn is_cancelled(&self) -> bool {
        self.calls.fetch_add(1, Ordering::SeqCst) + 1 >= self.cancel_on
    }

    fn cancelled<'a>(&'a self) -> CancellationFuture<'a> {
        Box::pin(std::future::pending())
    }
}

fn invocation(name: &str, effect: ToolEffect, arguments: Value) -> ToolInvocation {
    let call = must(ToolCall::new(
        must(minimax_protocol::ToolCallId::new(format!("call-{name}"))),
        name,
        must(serde_json::to_string(&arguments)),
    ));
    must(ToolInvocation::new(call, effect))
}

fn sha256(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(unix)]
fn create_file_symlink(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_file_symlink(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(source, target)
}

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

fn must_option<T>(value: Option<T>) -> T {
    match value {
        Some(value) => value,
        None => panic!("expected value"),
    }
}
