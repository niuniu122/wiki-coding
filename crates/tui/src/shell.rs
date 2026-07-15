use std::io;

#[cfg(not(target_abi = "llvm"))]
use std::io::Write as _;

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
            match self.hooks.enable_raw_mode() {
                Ok(()) => Ok(ShellSession {
                    mode: ShellMode::Raw,
                    guard: Some(RawModeGuard { hooks: self.hooks }),
                }),
                Err(error) if error.kind() == io::ErrorKind::Unsupported => Ok(ShellSession {
                    mode: ShellMode::Line,
                    guard: None,
                }),
                Err(error) => Err(error),
            }
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

    pub fn read_line(&self) -> io::Result<Option<String>> {
        match self.mode {
            ShellMode::Line => read_standard_line(),
            ShellMode::Raw => read_raw_line(),
        }
    }
}

fn read_standard_line() -> io::Result<Option<String>> {
    let mut line = String::new();
    match io::stdin().read_line(&mut line)? {
        0 => Ok(None),
        _ => Ok(Some(line)),
    }
}

#[cfg(not(target_abi = "llvm"))]
fn read_raw_line() -> io::Result<Option<String>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

    let mut line = String::new();
    loop {
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Enter => {
                println!();
                return Ok(Some(line));
            }
            KeyCode::Backspace => {
                if line.pop().is_some() {
                    print!("\u{8} \u{8}");
                    io::stdout().flush()?;
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "turn interrupted",
                ));
            }
            KeyCode::Char('d')
                if line.is_empty() && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                println!();
                return Ok(None);
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                line.push(character);
                print!("{character}");
                io::stdout().flush()?;
            }
            _ => {}
        }
    }
}

#[cfg(target_abi = "llvm")]
fn read_raw_line() -> io::Result<Option<String>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "raw terminal input is unavailable on this development fallback target",
    ))
}

struct RawModeGuard<'a> {
    hooks: &'a dyn TerminalHooks,
}

impl Drop for RawModeGuard<'_> {
    fn drop(&mut self) {
        let _ = self.hooks.disable_raw_mode();
    }
}
