use std::process::{Command, Stdio};
use std::time::Duration;

use minimax_tools::internal_shell_host::HostListener;

#[test]
fn internal_shell_host_is_dispatched_before_clap_and_fails_silently_without_bootstrap() {
    let output = Command::new(env!("CARGO_BIN_EXE_minimax-cli"))
        .arg("--minimax-internal-shell-host")
        .env_remove("MINIMAX_SHELL_HOST_ADDRESS")
        .env_remove("MINIMAX_SHELL_HOST_TOKEN")
        .env_remove("MINIMAX_SHELL_HOST_VERSION")
        .env_remove("MINIMAX_SHELL_HOST_TIMEOUT_MS")
        .output()
        .expect("run hidden shell host entrypoint");

    assert_eq!(output.status.code(), Some(125));
    assert!(
        output.stdout.is_empty(),
        "bootstrap must not write to PTY stdout"
    );
    assert!(
        output.stderr.is_empty(),
        "bootstrap must not write to PTY stderr"
    );
}

#[test]
fn internal_shell_host_flag_is_absent_from_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_minimax-cli"))
        .arg("--help")
        .output()
        .expect("render CLI help");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 help");

    assert!(output.status.success());
    assert!(!stdout.contains("minimax-internal-shell-host"));
}

#[test]
fn internal_shell_host_dispatch_requires_the_exact_single_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_minimax-cli"))
        .args(["--minimax-internal-shell-host", "doctor"])
        .output()
        .expect("run non-exact hidden host invocation");

    assert_eq!(output.status.code(), Some(2));
    assert!(!output.stderr.is_empty());
}

#[test]
fn internal_shell_host_parses_bootstrap_and_authenticates_before_waiting_for_activation() {
    let (listener, bootstrap) =
        HostListener::bind(Duration::from_secs(2)).expect("bind host bootstrap");
    let mut command = Command::new(env!("CARGO_BIN_EXE_minimax-cli"));
    command.args(bootstrap.arguments());
    command.envs(bootstrap.environment());
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = command.spawn().expect("spawn internal shell host");

    let mut parent = listener.accept().expect("host authenticates");
    parent
        .send_activate()
        .expect("activate bootstrap-only host");
    let output = child
        .wait_with_output()
        .expect("wait for bootstrap-only host");

    assert_eq!(output.status.code(), Some(125));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
