use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use minimax_protocol::ShellSessionId;
use portable_pty::{CommandBuilder, MasterPty, PtySize};

use super::{
    PtyBackend, PtyChild, PtyTerminateFuture, ShellManagerError, ShellSessionIdSource,
    ShellSpawnRequest, SpawnedPty,
};

const PROCESS_NONCE_BYTES: usize = 8;

#[derive(Clone, Copy, Debug, Default)]
pub struct NativePtyBackend;

impl PtyBackend for NativePtyBackend {
    fn requires_cursor_handshake(&self) -> bool {
        cfg!(windows)
    }

    fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedPty> {
        let resolved = resolve_native_shell(&request.command)?;
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: request.rows,
                cols: request.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(pty_error)?;
        let mut command = CommandBuilder::new(&resolved.program);
        command.args(&resolved.args);
        command.cwd(&request.cwd);
        let (mut child, reader, writer) = acquire_handles_then_spawn(
            || pair.master.try_clone_reader().map_err(pty_error),
            || pair.master.take_writer().map_err(pty_error),
            || pair.slave.spawn_command(command).map_err(pty_error),
        )?;
        drop(pair.slave);

        let process_id = process_id_or_cleanup(child.as_mut())?;

        Ok(SpawnedPty {
            child: Box::new(NativePtyChild { child, process_id }),
            reader,
            writer,
            guard: Box::new(NativePtyGuard {
                master: Some(pair.master),
                process_id,
                armed: true,
            }),
        })
    }

    fn terminate_tree<'a>(&'a self, process_id: u32) -> PtyTerminateFuture<'a> {
        Box::pin(crate::process::terminate_process_tree(process_id))
    }
}

struct NativePtyChild {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    process_id: u32,
}

impl PtyChild for NativePtyChild {
    fn process_id(&self) -> u32 {
        self.process_id
    }

    fn try_wait(&mut self) -> io::Result<Option<i32>> {
        self.child
            .try_wait()
            .map(|status| status.map(|status| status.exit_code() as i32))
    }

    fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }
}

struct NativePtyGuard {
    master: Option<Box<dyn MasterPty + Send>>,
    process_id: u32,
    armed: bool,
}

impl super::backend::PtyGuard for NativePtyGuard {
    fn close_io(&mut self) {
        drop(self.master.take());
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for NativePtyGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = crate::process::terminate_process_tree_sync(self.process_id);
        }
    }
}

#[derive(Debug)]
pub struct ProcessShellSessionIds {
    nonce: String,
    counter: AtomicU64,
}

impl ProcessShellSessionIds {
    pub fn new() -> Result<Self, ShellManagerError> {
        let mut nonce = [0_u8; PROCESS_NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| ShellManagerError::Identifier)?;
        Ok(Self::from_nonce_and_counter(nonce, 0))
    }

    fn from_nonce_and_counter(nonce: [u8; PROCESS_NONCE_BYTES], counter: u64) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(PROCESS_NONCE_BYTES * 2);
        for byte in nonce {
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self {
            nonce: encoded,
            counter: AtomicU64::new(counter),
        }
    }
}

impl ShellSessionIdSource for ProcessShellSessionIds {
    fn next_session_id(&self) -> Result<ShellSessionId, ShellManagerError> {
        let previous = self
            .counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |counter| {
                counter.checked_add(1)
            })
            .map_err(|_| ShellManagerError::Identifier)?;
        let counter = previous
            .checked_add(1)
            .ok_or(ShellManagerError::Identifier)?;
        ShellSessionId::new(format!("shell-{}-{counter:016x}", self.nonce))
            .map_err(|_| ShellManagerError::Identifier)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ResolvedShell {
    program: PathBuf,
    args: Vec<String>,
}

#[cfg(any(windows, test))]
fn resolve_windows_shell(
    command: &str,
    pwsh_candidates: &[PathBuf],
    powershell_candidate: &Path,
    is_executable: impl Fn(&Path) -> bool,
) -> io::Result<ResolvedShell> {
    let program = pwsh_candidates
        .iter()
        .find(|candidate| is_executable(candidate))
        .cloned()
        .or_else(|| is_executable(powershell_candidate).then(|| powershell_candidate.to_owned()))
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "PowerShell executable not found")
        })?;
    Ok(ResolvedShell {
        program,
        args: vec![
            "-NoLogo".to_owned(),
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            command.to_owned(),
        ],
    })
}

#[cfg(any(target_os = "linux", test))]
fn resolve_linux_shell(
    command: &str,
    requested_shell: Option<&Path>,
    bash_candidate: &Path,
    sh_candidate: &Path,
    is_executable: impl Fn(&Path) -> bool,
) -> io::Result<ResolvedShell> {
    let requested_shell = requested_shell
        .filter(|candidate| is_posix_absolute(candidate) && is_executable(candidate));
    let program = requested_shell
        .or_else(|| is_executable(bash_candidate).then_some(bash_candidate))
        .or_else(|| is_executable(sh_candidate).then_some(sh_candidate))
        .map(Path::to_owned)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "POSIX shell executable not found")
        })?;
    Ok(ResolvedShell {
        program,
        args: vec!["-lc".to_owned(), command.to_owned()],
    })
}

#[cfg(any(target_os = "linux", test))]
fn is_posix_absolute(path: &Path) -> bool {
    path.as_os_str().as_encoded_bytes().first() == Some(&b'/')
}

#[cfg(windows)]
fn resolve_native_shell(command: &str) -> io::Result<ResolvedShell> {
    let pwsh_candidates = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|directory| directory.join("pwsh.exe"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let powershell = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .map(|root| {
            root.join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .unwrap_or_default();
    resolve_windows_shell(command, &pwsh_candidates, &powershell, Path::is_file)
}

#[cfg(target_os = "linux")]
fn resolve_native_shell(command: &str) -> io::Result<ResolvedShell> {
    let requested_shell = std::env::var_os("SHELL").map(PathBuf::from);
    resolve_linux_shell(
        command,
        requested_shell.as_deref(),
        Path::new("/bin/bash"),
        Path::new("/bin/sh"),
        is_executable_for_current_process,
    )
}

#[cfg(target_os = "linux")]
fn is_executable_for_current_process(path: &Path) -> bool {
    is_executable_with_access_check(path, |candidate| {
        rustix::fs::access(candidate, rustix::fs::Access::EXEC_OK).is_ok()
    })
}

#[cfg(any(target_os = "linux", test))]
fn is_executable_with_access_check(path: &Path, check_x_ok: impl FnOnce(&Path) -> bool) -> bool {
    path.is_file() && check_x_ok(path)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn resolve_native_shell(_command: &str) -> io::Result<ResolvedShell> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native PTY shell is supported only on Windows and Linux",
    ))
}

fn pty_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

fn acquire_handles_then_spawn<Child, Reader, Writer>(
    acquire_reader: impl FnOnce() -> io::Result<Reader>,
    acquire_writer: impl FnOnce() -> io::Result<Writer>,
    spawn: impl FnOnce() -> io::Result<Child>,
) -> io::Result<(Child, Reader, Writer)> {
    let reader = acquire_reader()?;
    let writer = acquire_writer()?;
    let child = spawn()?;
    Ok((child, reader, writer))
}

fn process_id_or_cleanup(child: &mut (dyn portable_pty::Child + Send + Sync)) -> io::Result<u32> {
    if let Some(process_id) = child.process_id() {
        return Ok(process_id);
    }

    let kill_error = child.kill().err();
    let wait_error = child.wait().err();
    let cleanup = match (kill_error, wait_error) {
        (None, None) => "direct kill and wait completed".to_owned(),
        (Some(kill), None) => format!("direct kill failed: {kill}; wait completed"),
        (None, Some(wait)) => format!("direct kill completed; wait failed: {wait}"),
        (Some(kill), Some(wait)) => {
            format!("direct kill failed: {kill}; wait failed: {wait}")
        }
    };
    Err(io::Error::other(format!(
        "PTY child did not expose a process ID; {cleanup}"
    )))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::{self, Cursor};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{
        NativePtyBackend, ProcessShellSessionIds, acquire_handles_then_spawn,
        is_executable_with_access_check, process_id_or_cleanup, resolve_linux_shell,
        resolve_windows_shell,
    };
    use crate::shell::{PtyBackend, ShellManagerError, ShellSessionIdSource};

    #[test]
    fn native_backend_requires_startup_cursor_handshake_only_on_windows() {
        assert_eq!(NativePtyBackend.requires_cursor_handshake(), cfg!(windows));
    }

    #[test]
    fn native_startup_acquires_both_fallible_master_handles_before_spawn() {
        type HandleResult = io::Result<((), Cursor<Vec<u8>>, Cursor<Vec<u8>>)>;

        let spawn_count = AtomicUsize::new(0);
        let reader_failure: HandleResult = acquire_handles_then_spawn(
            || Err(io::Error::other("reader acquisition failed")),
            || Ok(Cursor::new(Vec::new())),
            || {
                spawn_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        assert_eq!(
            reader_failure
                .expect_err("reader acquisition must fail")
                .kind(),
            io::ErrorKind::Other
        );

        let writer_failure: HandleResult = acquire_handles_then_spawn(
            || Ok(Cursor::new(Vec::new())),
            || Err(io::Error::other("writer acquisition failed")),
            || {
                spawn_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        assert_eq!(
            writer_failure
                .expect_err("writer acquisition must fail")
                .kind(),
            io::ErrorKind::Other
        );
        assert_eq!(
            spawn_count.load(Ordering::SeqCst),
            0,
            "no child may be spawned until both fallible master handles exist"
        );
    }

    #[derive(Debug)]
    struct MissingPidChild {
        kills: Arc<AtomicUsize>,
        waits: Arc<AtomicUsize>,
    }

    impl portable_pty::ChildKiller for MissingPidChild {
        fn kill(&mut self) -> io::Result<()> {
            self.kills.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
            Box::new(Self {
                kills: Arc::clone(&self.kills),
                waits: Arc::clone(&self.waits),
            })
        }
    }

    impl portable_pty::Child for MissingPidChild {
        fn try_wait(&mut self) -> io::Result<Option<portable_pty::ExitStatus>> {
            Ok(None)
        }

        fn wait(&mut self) -> io::Result<portable_pty::ExitStatus> {
            self.waits.fetch_add(1, Ordering::SeqCst);
            Ok(portable_pty::ExitStatus::with_exit_code(1))
        }

        fn process_id(&self) -> Option<u32> {
            None
        }

        #[cfg(windows)]
        fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
            None
        }
    }

    #[test]
    fn native_startup_missing_process_id_directly_kills_and_waits() {
        let kills = Arc::new(AtomicUsize::new(0));
        let waits = Arc::new(AtomicUsize::new(0));
        let mut child = MissingPidChild {
            kills: Arc::clone(&kills),
            waits: Arc::clone(&waits),
        };

        let error = process_id_or_cleanup(&mut child).expect_err("missing PID must fail startup");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(kills.load(Ordering::SeqCst), 1);
        assert_eq!(waits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn linux_x_ok_result_controls_production_executability_instead_of_mode_bits() {
        let fixture = tempfile::tempdir().expect("x-ok fixture");
        let candidate = fixture.path().join("candidate-shell");
        std::fs::write(&candidate, []).expect("candidate fixture");
        let calls = AtomicUsize::new(0);

        let executable = is_executable_with_access_check(&candidate, |path| {
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(path, candidate);
            false
        });

        assert!(!executable, "a denied X_OK check must remain denied");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_x_ok_rejects_a_file_without_execute_permission() {
        use std::os::unix::fs::PermissionsExt as _;

        let fixture = tempfile::tempdir().expect("x-ok fixture");
        let candidate = fixture.path().join("candidate-shell");
        std::fs::write(&candidate, b"#!/bin/sh\nexit 0\n").expect("candidate fixture");
        let mut permissions = std::fs::metadata(&candidate)
            .expect("candidate metadata")
            .permissions();
        permissions.set_mode(0o000);
        std::fs::set_permissions(&candidate, permissions).expect("candidate permissions");
        assert_eq!(
            std::fs::metadata(&candidate)
                .expect("candidate metadata")
                .permissions()
                .mode()
                & 0o111,
            0,
            "the fixture must not have execute permission"
        );
        assert!(!super::is_executable_for_current_process(&candidate));
    }

    #[test]
    fn native_shell_resolution_windows_prefers_pwsh_then_powershell_and_never_cmd() {
        let fixture = tempfile::tempdir().expect("shell fixture");
        let pwsh = fixture.path().join("pwsh.exe");
        let powershell = fixture.path().join("powershell.exe");
        let cmd = fixture.path().join("cmd.exe");
        for executable in [&pwsh, &powershell, &cmd] {
            std::fs::write(executable, []).expect("shell executable fixture");
        }

        let resolved = resolve_windows_shell(
            "Write-Output ok",
            std::slice::from_ref(&pwsh),
            &powershell,
            Path::is_file,
        )
        .expect("pwsh resolution");
        assert_eq!(resolved.program, pwsh);
        assert_eq!(
            resolved.args,
            ["-NoLogo", "-NoProfile", "-Command", "Write-Output ok"]
        );

        std::fs::remove_file(&resolved.program).expect("remove pwsh fixture");
        let resolved = resolve_windows_shell(
            "Write-Output fallback",
            std::slice::from_ref(&resolved.program),
            &powershell,
            Path::is_file,
        )
        .expect("Windows PowerShell resolution");
        assert_eq!(resolved.program, powershell);
        assert_eq!(
            resolved.args,
            ["-NoLogo", "-NoProfile", "-Command", "Write-Output fallback"]
        );

        std::fs::remove_file(&resolved.program).expect("remove powershell fixture");
        let error = resolve_windows_shell(
            "echo must-not-use-cmd",
            &[fixture.path().join("pwsh.exe")],
            &fixture.path().join("powershell.exe"),
            Path::is_file,
        )
        .expect_err("cmd.exe must never be selected");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(
            cmd.is_file(),
            "cmd fixture proves it was deliberately ignored"
        );
    }

    #[test]
    fn native_shell_resolution_linux_prefers_absolute_executable_shell_then_bash_then_sh() {
        let requested = PathBuf::from("/opt/user/bin/zsh");
        let relative = PathBuf::from("opt/user/bin/fish");
        let bash = PathBuf::from("/bin/bash");
        let sh = PathBuf::from("/bin/sh");

        let executable = HashSet::from([
            requested.clone(),
            relative.clone(),
            bash.clone(),
            sh.clone(),
        ]);
        let resolved = resolve_linux_shell(
            "printf ok",
            Some(requested.as_path()),
            &bash,
            &sh,
            |candidate| executable.contains(candidate),
        )
        .expect("absolute executable SHELL");
        assert_eq!(resolved.program, requested);
        assert_eq!(resolved.args, ["-lc", "printf ok"]);

        let resolved = resolve_linux_shell(
            "printf bash",
            Some(relative.as_path()),
            &bash,
            &sh,
            |candidate| executable.contains(candidate),
        )
        .expect("relative SHELL must be ignored");
        assert_eq!(resolved.program, bash);
        assert_eq!(resolved.args, ["-lc", "printf bash"]);

        let only_sh = HashSet::from([sh.clone()]);
        let resolved = resolve_linux_shell(
            "printf sh",
            Some(Path::new("/missing/shell")),
            Path::new("/missing/bash"),
            &sh,
            |candidate| only_sh.contains(candidate),
        )
        .expect("sh fallback");
        assert_eq!(resolved.program, sh);
        assert_eq!(resolved.args, ["-lc", "printf sh"]);
    }

    #[test]
    fn process_shell_session_ids_report_identifier_when_the_counter_is_exhausted() {
        let ids = ProcessShellSessionIds::from_nonce_and_counter([0xab; 8], u64::MAX);
        assert!(matches!(
            ids.next_session_id(),
            Err(ShellManagerError::Identifier)
        ));
    }
}
