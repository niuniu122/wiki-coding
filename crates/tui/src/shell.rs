use std::io;

use is_terminal::IsTerminal as _;

pub trait TerminalHooks {
    fn enable_raw_mode(&self) -> io::Result<()>;
    fn disable_raw_mode(&self) -> io::Result<()>;
}

pub struct CrosstermTerminalHooks;

impl TerminalHooks for CrosstermTerminalHooks {
    fn enable_raw_mode(&self) -> io::Result<()> {
        #[cfg(not(target_abi = "llvm"))]
        {
            crossterm::terminal::enable_raw_mode()
        }
        #[cfg(target_abi = "llvm")]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "raw terminal mode is unavailable on this development fallback target",
            ))
        }
    }

    fn disable_raw_mode(&self) -> io::Result<()> {
        #[cfg(not(target_abi = "llvm"))]
        {
            crossterm::terminal::disable_raw_mode()
        }
        #[cfg(target_abi = "llvm")]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "raw terminal mode is unavailable on this development fallback target",
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellMode {
    Raw,
    Line,
}

pub struct InteractiveShell<'a> {
    hooks: &'a dyn TerminalHooks,
    input_is_terminal: bool,
    output_is_terminal: bool,
}

impl<'a> InteractiveShell<'a> {
    #[must_use]
    pub fn from_stdio(hooks: &'a dyn TerminalHooks) -> Self {
        Self {
            hooks,
            input_is_terminal: std::io::stdin().is_terminal(),
            output_is_terminal: std::io::stdout().is_terminal(),
        }
    }

    #[must_use]
    pub const fn with_capabilities(
        hooks: &'a dyn TerminalHooks,
        input_is_terminal: bool,
        output_is_terminal: bool,
    ) -> Self {
        Self {
            hooks,
            input_is_terminal,
            output_is_terminal,
        }
    }

    pub fn begin(&self) -> io::Result<ShellSession<'a>> {
        if self.input_is_terminal && self.output_is_terminal {
            self.hooks.enable_raw_mode()?;
            Ok(ShellSession {
                mode: ShellMode::Raw,
                guard: Some(RawModeGuard { hooks: self.hooks }),
            })
        } else {
            Ok(ShellSession {
                mode: ShellMode::Line,
                guard: None,
            })
        }
    }
}

pub struct ShellSession<'a> {
    mode: ShellMode,
    guard: Option<RawModeGuard<'a>>,
}

impl ShellSession<'_> {
    #[must_use]
    pub const fn mode(&self) -> ShellMode {
        self.mode
    }

    #[must_use]
    pub const fn raw_mode_is_guarded(&self) -> bool {
        self.guard.is_some()
    }
}

struct RawModeGuard<'a> {
    hooks: &'a dyn TerminalHooks,
}

impl Drop for RawModeGuard<'_> {
    fn drop(&mut self) {
        let _ = self.hooks.disable_raw_mode();
    }
}
