use std::time::{SystemTime, UNIX_EPOCH};

use minimax_core::{SessionCommand, SessionEffect, SessionMachine};
use minimax_protocol::{RecordId, SessionRecordV1, TurnStatus};

use super::RuntimeStoreError;
use super::journal::stable_hash;

pub(crate) fn recover_abandoned_turns(
    machine: &mut SessionMachine,
) -> Result<Vec<SessionRecordV1>, RuntimeStoreError> {
    let abandoned = machine
        .sessions()
        .values()
        .flat_map(|session| {
            session.turns.iter().map(|turn| {
                (
                    session.session_id.clone(),
                    turn.turn_id.clone(),
                    turn.assistant_message
                        .as_ref()
                        .map(|message| message.content.clone()),
                    turn.status,
                )
            })
        })
        .filter(|(_, _, _, status)| *status == TurnStatus::Running)
        .collect::<Vec<_>>();
    let mut records = Vec::new();
    for (session_id, turn_id, partial_assistant, _) in abandoned {
        let identity = format!("{}:{}", session_id.as_str(), turn_id.as_str());
        let digest = stable_hash(identity.as_bytes());
        let record_id = RecordId::new(format!("recovery-{digest:016x}"))
            .map_err(|_| RuntimeStoreError::Recovery)?;
        let effects = machine
            .apply(SessionCommand::Recover {
                record_id,
                turn_id,
                partial_assistant,
                now_unix_ms: now_unix_ms()?,
            })
            .map_err(|_| RuntimeStoreError::Recovery)?;
        let record = effects
            .into_iter()
            .find_map(|effect| match effect {
                SessionEffect::Persist(record) => Some(record),
                _ => None,
            })
            .ok_or(RuntimeStoreError::Recovery)?;
        records.push(record);
    }
    Ok(records)
}

fn now_unix_ms() -> Result<u64, RuntimeStoreError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RuntimeStoreError::Recovery)?
        .as_millis();
    u64::try_from(millis).map_err(|_| RuntimeStoreError::Recovery)
}
