//! OS subprocess sandbox construction kept separate from process lifecycle and tool policy.

#[cfg(any(target_os = "linux", test))]
use std::ffi::OsString;
#[cfg(any(target_os = "linux", test))]
use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::process::{ProcessLaunchError, ProcessRequest};

#[cfg(target_os = "linux")]
const BUBBLEWRAP_NAMESPACE_REMEDIATION: &str = "enable unprivileged user namespaces or add a targeted AppArmor userns profile for Bubblewrap; do not disable the sandbox";

#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::io::{Seek as _, Write as _};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd as _;

#[cfg(target_os = "linux")]
pub(crate) fn restricted_command(
    request: &ProcessRequest,
) -> Result<(Command, Option<tempfile::TempDir>, Option<File>), ProcessLaunchError> {
    let bwrap = discover_bubblewrap(request.cwd())?;
    let sandbox_home = tempfile::tempdir().map_err(|_| {
        ProcessLaunchError::sandbox_denied(
            "bubblewrap",
            "linux",
            "make the system temporary directory writable",
        )
    })?;
    let (program, runtime_mounts) = resolve_sandbox_program(request)?;
    let sandbox_request = request.with_program(program.to_string_lossy().into_owned());
    verify_bubblewrap_backend(
        &bwrap,
        &sandbox_request,
        sandbox_home.path(),
        &runtime_mounts,
    )?;
    let sandbox_filter = network_seccomp_filter()?;
    let args = bubblewrap_args(
        &sandbox_request,
        sandbox_home.path(),
        &runtime_mounts,
        Some(sandbox_filter.as_raw_fd()),
    );
    let mut command = Command::new(bwrap);
    command.args(args).env_clear();
    Ok((command, Some(sandbox_home), Some(sandbox_filter)))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn restricted_command(
    _request: &ProcessRequest,
) -> Result<(Command, Option<tempfile::TempDir>, Option<std::fs::File>), ProcessLaunchError> {
    Err(restricted_backend_unavailable())
}

#[cfg(target_os = "windows")]
const fn restricted_backend_unavailable() -> ProcessLaunchError {
    ProcessLaunchError::sandbox_unavailable(
        "windows_native",
        "windows",
        "use full-access only for a trusted project; a native sandbox is unavailable",
    )
}

#[cfg(target_os = "macos")]
const fn restricted_backend_unavailable() -> ProcessLaunchError {
    ProcessLaunchError::sandbox_unavailable(
        "seatbelt",
        "macos",
        "use full-access only for a trusted project; the macOS backend is unavailable",
    )
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
const fn restricted_backend_unavailable() -> ProcessLaunchError {
    ProcessLaunchError::sandbox_unavailable(
        "unsupported",
        "unsupported",
        "use full-access only for a trusted project",
    )
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn bubblewrap_args(
    request: &ProcessRequest,
    sandbox_home: &Path,
    runtime_mounts: &[PathBuf],
    seccomp_fd: Option<i32>,
) -> Vec<OsString> {
    let mut args = Vec::new();
    push_args(
        &mut args,
        &[
            "--unshare-user",
            "--unshare-ipc",
            "--unshare-pid",
            "--unshare-net",
            "--unshare-uts",
            "--unshare-cgroup-try",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
            "--clearenv",
        ],
    );
    if let Some(seccomp_fd) = seccomp_fd {
        push_args(&mut args, &["--seccomp"]);
        args.push(OsString::from(seccomp_fd.to_string()));
    }

    for system_path in ["/usr", "/bin", "/lib", "/lib64", "/sbin"] {
        let path = Path::new(system_path);
        if path.exists() {
            push_path_bind(&mut args, "--ro-bind", path, path);
        }
    }
    push_args(&mut args, &["--dir", "/etc"]);
    let alternatives = Path::new("/etc/alternatives");
    if alternatives.is_dir() {
        push_path_bind(&mut args, "--ro-bind", alternatives, alternatives);
    }
    for system_file in [
        "/etc/ld.so.cache",
        "/etc/passwd",
        "/etc/group",
        "/etc/nsswitch.conf",
    ] {
        let path = Path::new(system_file);
        if path.is_file() {
            push_path_bind(&mut args, "--ro-bind", path, path);
        }
    }

    for runtime_mount in runtime_mounts {
        if runtime_mount.exists() {
            push_parent_directories(&mut args, runtime_mount);
            push_path_bind(&mut args, "--ro-bind", runtime_mount, runtime_mount);
        }
    }

    push_args(&mut args, &["--proc", "/proc", "--dev", "/dev"]);
    push_args(&mut args, &["--tmpfs", "/tmp"]);
    push_path_bind(
        &mut args,
        "--bind",
        sandbox_home,
        Path::new("/tmp/wiki-coding-home"),
    );
    push_path_bind(&mut args, "--bind", request.cwd(), Path::new("/workspace"));
    for protected in [
        ".git",
        ".wiki-coding",
        ".minimax",
        ".obsidian",
        ".minimax-runtime",
    ] {
        let source = request.cwd().join(protected);
        if is_non_symlink_entry(&source) {
            let destination = Path::new("/workspace").join(protected);
            push_path_bind(&mut args, "--ro-bind", &source, &destination);
        }
    }

    let sandbox_path = sandbox_path(runtime_mounts);
    for (key, value) in request.env() {
        if matches!(
            key.as_str(),
            "PATH"
                | "Path"
                | "HOME"
                | "USERPROFILE"
                | "APPDATA"
                | "LOCALAPPDATA"
                | "TEMP"
                | "TMP"
                | "SystemRoot"
                | "ComSpec"
                | "PATHEXT"
        ) {
            continue;
        }
        push_setenv(&mut args, key, value);
    }
    push_setenv(&mut args, "PATH", &sandbox_path);
    push_setenv(&mut args, "HOME", "/tmp/wiki-coding-home");
    push_setenv(&mut args, "TMPDIR", "/tmp");
    let cargo_home = mounted_cargo_home(runtime_mounts)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/tmp/wiki-coding-home/.cargo".to_owned());
    push_setenv(&mut args, "CARGO_HOME", &cargo_home);
    if let Some(rustup_home) = mounted_rustup_home(runtime_mounts) {
        push_setenv(
            &mut args,
            "RUSTUP_HOME",
            rustup_home.to_string_lossy().as_ref(),
        );
    }
    push_args(&mut args, &["--chdir", "/workspace", "--"]);
    args.push(OsString::from(request.program()));
    args.extend(request.args().iter().map(OsString::from));
    args
}

#[cfg(any(target_os = "linux", test))]
fn is_non_symlink_entry(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.file_type().is_symlink())
}

#[cfg(any(target_os = "linux", test))]
fn push_args(args: &mut Vec<OsString>, values: &[&str]) {
    args.extend(values.iter().map(OsString::from));
}

#[cfg(any(target_os = "linux", test))]
fn push_path_bind(args: &mut Vec<OsString>, flag: &str, source: &Path, destination: &Path) {
    args.push(OsString::from(flag));
    args.push(source.as_os_str().to_owned());
    args.push(destination.as_os_str().to_owned());
}

#[cfg(any(target_os = "linux", test))]
fn push_parent_directories(args: &mut Vec<OsString>, path: &Path) {
    let mut parents = path.ancestors().skip(1).collect::<Vec<_>>();
    parents.reverse();
    for parent in parents {
        if parent == Path::new("/") || parent.as_os_str().is_empty() {
            continue;
        }
        args.push(OsString::from("--dir"));
        args.push(parent.as_os_str().to_owned());
    }
}

#[cfg(any(target_os = "linux", test))]
fn push_setenv(args: &mut Vec<OsString>, key: &str, value: &str) {
    push_args(args, &["--setenv", key, value]);
}

#[cfg(any(target_os = "linux", test))]
fn sandbox_path(runtime_mounts: &[PathBuf]) -> String {
    let mut paths = Vec::new();
    for path in runtime_mounts {
        let candidate = if path.file_name().is_some_and(|name| name == "bin") {
            path.clone()
        } else {
            path.join("bin")
        };
        if candidate.is_dir() {
            paths.push(candidate);
        }
    }
    paths.extend([
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);
    std::env::join_paths(paths)
        .ok()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".to_owned())
}

#[cfg(any(target_os = "linux", test))]
fn mounted_cargo_home(runtime_mounts: &[PathBuf]) -> Option<&Path> {
    runtime_mounts.iter().find_map(|path| {
        let name = path.file_name()?.to_str()?;
        let parent = path.parent()?;
        if matches!(name, "bin" | "registry" | "git")
            && parent.file_name().is_some_and(|name| name == ".cargo")
        {
            Some(parent)
        } else {
            None
        }
    })
}

#[cfg(any(target_os = "linux", test))]
fn mounted_rustup_home(runtime_mounts: &[PathBuf]) -> Option<&Path> {
    runtime_mounts
        .iter()
        .find(|path| path.file_name().is_some_and(|name| name == ".rustup"))
        .map(PathBuf::as_path)
}

#[cfg(target_os = "linux")]
pub(crate) fn discover_bubblewrap(workspace: &Path) -> Result<PathBuf, ProcessLaunchError> {
    for candidate in [Path::new("/usr/bin/bwrap"), Path::new("/bin/bwrap")] {
        if !candidate.is_file() {
            continue;
        }
        let canonical = candidate.canonicalize().map_err(ProcessLaunchError::Io)?;
        if !canonical.starts_with(workspace) {
            return Ok(canonical);
        }
    }
    Err(ProcessLaunchError::sandbox_unavailable(
        "bubblewrap",
        "linux",
        "install bubblewrap",
    ))
}

#[cfg(target_os = "linux")]
fn resolve_sandbox_program(
    request: &ProcessRequest,
) -> Result<(PathBuf, Vec<PathBuf>), ProcessLaunchError> {
    let program = Path::new(request.program());
    let resolved = if program.is_absolute() {
        program.to_path_buf()
    } else {
        let path = request.env().get("PATH").ok_or_else(|| {
            ProcessLaunchError::sandbox_denied(
                "bubblewrap",
                "linux",
                "configure a system tool path outside the project",
            )
        })?;
        std::env::split_paths(path)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| {
                ProcessLaunchError::sandbox_denied(
                    "bubblewrap",
                    "linux",
                    "install the requested diagnostic tool outside the project",
                )
            })?
    };
    let canonical = resolved.canonicalize().map_err(|_| {
        ProcessLaunchError::sandbox_denied(
            "bubblewrap",
            "linux",
            "install the requested diagnostic tool outside the project",
        )
    })?;
    if canonical.starts_with(request.cwd()) {
        return Err(ProcessLaunchError::sandbox_denied(
            "bubblewrap",
            "linux",
            "remove project-local executables from PATH",
        ));
    }

    let mut mounts = rust_runtime_mounts(request);
    if !is_system_path(&resolved) {
        let parent = resolved.parent().ok_or_else(|| {
            ProcessLaunchError::sandbox_denied(
                "bubblewrap",
                "linux",
                "install the requested diagnostic tool in a system path",
            )
        })?;
        if is_cargo_bin(parent, request) {
            mounts.push(parent.to_path_buf());
        } else if parent.file_name().is_some_and(|name| name == "bin") {
            mounts.push(parent.parent().unwrap_or(parent).to_path_buf());
        } else {
            mounts.push(parent.to_path_buf());
        }
    }
    mounts.sort();
    mounts.dedup();
    Ok((resolved, mounts))
}

#[cfg(target_os = "linux")]
fn rust_runtime_mounts(request: &ProcessRequest) -> Vec<PathBuf> {
    let home = request.env().get("HOME").map(PathBuf::from);
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".cargo")));
    let rustup_home = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".rustup")));
    let mut mounts = Vec::new();
    if let Some(cargo_home) = cargo_home {
        for relative in ["bin", "registry", "git"] {
            let path = cargo_home.join(relative);
            if path.exists() {
                mounts.push(path);
            }
        }
    }
    if let Some(rustup_home) = rustup_home
        && rustup_home.exists()
    {
        mounts.push(rustup_home);
    }
    mounts
}

#[cfg(target_os = "linux")]
fn is_cargo_bin(path: &Path, request: &ProcessRequest) -> bool {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            request
                .env()
                .get("HOME")
                .map(|home| Path::new(home).join(".cargo"))
        });
    cargo_home.is_some_and(|cargo_home| path == cargo_home.join("bin"))
}

#[cfg(target_os = "linux")]
fn is_system_path(path: &Path) -> bool {
    ["/usr", "/bin", "/lib", "/lib64", "/sbin"]
        .iter()
        .any(|root| path.starts_with(root))
}

#[cfg(target_os = "linux")]
fn network_seccomp_filter() -> Result<File, ProcessLaunchError> {
    let mut filter = tempfile::tempfile().map_err(|_| network_filter_denied())?;
    filter
        .write_all(&network_seccomp_program())
        .and_then(|()| filter.rewind())
        .map_err(|_| network_filter_denied())?;
    rustix::io::fcntl_setfd(&filter, rustix::io::FdFlags::empty())
        .map_err(|_| network_filter_denied())?;
    Ok(filter)
}

#[cfg(target_os = "linux")]
const fn network_filter_denied() -> ProcessLaunchError {
    ProcessLaunchError::sandbox_denied(
        "bubblewrap+seccomp",
        "linux",
        "allow the runtime to create an anonymous seccomp filter",
    )
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn network_seccomp_program() -> Vec<u8> {
    const BPF_LOAD_WORD_ABSOLUTE: u16 = 0x20;
    const BPF_JUMP_EQUAL: u16 = 0x15;
    const BPF_JUMP_GREATER_OR_EQUAL: u16 = 0x35;
    const BPF_RETURN: u16 = 0x06;
    const SECCOMP_DATA_ARCH_OFFSET: u32 = 4;
    const SECCOMP_DATA_SYSCALL_OFFSET: u32 = 0;
    const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
    const SECCOMP_RETURN_KILL_PROCESS: u32 = 0x8000_0000;
    const SECCOMP_RETURN_ERRNO: u32 = 0x0005_0000;
    const SECCOMP_RETURN_ALLOW: u32 = 0x7fff_0000;
    const ERRNO_OPERATION_NOT_PERMITTED: u32 = 1;
    const X32_SYSCALL_BIT: u32 = 0x4000_0000;
    // Keep socketpair available: Rust uses it as the child-exec error channel, and a fresh
    // local pair cannot connect to a host endpoint. The socket syscall remains denied.
    const DENIED_X86_64_SYSCALLS: [u32; 5] = [41, 248, 249, 250, 425];

    let mut instructions = vec![
        ClassicBpfInstruction::statement(BPF_LOAD_WORD_ABSOLUTE, SECCOMP_DATA_ARCH_OFFSET),
        ClassicBpfInstruction::jump(BPF_JUMP_EQUAL, AUDIT_ARCH_X86_64, 1, 0),
        ClassicBpfInstruction::statement(BPF_RETURN, SECCOMP_RETURN_KILL_PROCESS),
        ClassicBpfInstruction::statement(BPF_LOAD_WORD_ABSOLUTE, SECCOMP_DATA_SYSCALL_OFFSET),
        ClassicBpfInstruction::jump(BPF_JUMP_GREATER_OR_EQUAL, X32_SYSCALL_BIT, 0, 1),
        ClassicBpfInstruction::statement(
            BPF_RETURN,
            SECCOMP_RETURN_ERRNO | ERRNO_OPERATION_NOT_PERMITTED,
        ),
    ];
    for syscall in DENIED_X86_64_SYSCALLS {
        instructions.push(ClassicBpfInstruction::jump(BPF_JUMP_EQUAL, syscall, 0, 1));
        instructions.push(ClassicBpfInstruction::statement(
            BPF_RETURN,
            SECCOMP_RETURN_ERRNO | ERRNO_OPERATION_NOT_PERMITTED,
        ));
    }
    instructions.push(ClassicBpfInstruction::statement(
        BPF_RETURN,
        SECCOMP_RETURN_ALLOW,
    ));

    instructions
        .into_iter()
        .flat_map(ClassicBpfInstruction::to_ne_bytes)
        .collect()
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ClassicBpfInstruction {
    code: u16,
    jump_true: u8,
    jump_false: u8,
    value: u32,
}

#[cfg(any(target_os = "linux", test))]
impl ClassicBpfInstruction {
    const fn statement(code: u16, value: u32) -> Self {
        Self {
            code,
            jump_true: 0,
            jump_false: 0,
            value,
        }
    }

    const fn jump(code: u16, value: u32, jump_true: u8, jump_false: u8) -> Self {
        Self {
            code,
            jump_true,
            jump_false,
            value,
        }
    }

    fn to_ne_bytes(self) -> [u8; 8] {
        let code = self.code.to_ne_bytes();
        let value = self.value.to_ne_bytes();
        [
            code[0],
            code[1],
            self.jump_true,
            self.jump_false,
            value[0],
            value[1],
            value[2],
            value[3],
        ]
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn verify_bubblewrap_backend(
    bwrap: &Path,
    request: &ProcessRequest,
    sandbox_home: &Path,
    runtime_mounts: &[PathBuf],
) -> Result<(), ProcessLaunchError> {
    let probe = ProcessRequest::fixed("/bin/true", Vec::new(), request.cwd());
    let filter = network_seccomp_filter()?;
    let status = std::process::Command::new(bwrap)
        .args(bubblewrap_args(
            &probe,
            sandbox_home,
            runtime_mounts,
            Some(filter.as_raw_fd()),
        ))
        .current_dir(request.cwd())
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| {
            ProcessLaunchError::sandbox_denied(
                "bubblewrap",
                "linux",
                BUBBLEWRAP_NAMESPACE_REMEDIATION,
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(ProcessLaunchError::sandbox_denied(
            "bubblewrap",
            "linux",
            BUBBLEWRAP_NAMESPACE_REMEDIATION,
        ))
    }
}
