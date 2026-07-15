//! Runtime orchestration and policy for the Rust rewrite.
//!
//! Core coordinates state transitions through protocol contracts. Concrete
//! providers, tools, retrieval engines, Vault storage, and UI code live outside
//! this crate so policy remains testable without side effects.

mod ports;
mod runtime;
mod sequence;
mod session;

pub use ports::{Clock, FixedClock, FixedIdGenerator, IdGenerator};
pub use runtime::{RunEffect, RunInput, RunMachine, RunState};
pub use sequence::{NormalizedReplayRecord, StreamSequence, replay_stream};
pub use session::{SessionCommand, SessionEffect, SessionMachine, SessionSummary};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "runtime orchestration and policy without concrete adapters";
