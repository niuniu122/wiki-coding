use std::time::Duration;

use minimax_protocol::{
    MessageRole, ModelId, ModelMessage, OutputSettings, ProviderId, ProviderProtocolKind,
    RequestId, RuntimeErrorCode, RuntimeTerminalOutcome, SessionId, StreamEvent, TerminalOutcome,
    TurnId, TurnRequest,
};
use minimax_provider::HttpProviderClient;
use secrecy::SecretString;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

fn request(protocol: ProviderProtocolKind) -> TurnRequest {
    TurnRequest {
        session_id: SessionId::new("session-1").expect("valid session"),
        turn_id: TurnId::new("turn-1").expect("valid turn"),
        request_id: RequestId::new("request-1").expect("valid request"),
        provider_id: ProviderId::new("provider:test").expect("valid provider"),
        model_id: ModelId::new("model-test").expect("valid model"),
        protocol,
        messages: vec![ModelMessage {
            role: MessageRole::User,
            content: "hello".to_owned(),
        }],
        output: OutputSettings::new(128).expect("valid output"),
    }
}

async fn fixture_server(
    status: u16,
    body_parts: Vec<Vec<u8>>,
    initial_delay: Duration,
) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback fixture");
    let address = listener.local_addr().expect("fixture address");
    let (request_tx, request_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept fixture request");
        let mut request = vec![0_u8; 16 * 1024];
        let bytes_read = socket.read(&mut request).await.expect("read request");
        request.truncate(bytes_read);
        let _ = request_tx.send(String::from_utf8_lossy(&request).into_owned());
        tokio::time::sleep(initial_delay).await;
        let total_len = body_parts.iter().map(Vec::len).sum::<usize>();
        let reason = match status {
            200 => "OK",
            302 => "Found",
            429 => "Too Many Requests",
            _ => "Fixture",
        };
        let headers = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: text/event-stream\r\nContent-Length: {total_len}\r\nConnection: close\r\n\r\n"
        );
        if socket.write_all(headers.as_bytes()).await.is_err() {
            return;
        }
        for part in body_parts {
            if socket.write_all(&part).await.is_err() {
                return;
            }
            tokio::task::yield_now().await;
        }
        let _ = socket.shutdown().await;
    });
    (format!("http://{address}"), request_rx)
}

fn secret() -> SecretString {
    SecretString::from("synthetic-secret-marker")
}

#[tokio::test]
async fn responses_and_chat_completions_converge_on_safe_stream_events() {
    let cases = [
        (
            ProviderProtocolKind::Responses,
            concat!(
                "data: {\"type\":\"response.reasoning.delta\",\"delta\":\"PRIVATE_REASONING\"}\r\n\r\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"visible\"}\r\n\r\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"total_tokens\":5}}}\r\n\r\n"
            ),
        ),
        (
            ProviderProtocolKind::ChatCompletions,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"PRIVATE_REASONING\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"visible\"}}]}\n\n",
                "data: {\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
                "data: [DONE]\n\n"
            ),
        ),
    ];

    for (protocol, body) in cases {
        let split = body.len() / 3;
        let (endpoint, observed) = fixture_server(
            200,
            vec![
                body.as_bytes()[..split].to_vec(),
                body.as_bytes()[split..].to_vec(),
            ],
            Duration::ZERO,
        )
        .await;
        let client =
            HttpProviderClient::new(&endpoint, Some(Duration::from_secs(2))).expect("valid client");
        let events = client
            .stream_collect(&request(protocol), &secret(), &CancellationToken::new())
            .await
            .expect("fixture stream should complete");
        assert!(matches!(
            events.first(),
            Some(StreamEvent::ReasoningFiltered)
        ));
        assert!(events.iter().any(
            |event| matches!(event, StreamEvent::VisibleTextDelta { delta } if delta == "visible")
        ));
        assert!(matches!(
            events.last(),
            Some(StreamEvent::Terminal {
                outcome: TerminalOutcome::Completed
            })
        ));
        let wire_request = observed.await.expect("captured request");
        assert!(
            wire_request
                .to_ascii_lowercase()
                .contains("authorization: bearer synthetic-secret-marker")
        );
        assert!(wire_request.contains("model-test"));
    }
}

#[tokio::test]
async fn caller_cancellation_and_deadline_are_distinct() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let (endpoint, _) = fixture_server(200, Vec::new(), Duration::from_secs(1)).await;
    let client =
        HttpProviderClient::new(&endpoint, Some(Duration::from_secs(2))).expect("valid client");
    let error = client
        .stream_collect(
            &request(ProviderProtocolKind::Responses),
            &secret(),
            &cancellation,
        )
        .await
        .expect_err("cancelled request should fail");
    assert_eq!(error.code, RuntimeErrorCode::Interrupted);

    let (endpoint, _) = fixture_server(200, Vec::new(), Duration::from_millis(200)).await;
    let client =
        HttpProviderClient::new(&endpoint, Some(Duration::from_millis(20))).expect("valid client");
    let error = client
        .stream_collect(
            &request(ProviderProtocolKind::Responses),
            &secret(),
            &CancellationToken::new(),
        )
        .await
        .expect_err("deadline should fail");
    assert_eq!(error.code, RuntimeErrorCode::TransportTimeout);
}

#[tokio::test]
async fn http_failures_and_redirects_are_status_only_and_never_followed() {
    for status in [302, 429] {
        let (endpoint, _) = fixture_server(
            status,
            vec![b"synthetic-secret-marker PRIVATE_REASONING".to_vec()],
            Duration::ZERO,
        )
        .await;
        let client =
            HttpProviderClient::new(&endpoint, Some(Duration::from_secs(2))).expect("valid client");
        let error = client
            .stream_collect(
                &request(ProviderProtocolKind::Responses),
                &secret(),
                &CancellationToken::new(),
            )
            .await
            .expect_err("non-success status should fail");
        assert_eq!(error.code, RuntimeErrorCode::HttpStatus);
        assert_eq!(error.http_status, Some(status));
        assert!(!format!("{error:?} {error}").contains("PRIVATE_REASONING"));
        assert!(!format!("{error:?} {error}").contains("synthetic-secret-marker"));
    }
}

#[tokio::test]
async fn malformed_premature_duplicate_and_after_terminal_streams_fail_truthfully() {
    let cases = [
        (
            "data: not-json\n\n",
            RuntimeErrorCode::ProtocolMalformedJson,
        ),
        (
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
            RuntimeErrorCode::ProtocolPrematureEof,
        ),
        (
            concat!(
                "data: {\"type\":\"response.completed\"}\n\n",
                "data: {\"type\":\"response.completed\"}\n\n"
            ),
            RuntimeErrorCode::ProtocolDuplicateTerminal,
        ),
        (
            concat!(
                "data: {\"type\":\"response.completed\"}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"late\"}\n\n"
            ),
            RuntimeErrorCode::ProtocolEventAfterTerminal,
        ),
    ];
    for (body, expected) in cases {
        let (endpoint, _) =
            fixture_server(200, vec![body.as_bytes().to_vec()], Duration::ZERO).await;
        let client =
            HttpProviderClient::new(&endpoint, Some(Duration::from_secs(2))).expect("valid client");
        let error = client
            .stream_collect(
                &request(ProviderProtocolKind::Responses),
                &secret(),
                &CancellationToken::new(),
            )
            .await
            .expect_err("invalid stream should fail");
        assert_eq!(error.code, expected);
    }
}

#[test]
fn runtime_terminal_type_remains_provider_neutral() {
    let terminal = RuntimeTerminalOutcome::Completed;
    assert_eq!(
        serde_json::to_string(&terminal).expect("terminal serializes"),
        r#"{"type":"completed"}"#
    );
}
