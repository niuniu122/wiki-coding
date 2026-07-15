use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use futures_util::StreamExt as _;
use minimax_core::StreamSequence;
use minimax_protocol::{
    ProviderProtocolKind, RuntimeErrorCode, RuntimeFailure, StreamEvent, ToolCallFragment,
    ToolCallId, TurnRequest,
};
use reqwest::redirect::Policy;
use secrecy::{ExposeSecret as _, SecretString};
use tokio_util::sync::CancellationToken;

use crate::{ChatCompletionsAdapter, ResponsesAdapter, SseDecoder};

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

    for event in events {
        if let StreamEvent::ToolCallFragments { fragments } = event {
            tools.accept(&fragments);
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
}

#[derive(Default)]
struct ToolAssembly {
    name: Option<String>,
    arguments: String,
    index: Option<u32>,
}

impl ToolAssembler {
    fn accept(&mut self, fragments: &[ToolCallFragment]) {
        for fragment in fragments {
            let raw_id = fragment.call_id.as_str();
            let actual_id = if raw_id.starts_with("index:") {
                fragment
                    .index
                    .and_then(|index| self.id_by_index.get(&index).cloned())
                    .unwrap_or_else(|| raw_id.to_owned())
            } else {
                if let Some(index) = fragment.index {
                    self.id_by_index.insert(index, raw_id.to_owned());
                    if let Some(provisional) = self.by_id.remove(&format!("index:{index}")) {
                        self.by_id.insert(raw_id.to_owned(), provisional);
                    }
                }
                raw_id.to_owned()
            };
            let assembly = self.by_id.entry(actual_id).or_default();
            assembly.index = fragment.index.or(assembly.index);
            if let Some(name) = &fragment.name {
                assembly.name = Some(name.clone());
            }
            if let Some(arguments) = &fragment.arguments_delta {
                assembly.arguments.push_str(arguments);
            }
        }
    }

    fn flush(&mut self) -> Result<Vec<StreamEvent>, RuntimeFailure> {
        let pending = std::mem::take(&mut self.by_id);
        self.id_by_index.clear();
        pending
            .into_iter()
            .map(|(call_id, assembly)| {
                if call_id.starts_with("index:") || assembly.name.is_none() {
                    return Err(RuntimeFailure::new(RuntimeErrorCode::ProtocolMalformedJson));
                }
                Ok(StreamEvent::ToolCallFragments {
                    fragments: vec![ToolCallFragment {
                        call_id: ToolCallId::new(call_id)
                            .map_err(RuntimeErrorCode::from)
                            .map_err(RuntimeFailure::new)?,
                        name: assembly.name,
                        arguments_delta: Some(assembly.arguments),
                        index: assembly.index,
                    }],
                })
            })
            .collect()
    }
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
