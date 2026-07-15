//! Terminal presentation for the Rust rewrite.
//!
//! The TUI renders protocol state and sends user intent inward. It does not own
//! runtime policy, provider parsing, tool effects, retrieval, or Vault writes.

mod command;
mod render;
mod shell;

pub use command::{
    CommandAvailability, CommandIntent, CommandParseError, ParsedInput, PermissionName, parse_input,
};
pub use render::EventRenderer;
pub use shell::{CrosstermTerminalHooks, InteractiveShell, ShellMode, ShellSession, TerminalHooks};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "terminal rendering and input translation over protocol state";
