use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;

use minimax_cli::{DriverIds, ExitClass, ProviderPort, RuntimeDriver, exit_for_report};
use minimax_core::CompactionBudget;
use minimax_protocol::{
    ModelBinding, ModelId, ProviderId, ProviderProtocolKind, RuntimeErrorCode, RuntimeFailure,
    RuntimeTerminalOutcome, StreamEvent, TerminalOutcome, TurnId, TurnStatus, Usage,
};
use minimax_vault::{RuntimeStore, RuntimeStoreError};
use tokio_util::sync::CancellationToken;

enum MockRun {
    Events(Vec<StreamEvent>),
    WaitForCancellation,
}

struct MockProvider {
    runs: VecDeque<MockRun>,
}

impl MockProvider {
    fn completed(contents: &[&str]) -> MockRun {
        let mut events = contents
            .iter()
            .map(|delta| StreamEvent::VisibleTextDelta {
                delta: (*delta).to_owned(),
            })
            .collect::<Vec<_>>();
        events.push(StreamEvent::Usage {
            usage: Usage {
                input_tokens: Some(3),
                output_tokens: Some(2),
                total_tokens: Some(5),
            },
        });
        events.push(StreamEvent::Terminal {
            outcome: TerminalOutcome::Completed,
        });
        MockRun::Events(events)
    }
}

impl ProviderPort for MockProvider {
    fn stream<'a>(
        &'a mut self,
        _request: &'a minimax_protocol::TurnRequest,
        cancellation: &'a CancellationToken,
        emit: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<(), RuntimeFailure>> + Send + 'a>> {
        Box::pin(async move {
            match self
                .runs
                .pop_front()
                .ok_or_else(|| RuntimeFailure::new(RuntimeErrorCode::ProtocolPrematureEof))?
            {
                MockRun::Events(events) => {
                    for event in events {
                        emit(event);
                        tokio::task::yield_now().await;
                    }
                    Ok(())
                }
                MockRun::WaitForCancellation => {
                    cancellation.cancelled().await;
                    Err(RuntimeFailure::new(RuntimeErrorCode::Interrupted))
                }
            }
        })
    }
}

#[tokio::test]
async fn conversation_reconstructs_then_lists_resumes_continues_retries_and_compacts() {
    let project = tempfile::tempdir().expect("temporary project");
    let first_session;
    let first_turn;
    {
        let provider = MockProvider {
            runs: VecDeque::from([MockProvider::completed(&["first ", "answer"])]),
        };
        let mut driver = RuntimeDriver::open(
            project.path(),
            binding(),
            provider,
            DriverIds::new("process-one", 10_000),
        )
        .expect("first process");
        first_session = driver.active_session_id().expect("active session");
        let report = driver
            .run_prompt("first prompt", 128)
            .await
            .expect("first turn");
        first_turn = report.receipt.turn_id.clone();
        assert_eq!(driver.latest_retryable_turn_id(), Some(first_turn.clone()));
        assert_eq!(exit_for_report(&report), ExitClass::Completed);
        assert_eq!(driver.list_sessions().expect("list").len(), 1);

        let second_session = driver.create_session(binding()).expect("new session");
        assert_ne!(first_session, second_session);
        driver.resume(first_session.clone()).expect("resume first");
        assert_eq!(driver.list_sessions().expect("list two").len(), 2);
        assert!(matches!(
            RuntimeStore::open(project.path()),
            Err(RuntimeStoreError::Busy)
        ));
    }

    {
        let provider = MockProvider {
            runs: VecDeque::from([
                MockProvider::completed(&["continued"]),
                MockProvider::completed(&["retried"]),
            ]),
        };
        let mut driver = RuntimeDriver::open(
            project.path(),
            binding(),
            provider,
            DriverIds::new("process-two", 20_000),
        )
        .expect("restarted process");
        assert_eq!(driver.active_session_id(), Some(first_session.clone()));
        let reconstructed = driver
            .session(&first_session)
            .expect("reconstructed session");
        assert_eq!(reconstructed.turns.len(), 1);
        assert_eq!(
            reconstructed.turns[0]
                .assistant_message
                .as_ref()
                .expect("assistant")
                .content,
            "first answer"
        );

        driver
            .run_prompt("second prompt", 128)
            .await
            .expect("continued turn");
        let retry = driver
            .retry_turn(first_turn.clone(), 128)
            .await
            .expect("retry turn");
        let compact = driver
            .compact_active(CompactionBudget {
                max_record_bytes: 64 * 1024,
                retain_recent_turns: 2,
            })
            .expect("local compaction");
        assert_eq!(compact.retained_recent_turns.len(), 2);
        assert_ne!(retry.receipt.turn_id, first_turn);
    }

    let store = RuntimeStore::open(project.path()).expect("third process recovery");
    let session = store
        .machine()
        .sessions()
        .get(&first_session)
        .expect("persisted first session");
    assert_eq!(session.turns.len(), 3);
    assert!(session.turns.iter().all(|turn| turn.status.is_terminal()));
    assert!(session.compaction.is_some());
}

#[tokio::test]
async fn retry_and_continue_execute_distinct_durable_outcomes() {
    let project = tempfile::tempdir().expect("temporary project");
    let provider = MockProvider {
        runs: VecDeque::from([
            MockProvider::completed(&["source answer"]),
            MockProvider::completed(&["continued answer"]),
            MockProvider::completed(&["retried answer"]),
        ]),
    };
    let mut driver = RuntimeDriver::open(
        project.path(),
        binding(),
        provider,
        DriverIds::new("retry-continue", 25_000),
    )
    .expect("driver");

    let source = driver
        .run_prompt("source prompt", 128)
        .await
        .expect("source turn");
    let session_id = source.receipt.session_id.clone();
    let source_turn_id = source.receipt.turn_id.clone();
    let source_before = driver.session(&session_id).expect("session").turns[0].clone();

    let continued = driver
        .run_prompt("continue with a new prompt", 128)
        .await
        .expect("continued turn");
    let retried = driver
        .retry_turn(source_turn_id.clone(), 128)
        .await
        .expect("retried turn");

    assert_ne!(continued.receipt.turn_id, source_turn_id);
    assert_ne!(retried.receipt.turn_id, source_turn_id);
    assert_ne!(continued.receipt.turn_id, retried.receipt.turn_id);
    assert_ne!(continued.receipt.request_id, source.receipt.request_id);
    assert_ne!(retried.receipt.request_id, source.receipt.request_id);
    assert_ne!(continued.receipt.request_id, retried.receipt.request_id);
    assert_eq!(continued.receipt.outcome, RuntimeTerminalOutcome::Completed);
    assert_eq!(retried.receipt.outcome, RuntimeTerminalOutcome::Completed);

    let turns = &driver.session(&session_id).expect("session").turns;
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[0], source_before, "retry must not rewrite its source");
    assert!(turns[1].retry_of.is_none(), "continue is a normal new turn");
    assert_eq!(
        turns[2].retry_of.as_ref(),
        Some(&source_turn_id),
        "retry records its immutable terminal source"
    );
    assert!(turns.iter().all(|turn| turn.status.is_terminal()));
    drop(driver);

    let replayed = RuntimeStore::open(project.path()).expect("persisted replay");
    let turns = &replayed
        .machine()
        .sessions()
        .get(&session_id)
        .expect("replayed session")
        .turns;
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[0], source_before);
    assert!(turns[1].retry_of.is_none());
    assert_eq!(turns[2].retry_of.as_ref(), Some(&source_turn_id));
    assert!(turns.iter().all(|turn| turn.status.is_terminal()));
}

#[tokio::test]
async fn controlled_cancellation_persists_once_and_releases_lease() {
    let project = tempfile::tempdir().expect("temporary project");
    let session_id;
    let turn_id: TurnId;
    {
        let provider = MockProvider {
            runs: VecDeque::from([MockRun::WaitForCancellation]),
        };
        let mut driver = RuntimeDriver::open(
            project.path(),
            binding(),
            provider,
            DriverIds::new("cancel", 30_000),
        )
        .expect("driver");
        session_id = driver.active_session_id().expect("active session");
        let cancellation = driver.cancellation_token();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            cancellation.cancel();
        });
        let report = driver
            .run_prompt("partial request", 128)
            .await
            .expect("interrupted report");
        turn_id = report.receipt.turn_id.clone();
        assert_eq!(exit_for_report(&report), ExitClass::Interrupted);
        assert_eq!(report.receipt.outcome, RuntimeTerminalOutcome::Interrupted);
    }

    let store = RuntimeStore::open(project.path()).expect("lease released after shutdown");
    let session = store
        .machine()
        .sessions()
        .get(&session_id)
        .expect("recovered session");
    let matching = session
        .turns
        .iter()
        .filter(|turn| turn.turn_id == turn_id)
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].status, TurnStatus::Interrupted);
    assert_eq!(
        matching[0]
            .receipt
            .as_ref()
            .expect("one durable receipt")
            .outcome,
        RuntimeTerminalOutcome::Interrupted
    );
}

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("fixture").expect("provider id"),
        model_id: ModelId::new("fixture-model").expect("model id"),
        protocol: ProviderProtocolKind::Responses,
    }
}
