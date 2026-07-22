#![cfg(windows)]

use std::process::{Command, Stdio};
use std::time::Duration;

use minimax_tools::internal_shell_host::{HostEvent, HostListener, RootExit};

#[test]
fn real_internal_host_preserves_exit_stdio_and_hides_bootstrap_environment() {
    let (listener, bootstrap) =
        HostListener::bind(Duration::from_secs(5)).expect("bind internal host listener");
    let mut command = Command::new(env!("CARGO_BIN_EXE_minimax-shell-test-host"));
    command.args(bootstrap.arguments());
    command.envs(bootstrap.environment());
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = command.spawn().expect("spawn trusted test host");

    let mut parent = listener.accept().expect("authenticate test host");
    parent.send_activate().expect("activate contained host");
    assert_eq!(
        parent.recv_event().expect("contained"),
        HostEvent::Contained
    );
    let secret_command_marker = "internal-command-must-not-be-echoed-71f5650d";
    let shell_command = format!(
        "$m='{secret_command_marker}'; Write-Output 'shell-stdio-ok'; \
         'MINIMAX_SHELL_HOST_ADDRESS','MINIMAX_SHELL_HOST_TOKEN','MINIMAX_SHELL_HOST_VERSION','MINIMAX_SHELL_HOST_TIMEOUT_MS' | \
         ForEach-Object {{ Write-Output \"$_=$([String]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($_, 'Process')))\" }}; exit 7"
    );
    parent
        .send_command(&shell_command)
        .expect("deliver command");
    assert_eq!(parent.recv_event().expect("ready"), HostEvent::Ready);
    assert_eq!(
        parent.recv_event().expect("done"),
        HostEvent::Done(RootExit::Code(7))
    );
    let output = child.wait_with_output().expect("wait for test host");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 shell output");

    assert_eq!(output.status.code(), Some(7));
    assert!(output.stderr.is_empty(), "host bootstrap must stay silent");
    assert!(stdout.contains("shell-stdio-ok"));
    for key in [
        "MINIMAX_SHELL_HOST_ADDRESS",
        "MINIMAX_SHELL_HOST_TOKEN",
        "MINIMAX_SHELL_HOST_VERSION",
        "MINIMAX_SHELL_HOST_TIMEOUT_MS",
    ] {
        assert!(stdout.contains(&format!("{key}=True")), "{stdout}");
    }
    assert!(!stdout.contains(secret_command_marker));
}
