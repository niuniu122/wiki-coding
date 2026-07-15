use std::io::{self, Write};

use minimax_protocol::{RuntimeErrorCode, RuntimeEventV1, RuntimeTerminalOutcome, TurnReceipt};
use minimax_vault::RuntimeStoreError;

use crate::{DriverError, RunReport};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum ExitClass {
    Completed = 0,
    Usage = 2,
    Provider = 3,
    Interrupted = 4,
    Workspace = 5,
}

impl ExitClass {
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }
}

pub struct JsonlWriter<W> {
    writer: W,
}

impl<W: Write> JsonlWriter<W> {
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn write_event(&mut self, event: &RuntimeEventV1) -> io::Result<()> {
        serde_json::to_writer(&mut self.writer, event).map_err(io::Error::other)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }

    pub fn write_report(&mut self, report: &RunReport) -> io::Result<()> {
        for event in &report.events {
            self.write_event(event)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn into_inner(self) -> W {
        self.writer
    }
}

#[must_use]
pub fn exit_for_report(report: &RunReport) -> ExitClass {
    exit_for_receipt(&report.receipt)
}

#[must_use]
pub fn exit_for_error(error: &DriverError) -> ExitClass {
    match error {
        DriverError::Runtime(code) => exit_for_code(*code),
        DriverError::Store(RuntimeStoreError::Command(code)) => exit_for_code(*code),
        DriverError::Store(_) | DriverError::Compaction(_) => ExitClass::Workspace,
    }
}

fn exit_for_receipt(receipt: &TurnReceipt) -> ExitClass {
    match &receipt.outcome {
        RuntimeTerminalOutcome::Completed => ExitClass::Completed,
        RuntimeTerminalOutcome::Interrupted | RuntimeTerminalOutcome::Stopped => {
            ExitClass::Interrupted
        }
        RuntimeTerminalOutcome::Failed { failure } => exit_for_code(failure.code),
    }
}

const fn exit_for_code(code: RuntimeErrorCode) -> ExitClass {
    match code {
        RuntimeErrorCode::Configuration | RuntimeErrorCode::CredentialMissing => ExitClass::Usage,
        RuntimeErrorCode::Interrupted => ExitClass::Interrupted,
        RuntimeErrorCode::WorkspaceBusy | RuntimeErrorCode::Recovery => ExitClass::Workspace,
        RuntimeErrorCode::TransportTimeout
        | RuntimeErrorCode::TransportNetwork
        | RuntimeErrorCode::HttpStatus
        | RuntimeErrorCode::ProtocolMalformedJson
        | RuntimeErrorCode::ProtocolPrematureEof
        | RuntimeErrorCode::ProtocolDuplicateTerminal
        | RuntimeErrorCode::ProtocolEventAfterTerminal
        | RuntimeErrorCode::ProtocolUnknownEvent
        | RuntimeErrorCode::ToolUnavailable => ExitClass::Provider,
    }
}
