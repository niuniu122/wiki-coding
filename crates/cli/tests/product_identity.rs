use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn direct_binary_reports_the_rust_package_identity() {
    let output = Command::new(env!("CARGO_BIN_EXE_minimax-cli"))
        .arg("--version")
        .output()
        .expect("Rust CLI version command");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("version output UTF-8"),
        format!("minimax-codex-rust {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn release_smoke_binds_the_launcher_to_the_exact_packaged_identity() {
    let source = std::fs::read_to_string(repo_root().join("scripts/release/verify-rust-release.mjs"))
        .expect("release verifier source");

    for required_contract in [
        "sourceVersionOutput",
        "installedVersionOutput",
        "packagedBinarySha256",
        "missingSiblingRejected",
        "unsafeSiblingRejected",
    ] {
        assert!(
            source.contains(required_contract),
            "release verifier must record {required_contract}"
        );
    }
    assert!(
        source.contains("installedVersionOutput !== sourceVersionOutput"),
        "release verifier must reject direct/installed version drift"
    );
}

#[test]
fn installed_smoke_uses_an_isolated_environment_and_fixed_sibling() {
    let source = std::fs::read_to_string(repo_root().join("scripts/release/verify-rust-release.mjs"))
        .expect("release verifier source");
    let launcher = std::fs::read_to_string(repo_root().join("bin/minimax-codex.cjs"))
        .expect("launcher source");

    assert!(source.contains("releaseSmokeEnvironment"));
    assert!(source.contains("credentialsExcluded: true"));
    assert!(source.contains("pathLookupExcluded: true"));
    assert!(launcher.contains("join(__dirname, \"..\", packagedBinary)"));
    assert!(!launcher.contains("dist/cli.js"));
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .to_path_buf()
}
