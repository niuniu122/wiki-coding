use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use futures_util::StreamExt as _;
use minimax_core::StreamSequence;
use minimax_protocol::{
    ProviderProtocolKind, RuntimeErrorCode, RuntimeFailure, StreamEvent, ToolCall,
    ToolCallFragment, ToolCallId, TurnRequest,
};
use reqwest::redirect::Policy;
use secrecy::{ExposeSecret as _, SecretString};
use tokio_util::sync::CancellationToken;

use crate::{
    ChatCompletionsAdapter, ResponsesAdapter, SseDecoder, reasoning_filter::ChatReasoningFilter,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug)]
pub struct HttpProviderClient {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    timeout: Duration,
}

impl HttpProviderClient {
    pub fn new(endpoint: &str, timeout: Option<Duration>) -> Result<Self, RuntimeFailure> {
        let endpoint = validate_endpoint(endpoint)?;
        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(|_| RuntimeFailure::new(RuntimeErrorCode::Configuration))?;
        Ok(Self {
            client,
            endpoint,
            timeout: timeout.unwrap_or(DEFAULT_TIMEOUT),
        })
    }

    pub async fn stream_collect(
        &self,
        request: &TurnRequest,
        api_key: &SecretString,
        cancellation: &CancellationToken,
    ) -> Result<Vec<StreamEvent>, RuntimeFailure> {
        let mut events = Vec::new();
        self.stream_with(request, api_key, cancellation, |event| {
            events.push(event);
            std::future::ready(())
        })
        .await?;
        Ok(events)
    }

    pub async fn stream_with<F, Fut>(
        &self,
        request: &TurnRequest,
        api_key: &SecretString,
        cancellation: &CancellationToken,
        mut publish: F,
    ) -> Result<(), RuntimeFailure>
    where
        F: FnMut(StreamEvent) -> Fut,
        Fut: Future<Output = ()>,
    {
        let (path, body) = match request.protocol {
            ProviderProtocolKind::Responses => (
                ResponsesAdapter::PATH,
                ResponsesAdapter::build_request(request),
            ),
            ProviderProtocolKind::ChatCompletions => (
                ChatCompletionsAdapter::PATH,
                ChatCompletionsAdapter::build_request(request),
            ),
        };
        let url = endpoint_with_path(&self.endpoint, path)?;
        let send = self
            .client
            .post(url)
            .bearer_auth(api_key.expose_secret())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send();

        let deadline = tokio::time::sleep(self.timeout);
        tokio::pin!(deadline);
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(RuntimeFailure::new(RuntimeErrorCode::Interrupted)),
            _ = &mut deadline => return Err(RuntimeFailure::new(RuntimeErrorCode::TransportTimeout)),
            response = send => response.map_err(|_| RuntimeFailure::new(RuntimeErrorCode::TransportNetwork))?,
        };
        if !response.status().is_success() {
            return RuntimeFailure::http(response.status().as_u16())
                .map_or_else(|code| Err(RuntimeFailure::new(code)), Err);
        }

        let mut decoder = SseDecoder::new();
        let mut sequence = StreamSequence::new();
        let mut tools = ToolAssembler::default();
        let mut reasoning = ChatReasoningFilter::default();
        let mut body_stream = response.bytes_stream();
        loop {
            let next = tokio::select! {
                _ = cancellation.cancelled() => return Err(RuntimeFailure::new(RuntimeErrorCode::Interrupted)),
                _ = &mut deadline => return Err(RuntimeFailure::new(RuntimeErrorCode::TransportTimeout)),
                item = body_stream.next() => item,
            };
            let Some(chunk) = next else {
                break;
            };
            let chunk =
                chunk.map_err(|_| RuntimeFailure::new(RuntimeErrorCode::TransportNetwork))?;
            for frame in decoder.push(&chunk).map_err(RuntimeFailure::new)? {
                process_frame(
                    request.protocol,
                    &frame,
                    &mut sequence,
                    &mut tools,
                    &mut reasoning,
                    &mut publish,
                )
                .await?;
            }
        }
        for frame in decoder.finish().map_err(RuntimeFailure::new)? {
            process_frame(
                request.protocol,
                &frame,
                &mut sequence,
                &mut tools,
                &mut reasoning,
                &mut publish,
            )
            .await?;
        }
        flush_tools(&mut tools, &mut sequence, &mut publish).await?;
        sequence
            .finish_eof()
            .map_err(RuntimeErrorCode::from)
            .map_err(RuntimeFailure::new)?;
        Ok(())
    }
}

async fn process_frame<F, Fut>(
    protocol: ProviderProtocolKind,
    frame: &str,
    sequence: &mut StreamSequence,
    tools: &mut ToolAssembler,
    reasoning: &mut ChatReasoningFilter,
    publish: &mut F,
) -> Result<(), RuntimeFailure>
where
    F: FnMut(StreamEvent) -> Fut,
    Fut: Future<Output = ()>,
{
    let events = match protocol {
        ProviderProtocolKind::Responses => ResponsesAdapter::parse_frame(frame),
        ProviderProtocolKind::ChatCompletions => ChatCompletionsAdapter::parse_frame(frame),
    }
    .map_err(RuntimeErrorCode::from)
    .map_err(RuntimeFailure::new)?;

    let events = if protocol == ProviderProtocolKind::ChatCompletions {
        events
            .into_iter()
            .flat_map(|event| reasoning.accept(event))
            .collect()
    } else {
        events
    };

    for event in events {
        if let StreamEvent::ToolCallFragments { fragments } = event {
            tools.accept(&fragments)?;
            continue;
        }
        flush_tools(tools, sequence, publish).await?;
        sequence
            .accept(event.clone())
            .map_err(RuntimeErrorCode::from)
            .map_err(RuntimeFailure::new)?;
        publish(event).await;
    }
    Ok(())
}

async fn flush_tools<F, Fut>(
    tools: &mut ToolAssembler,
    sequence: &mut StreamSequence,
    publish: &mut F,
) -> Result<(), RuntimeFailure>
where
    F: FnMut(StreamEvent) -> Fut,
    Fut: Future<Output = ()>,
{
    for event in tools.flush()? {
        sequence
            .accept(event.clone())
            .map_err(RuntimeErrorCode::from)
            .map_err(RuntimeFailure::new)?;
        publish(event).await;
    }
    Ok(())
}

fn validate_endpoint(raw: &str) -> Result<reqwest::Url, RuntimeFailure> {
    let url = reqwest::Url::parse(raw)
        .map_err(|_| RuntimeFailure::new(RuntimeErrorCode::Configuration))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(RuntimeFailure::new(RuntimeErrorCode::Configuration));
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(RuntimeFailure::new(RuntimeErrorCode::Configuration));
    }
    Ok(url)
}

fn endpoint_with_path(endpoint: &reqwest::Url, path: &str) -> Result<reqwest::Url, RuntimeFailure> {
    let mut url = endpoint.clone();
    url.set_path(&format!(
        "{}{}",
        endpoint.path().trim_end_matches('/'),
        path
    ));
    url.set_query(None);
    Ok(url)
}

#[derive(Default)]
struct ToolAssembler {
    by_id: BTreeMap<String, ToolAssembly>,
    id_by_index: BTreeMap<u32, String>,
    id_by_stream: BTreeMap<String, String>,
    order: Vec<String>,
}

#[derive(Default)]
struct ToolAssembly {
    name: Option<String>,
    arguments: String,
    index: Option<u32>,
}

impl ToolAssembler {
    fn accept(&mut self, fragments: &[ToolCallFragment]) -> Result<(), RuntimeFailure> {
        for fragment in fragments {
            let raw_id = fragment.call_id.as_str();
            let actual_id = if raw_id.starts_with("index:") {
                fragment
                    .index
                    .and_then(|index| self.id_by_index.get(&index).cloned())
                    .unwrap_or_else(|| raw_id.to_owned())
            } else if raw_id.starts_with("stream:") {
                fragment
                    .stream_id
                    .as_ref()
                    .and_then(|stream_id| self.id_by_stream.get(stream_id).cloned())
                    .or_else(|| {
                        fragment
                            .stream_id
                            .as_ref()
                            .filter(|stream_id| self.by_id.contains_key(*stream_id))
                            .cloned()
                    })
                    .unwrap_or_else(|| raw_id.to_owned())
            } else {
                if let Some(index) = fragment.index {
                    self.register_index(index, raw_id)?;
                    self.promote(&format!("index:{index}"), raw_id)?;
                }
                if let Some(stream_id) = &fragment.stream_id {
                    self.register_stream(stream_id, raw_id)?;
                    self.promote(&format!("stream:{stream_id}"), raw_id)?;
                }
                raw_id.to_owned()
            };
            if !self.by_id.contains_key(&actual_id) {
                self.order.push(actual_id.clone());
            }
            let assembly = self.by_id.entry(actual_id).or_default();
            if assembly.index.is_some()
                && fragment.index.is_some()
                && assembly.index != fragment.index
            {
                return Err(protocol_failure());
            }
            assembly.index = fragment.index.or(assembly.index);
            if let Some(name) = &fragment.name {
                if assembly.name.is_some() && !fragment.arguments_complete {
                    return Err(protocol_failure());
                }
                assembly.name = Some(name.clone());
            }
            if let Some(arguments) = &fragment.arguments_delta {
                if fragment.arguments_complete {
                    assembly.arguments.clone_from(arguments);
                } else {
                    assembly.arguments.push_str(arguments);
                }
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<Vec<StreamEvent>, RuntimeFailure> {
        let pending = std::mem::take(&mut self.by_id);
        self.id_by_index.clear();
        self.id_by_stream.clear();
        let order = std::mem::take(&mut self.order);
        order
            .into_iter()
            .map(|call_id| {
                let assembly = pending.get(&call_id).ok_or_else(protocol_failure)?;
                if call_id.starts_with("index:") || call_id.starts_with("stream:") {
                    return Err(protocol_failure());
                }
                let name = assembly.name.clone().ok_or_else(protocol_failure)?;
                let call = ToolCall::new(
                    ToolCallId::new(call_id)
                        .map_err(RuntimeErrorCode::from)
                        .map_err(RuntimeFailure::new)?,
                    name.clone(),
                    assembly.arguments.clone(),
                )
                .map_err(|_| protocol_failure())?;
                Ok(StreamEvent::ToolCallFragments {
                    fragments: vec![ToolCallFragment {
                        call_id: call.call_id,
                        stream_id: None,
                        name: Some(name),
                        arguments_delta: Some(call.arguments_json),
                        arguments_complete: true,
                        index: assembly.index,
                    }],
                })
            })
            .collect()
    }

    fn register_index(&mut self, index: u32, call_id: &str) -> Result<(), RuntimeFailure> {
        if self
            .id_by_index
            .get(&index)
            .is_some_and(|existing| existing != call_id)
        {
            return Err(protocol_failure());
        }
        self.id_by_index.insert(index, call_id.to_owned());
        Ok(())
    }

    fn register_stream(&mut self, stream_id: &str, call_id: &str) -> Result<(), RuntimeFailure> {
        if self
            .id_by_stream
            .get(stream_id)
            .is_some_and(|existing| existing != call_id)
        {
            return Err(protocol_failure());
        }
        self.id_by_stream
            .insert(stream_id.to_owned(), call_id.to_owned());
        Ok(())
    }

    fn promote(&mut self, provisional: &str, actual: &str) -> Result<(), RuntimeFailure> {
        if provisional == actual {
            return Ok(());
        }
        if let Some(assembly) = self.by_id.remove(provisional) {
            if self.by_id.contains_key(actual) {
                return Err(protocol_failure());
            }
            self.by_id.insert(actual.to_owned(), assembly);
            if let Some(position) = self.order.iter().position(|value| value == provisional) {
                self.order[position] = actual.to_owned();
            }
        }
        Ok(())
    }
}

fn protocol_failure() -> RuntimeFailure {
    RuntimeFailure::new(RuntimeErrorCode::ProtocolMalformedJson)
}

#[cfg(test)]
mod tests {
    use super::HttpProviderClient;

    #[test]
    fn endpoint_policy_rejects_insecure_remote_and_url_credentials() {
        assert!(HttpProviderClient::new("http://example.com", None).is_err());
        assert!(HttpProviderClient::new("https://user:pass@example.com", None).is_err());
        assert!(HttpProviderClient::new("http://127.0.0.1:3000", None).is_ok());
        assert!(HttpProviderClient::new("https://example.com", None).is_ok());
    }
}
