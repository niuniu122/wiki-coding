# Subprocess Sandbox and Network Boundary

## The short version

User confirmation answers “may this tool run?” It does not by itself answer “what can the child process reach?” wiki-coding therefore keeps approval and subprocess isolation as separate controls.

| Mode | Approval | Child filesystem/network | Intended use |
|------|----------|--------------------------|--------------|
| `confirm` | Required for every external tool call | Linux: Bubblewrap, project workspace is the only writable project view, host-private paths are absent, child network is denied. Missing/unsupported backend: process does not start. | Default, including unfamiliar repositories |
| `full-access` | Skipped for the current process | Subprocess sandbox disabled; child has ordinary host access. Hard tool/schema/path/secret/timeout/output/cancellation gates remain. | Projects you already trust |

Restart always returns to `confirm`. There is no saved “always allow” setting.

## What is inside the sandbox

The four process-backed adapters are Cargo diagnostics, Git status/diff, npm diagnostics, and fixed node/rg checks. On Linux they are started through Bubblewrap before project code can run. The sandbox requires new user, PID, IPC, UTS, and network namespaces, creates a cgroup namespace when the kernel permits it, mounts system runtimes read-only, gives the child a private temporary HOME, mounts the project at `/workspace`, and overlays `.git`, `.wiki-coding`, `.minimax`, `.obsidian`, and `.minimax-runtime` read-only. Symlinked metadata entries are never followed into host paths.

The sandbox intentionally does not pass Bubblewrap's `--disable-userns`. Bubblewrap 0.9 implements that option by denying `clone3`, which prevents Cargo from starting `rustc` on current Ubuntu runners. This follows Codex's Bubblewrap behavior: the child is already inside a new user namespace with all capabilities dropped, while the filesystem mounts, network namespace, and inherited seccomp filter remain in force for descendants.

Rust toolchains and non-credential Cargo cache directories may be mounted read-only so offline checks still work. Cargo credential/config files are not mounted. Child environments remain allowlisted and never receive Provider API credentials.

The runtime executes the complete Bubblewrap-plus-seccomp probe first. The syscall filter denies new sockets (including Unix-domain sockets), socket pairs, io_uring setup, and kernel keyring access; the separate network namespace remains a second layer. Missing Bubblewrap returns `sandbox_unavailable`; a backend that cannot create the namespaces or install the filter returns `sandbox_denied`. Neither error retries the target without a sandbox.

### Ubuntu 24.04 and AppArmor

Ubuntu 24.04 can restrict unprivileged user namespaces through AppArmor. In that state `bwrap --version` succeeds even though Bubblewrap cannot create the user/network namespaces required by the sandbox. `wiki-coding doctor` therefore runs the complete backend probe and reports the backend unavailable instead of treating the installed binary as proof.

On a long-lived machine, keep the global AppArmor restriction enabled and ask the administrator to grant a targeted `userns` profile to the trusted system Bubblewrap executable. Do not switch an unfamiliar project to full access to work around this error. The GitHub Actions workflow uses a temporary sysctl adjustment only inside its disposable Ubuntu runner, then executes a real namespace preflight and the malicious build-script canary.

## What is outside the sandbox

Provider HTTPS is opened by the host Provider adapter, not by a project child process. Denying child networking therefore does not break model calls. BM25-first project discovery, optional embedding reranking, Vault/Wiki Markdown, and direct bounded file tools keep their existing boundaries.

Domain-based child network allowlists are intentionally absent. A trustworthy allowlist needs both a managed proxy and an OS route that project code cannot bypass. Until both exist, the honest choices are `none` in confirm mode and ordinary host network in full access.

## Platform matrix

- Linux x64 GNU: Bubblewrap is required and exercised by a malicious build-script CI canary.
- Windows x64 MSVC: no native restricted-token/WFP backend is bundled yet. Confirm-mode process tools fail closed; full access is explicit and high risk on unknown code.
- macOS: deferred with the rest of macOS product support.

Environment clearing, fixed commands, Windows process-tree cleanup, and Job Objects are useful defenses but are not described as filesystem/network sandboxes.

## Testing an unfamiliar repository

1. Keep `confirm` selected.
2. Run `minimax-codex doctor` and check `subprocess_sandbox`.
3. On Linux, continue only when it reports Bubblewrap enforced.
4. On Windows, use file reads, retrieval, and discussion features; do not switch to full access merely to make an unfamiliar repository's build run.
5. Treat full access like running the repository yourself: its transitive build scripts can read host files and use the host network.
