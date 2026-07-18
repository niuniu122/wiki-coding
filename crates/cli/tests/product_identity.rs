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
    let source =
        std::fs::read_to_string(repo_root().join("scripts/release/verify-rust-release.mjs"))
            .expect("release verifier source");

    for required_contract in [
        "nativeInstalledRustIdentity",
        "installedVersionOutput",
        "packagedBinarySha256",
        "capabilityStatusOutputSha256",
        "productFingerprint",
        "providerCalls: 0",
        "credentialsRead: 0",
        "modelDownloads: 0",
        "missingSiblingRejected",
        "unsafeSiblingRejected",
    ] {
        assert!(
            source.contains(required_contract),
            "release verifier must record {required_contract}"
        );
    }
    assert!(
        source.contains("native and npm installed Rust identities do not match"),
        "release verifier must reject native/npm installed identity drift"
    );
}

#[test]
fn installed_smoke_uses_an_isolated_environment_and_fixed_sibling() {
    let source =
        std::fs::read_to_string(repo_root().join("scripts/release/verify-rust-release.mjs"))
            .expect("release verifier source");
    let launcher = std::fs::read_to_string(repo_root().join("bin/minimax-codex.cjs"))
        .expect("launcher source");

    assert!(source.contains("releaseSmokeEnvironment"));
    assert!(source.contains("credentialsExcluded: true"));
    assert!(source.contains("pathLookupExcluded: true"));
    assert!(launcher.contains("join(__dirname, \"..\", packagedBinary)"));
    assert!(!launcher.contains("dist/cli.js"));
}

#[test]
fn npm_launcher_defines_the_stable_fail_closed_error_taxonomy() {
    let launcher = std::fs::read_to_string(repo_root().join("bin/minimax-codex.cjs"))
        .expect("launcher source");

    for code in [
        "E_UNSUPPORTED_HOST",
        "E_BINARY_MISSING",
        "E_BINARY_UNSAFE",
        "E_BINARY_NOT_EXECUTABLE",
        "E_START_FAILED",
        "E_SIGNAL_TERMINATION",
    ] {
        assert!(
            launcher.contains(code),
            "launcher must define stable error code {code}"
        );
    }
    for forbidden in ["http://", "https://", "fetch(", "process.env", "download"] {
        assert!(
            !launcher.to_ascii_lowercase().contains(forbidden),
            "launcher must not contain fallback/download capability: {forbidden}"
        );
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .to_path_buf()
}
