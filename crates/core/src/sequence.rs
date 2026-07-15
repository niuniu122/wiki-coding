use minimax_protocol::{ProtocolErrorCode, StreamEvent, StreamEventV1, TerminalOutcome, TurnId};
use serde::{Deserialize, Serialize};

use crate::{Clock, IdGenerator};

/// Provider-neutral reducer that accepts exactly one terminal event.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StreamSequence {
    events: Vec<StreamEvent>,
    terminal: Option<TerminalOutcome>,
}

impl StreamSequence {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            events: Vec::new(),
            terminal: None,
        }
    }

    pub fn accept(&mut self, event: StreamEvent) -> Result<(), ProtocolErrorCode> {
        if self.terminal.is_some() {
            return if event.is_terminal() {
                Err(ProtocolErrorCode::DuplicateTerminal)
            } else {
                Err(ProtocolErrorCode::EventAfterTerminal)
            };
        }

        if let StreamEvent::Terminal { outcome } = &event {
            self.terminal = Some(outcome.clone());
        }
        self.events.push(event);
        Ok(())
    }

    pub fn finish_eof(&self) -> Result<&TerminalOutcome, ProtocolErrorCode> {
        self.terminal
            .as_ref()
            .ok_or(ProtocolErrorCode::PrematureEof)
    }

    #[must_use]
    pub fn events(&self) -> &[StreamEvent] {
        &self.events
    }
}

/// Deterministic evidence emitted by an offline fixture replay.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NormalizedReplayRecord {
    pub replay_id: TurnId,
    pub recorded_at_unix_ms: u64,
    pub events: Vec<StreamEventV1>,
    pub terminal: TerminalOutcome,
}

pub fn replay_stream(
    events: impl IntoIterator<Item = StreamEvent>,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<NormalizedReplayRecord, ProtocolErrorCode> {
    let mut sequence = StreamSequence::new();
    for event in events {
        sequence.accept(event)?;
    }
    let terminal = sequence.finish_eof()?.clone();

    Ok(NormalizedReplayRecord {
        replay_id: TurnId::new(ids.next_id("replay"))?,
        recorded_at_unix_ms: clock.now_unix_ms(),
        events: sequence
            .events()
            .iter()
            .cloned()
            .map(StreamEventV1::new)
            .collect(),
        terminal,
    })
}
