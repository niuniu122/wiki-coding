#![cfg(any(windows, target_os = "linux"))]

#[cfg(windows)]
use std::io::Write as _;
#[cfg(target_os = "linux")]
use std::io::{BufRead as _, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

use minimax_tools::internal_shell_host::{HostEvent, HostListener, RootExit};

#[cfg(windows)]
#[test]
fn real_internal_host_preserves_exit_stdio_and_hides_bootstrap_environment() {
    let secret_command_marker = "internal-command-must-not-be-echoed-71f5650d";
    let shell_command = format!(
        "$m='{secret_command_marker}'; if ([Environment]::CommandLine.Contains($m)) {{ Write-Output 'argv-dirty' }} else {{ Write-Output 'argv-clean' }}; Write-Output 'shell-stdio-ok'; \
         'MINIMAX_SHELL_HOST_ADDRESS','MINIMAX_SHELL_HOST_TOKEN','MINIMAX_SHELL_HOST_VERSION','MINIMAX_SHELL_HOST_TIMEOUT_MS','MINIMAX_SHELL_COMMAND_PATH' | \
         ForEach-Object {{ Write-Output \"$_=$([String]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($_, 'Process')))\" }}; exit 7"
    );
    let mut command_payload = tempfile::Builder::new()
        .prefix("minimax-shell-host-process-")
        .suffix(".ps1")
        .tempfile()
        .expect("stage parent-owned command payload");
    command_payload
        .write_all(shell_command.as_bytes())
        .expect("write command payload");
    command_payload.flush().expect("flush command payload");
    let command_payload = command_payload.into_temp_path();
    let command_payload_path = command_payload.to_path_buf();

    let (listener, bootstrap) =
        HostListener::bind(Duration::from_secs(5)).expect("bind internal host listener");
    let mut command = Command::new(env!("CARGO_BIN_EXE_minimax-shell-test-host"));
    command.args(bootstrap.arguments());
    command.envs(bootstrap.environment());
    command.env("MINIMAX_SHELL_COMMAND_PATH", &command_payload_path);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = command.spawn().expect("spawn trusted test host");

    let mut parent = listener.accept().expect("authenticate test host");
    parent.send_activate().expect("activate contained host");
    assert_eq!(
        parent.recv_event().expect("contained"),
        HostEvent::Contained
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
    assert!(stdout.contains("argv-clean"), "{stdout}");
    assert!(!stdout.contains("argv-dirty"), "{stdout}");
    assert!(stdout.contains("shell-stdio-ok"));
    for key in [
        "MINIMAX_SHELL_HOST_ADDRESS",
        "MINIMAX_SHELL_HOST_TOKEN",
        "MINIMAX_SHELL_HOST_VERSION",
        "MINIMAX_SHELL_HOST_TIMEOUT_MS",
        "MINIMAX_SHELL_COMMAND_PATH",
    ] {
        assert!(stdout.contains(&format!("{key}=True")), "{stdout}");
    }
    assert!(!stdout.contains(secret_command_marker));
    assert!(
        !command_payload_path.exists(),
        "PowerShell bootstrap must delete the parent-owned payload"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn real_linux_host_cleans_a_stdio_closed_double_fork_after_parent_eof() {
    assert_real_linux_host_cleanup(LinuxCleanupTrigger::ParentEof);
}

#[cfg(target_os = "linux")]
#[test]
fn real_linux_host_cleans_a_stdio_closed_double_fork_after_sigterm() {
    assert_real_linux_host_cleanup(LinuxCleanupTrigger::HostSigterm);
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum LinuxCleanupTrigger {
    ParentEof,
    HostSigterm,
}

#[cfg(target_os = "linux")]
fn assert_real_linux_host_cleanup(trigger: LinuxCleanupTrigger) {
    let (listener, bootstrap) =
        HostListener::bind(Duration::from_secs(5)).expect("bind internal host listener");
    let mut command = Command::new(env!("CARGO_BIN_EXE_minimax-shell-test-host"));
    command.args(bootstrap.arguments());
    command.envs(bootstrap.environment());
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().expect("spawn trusted test host");
    let stdout = child.stdout.take().expect("capture host PTY output");
    let (line_tx, line_rx) = std::sync::mpsc::sync_channel(1);
    let output_reader = std::thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        let mut line = String::new();
        let result = stdout.read_line(&mut line).map(|_| line);
        let _ = line_tx.send(result);
        let _ = std::io::copy(&mut stdout, &mut std::io::sink());
    });

    let mut parent = listener.accept().expect("authenticate test host");
    parent.send_activate().expect("activate contained host");
    assert_eq!(
        parent.recv_event().expect("contained"),
        HostEvent::Contained
    );
    parent
        .send_command(linux_double_fork_command())
        .expect("deliver double-fork command");
    assert_eq!(parent.recv_event().expect("ready"), HostEvent::Ready);
    let line = line_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("double-fork fixture reports identities")
        .expect("read double-fork fixture output");
    let identities =
        capture_linux_identities(&line).expect("capture exact double-fork process identities");

    match trigger {
        LinuxCleanupTrigger::ParentEof => drop(parent),
        LinuxCleanupTrigger::HostSigterm => {
            let signal = Command::new("/bin/kill")
                .args(["-TERM", "--", &child.id().to_string()])
                .status()
                .expect("signal trusted Linux host");
            assert!(signal.success(), "{signal:?}");
            assert_eq!(
                parent.recv_event().expect("signal cleanup completion"),
                HostEvent::Done(RootExit::Signal(15))
            );
            drop(parent);
        }
    }
    let host_status = wait_for_child_exit(&mut child, Duration::from_secs(5));
    let descendants = wait_for_linux_identities_to_disappear(&identities, Duration::from_secs(5));
    if host_status.is_err() || descendants.is_err() {
        let _ = child.kill();
        force_kill_linux_identities(&identities);
    }

    let status = host_status.expect("host exits only after EOF cleanup reaches a fixed point");
    assert_eq!(status.code(), Some(143), "{status:?}");
    descendants.expect("EOF cleanup removes the exact root and detached daemon identities");
    output_reader.join().expect("output reader");
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
struct LinuxProcessIdentity {
    process_id: u32,
    start_time: u64,
}

#[cfg(target_os = "linux")]
fn linux_double_fork_command() -> &'static str {
    r#"pidfile="$(mktemp)"; ( setsid sh -c 'pidfile="$1"; ( sh -c '"'"'printf "%s\n" "$$" > "$1"; exec sleep 120'"'"' sh "$pidfile" </dev/null >/dev/null 2>&1 & )' sh "$pidfile" </dev/null >/dev/null 2>&1 & ) & i=0; while test ! -s "$pidfile" && test "$i" -lt 500; do i=$((i + 1)); sleep 0.01; done; child="$(cat "$pidfile")"; rm -f "$pidfile"; printf 'parent=%s;child=%s\n' "$$" "$child"; sleep 120"#
}

#[cfg(target_os = "linux")]
fn capture_linux_identities(output: &str) -> Result<Vec<LinuxProcessIdentity>, String> {
    let mut parent = None;
    let mut child = None;
    for field in output.split([';', '\r', '\n']) {
        if let Some(value) = field.trim().strip_prefix("parent=") {
            parent = value.parse::<u32>().ok();
        }
        if let Some(value) = field.trim().strip_prefix("child=") {
            child = value.parse::<u32>().ok();
        }
    }
    [parent, child]
        .into_iter()
        .map(|process_id| {
            let process_id =
                process_id.ok_or_else(|| format!("missing process identity in {output:?}"))?;
            let start_time = linux_process_start_time(process_id)
                .ok_or_else(|| format!("process {process_id} disappeared before capture"))?;
            Ok(LinuxProcessIdentity {
                process_id,
                start_time,
            })
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn linux_process_start_time(process_id: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{process_id}/stat")).ok()?;
    let close = stat.rfind(')')?;
    stat[close + 1..].split_whitespace().nth(19)?.parse().ok()
}

#[cfg(target_os = "linux")]
fn linux_identity_is_alive(identity: LinuxProcessIdentity) -> bool {
    linux_process_start_time(identity.process_id) == Some(identity.start_time)
}

#[cfg(target_os = "linux")]
fn wait_for_linux_identities_to_disappear(
    identities: &[LinuxProcessIdentity],
    timeout: Duration,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let survivors = identities
            .iter()
            .copied()
            .filter(|identity| linux_identity_is_alive(*identity))
            .collect::<Vec<_>>();
        if survivors.is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("surviving Linux identities: {survivors:?}"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
fn force_kill_linux_identities(identities: &[LinuxProcessIdentity]) {
    for identity in identities {
        if !linux_identity_is_alive(*identity) {
            continue;
        }
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &identity.process_id.to_string()])
            .status();
    }
}

#[cfg(target_os = "linux")]
fn wait_for_child_exit(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus, String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => return Err("trusted Linux host did not exit after EOF cleanup".to_owned()),
            Err(error) => return Err(format!("trusted Linux host wait failed: {error}")),
        }
    }
}
