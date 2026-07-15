use std::collections::BTreeMap;
use std::time::Duration;

use minimax_protocol::{
    ModelBinding, ModelId, OutputSettings, ProviderId, ProviderProtocolKind, RuntimeErrorCode,
};
use serde::{Deserialize, Serialize};

const DEFAULT_PROVIDER_ID: &str = "minimax-official";
const DEFAULT_ENDPOINT: &str = "https://api.minimax.io/v1";
const DEFAULT_MODEL: &str = "MiniMax-M3";
const DEFAULT_ENVIRONMENT_KEY: &str = "MINIMAX_API_KEY";
const DEFAULT_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;
const MIN_TIMEOUT_MS: u64 = 100;
const MAX_TIMEOUT_MS: u64 = 600_000;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigLayer {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub protocol: Option<ProviderProtocolKind>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub environment_key: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub allow_insecure_loopback: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub protocol: Option<ProviderProtocolKind>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub environment_key: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub allow_insecure_loopback: Option<bool>,
}

impl ConfigDocument {
    fn into_layer(self) -> Result<ConfigLayer, RuntimeErrorCode> {
        if self.schema_version != 1 {
            return Err(RuntimeErrorCode::Configuration);
        }
        Ok(ConfigLayer {
            provider_id: self.provider_id,
            endpoint: self.endpoint,
            protocol: self.protocol,
            model: self.model,
            environment_key: self.environment_key,
            timeout_ms: self.timeout_ms,
            max_output_tokens: self.max_output_tokens,
            allow_insecure_loopback: self.allow_insecure_loopback,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    Defaults,
    User,
    Project,
    Environment,
    Cli,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfig {
    pub provider_id: ProviderId,
    pub endpoint: String,
    pub protocol: ProviderProtocolKind,
    pub model_id: ModelId,
    pub environment_key: String,
    pub timeout_ms: u64,
    pub max_output_tokens: u32,
    pub allow_insecure_loopback: bool,
    pub source: ConfigSource,
}

impl ResolvedConfig {
    #[must_use]
    pub fn binding(&self) -> ModelBinding {
        ModelBinding {
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            protocol: self.protocol,
        }
    }

    #[must_use]
    pub const fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

pub fn parse_config_document(raw: &str) -> Result<ConfigDocument, RuntimeErrorCode> {
    let document: ConfigDocument =
        serde_json::from_str(raw).map_err(|_| RuntimeErrorCode::Configuration)?;
    if document.schema_version != 1 {
        return Err(RuntimeErrorCode::Configuration);
    }
    Ok(document)
}

pub fn resolve_config(
    user: Option<ConfigDocument>,
    project: Option<ConfigDocument>,
    environment: &BTreeMap<String, String>,
    cli: ConfigLayer,
) -> Result<ResolvedConfig, RuntimeErrorCode> {
    let mut resolved = MutableConfig::defaults();
    if let Some(user) = user {
        resolved.apply(user.into_layer()?, ConfigSource::User)?;
    }
    if let Some(project) = project {
        resolved.apply(project.into_layer()?, ConfigSource::Project)?;
    }
    let environment = environment_layer(environment)?;
    resolved.apply(environment, ConfigSource::Environment)?;
    resolved.apply(cli, ConfigSource::Cli)?;
    resolved.finish()
}

struct MutableConfig {
    provider_id: String,
    endpoint: String,
    protocol: ProviderProtocolKind,
    model: String,
    environment_key: String,
    timeout_ms: u64,
    max_output_tokens: u32,
    allow_insecure_loopback: bool,
    source: ConfigSource,
}

impl MutableConfig {
    fn defaults() -> Self {
        Self {
            provider_id: DEFAULT_PROVIDER_ID.to_owned(),
            endpoint: DEFAULT_ENDPOINT.to_owned(),
            protocol: ProviderProtocolKind::Responses,
            model: DEFAULT_MODEL.to_owned(),
            environment_key: DEFAULT_ENVIRONMENT_KEY.to_owned(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
            allow_insecure_loopback: false,
            source: ConfigSource::Defaults,
        }
    }

    fn apply(&mut self, layer: ConfigLayer, source: ConfigSource) -> Result<(), RuntimeErrorCode> {
        validate_identity_patch(&layer)?;
        let changed = !layer.is_empty();
        if let Some(value) = layer.provider_id {
            self.provider_id = value;
        }
        if let Some(value) = layer.endpoint {
            self.endpoint = value;
        }
        if let Some(value) = layer.protocol {
            self.protocol = value;
        }
        if let Some(value) = layer.model {
            self.model = value;
        }
        if let Some(value) = layer.environment_key {
            self.environment_key = value;
        }
        if let Some(value) = layer.timeout_ms {
            self.timeout_ms = value;
        }
        if let Some(value) = layer.max_output_tokens {
            self.max_output_tokens = value;
        }
        if let Some(value) = layer.allow_insecure_loopback {
            self.allow_insecure_loopback = value;
        }
        if changed {
            self.source = source;
        }
        Ok(())
    }

    fn finish(self) -> Result<ResolvedConfig, RuntimeErrorCode> {
        let provider_id = ProviderId::new(self.provider_id)?;
        let model_id = ModelId::new(self.model)?;
        validate_environment_key(&self.environment_key)?;
        if !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&self.timeout_ms) {
            return Err(RuntimeErrorCode::Configuration);
        }
        OutputSettings::new(self.max_output_tokens)?;
        let endpoint = normalize_endpoint(&self.endpoint, self.allow_insecure_loopback)?;
        Ok(ResolvedConfig {
            provider_id,
            endpoint,
            protocol: self.protocol,
            model_id,
            environment_key: self.environment_key,
            timeout_ms: self.timeout_ms,
            max_output_tokens: self.max_output_tokens,
            allow_insecure_loopback: self.allow_insecure_loopback,
            source: self.source,
        })
    }
}

impl ConfigLayer {
    fn is_empty(&self) -> bool {
        self.provider_id.is_none()
            && self.endpoint.is_none()
            && self.protocol.is_none()
            && self.model.is_none()
            && self.environment_key.is_none()
            && self.timeout_ms.is_none()
            && self.max_output_tokens.is_none()
            && self.allow_insecure_loopback.is_none()
    }
}

fn validate_identity_patch(layer: &ConfigLayer) -> Result<(), RuntimeErrorCode> {
    if layer.provider_id.is_some() != layer.endpoint.is_some() {
        return Err(RuntimeErrorCode::Configuration);
    }
    Ok(())
}

fn environment_layer(
    environment: &BTreeMap<String, String>,
) -> Result<ConfigLayer, RuntimeErrorCode> {
    Ok(ConfigLayer {
        provider_id: environment.get("MINIMAX_CODEX_PROVIDER").cloned(),
        endpoint: environment.get("MINIMAX_CODEX_ENDPOINT").cloned(),
        protocol: environment
            .get("MINIMAX_CODEX_PROTOCOL")
            .map(|value| match value.as_str() {
                "responses" => Ok(ProviderProtocolKind::Responses),
                "chat_completions" => Ok(ProviderProtocolKind::ChatCompletions),
                _ => Err(RuntimeErrorCode::Configuration),
            })
            .transpose()?,
        model: environment.get("MINIMAX_CODEX_MODEL").cloned(),
        environment_key: environment.get("MINIMAX_CODEX_ENV_KEY").cloned(),
        timeout_ms: parse_environment_number(environment, "MINIMAX_CODEX_TIMEOUT_MS")?,
        max_output_tokens: parse_environment_number(
            environment,
            "MINIMAX_CODEX_MAX_OUTPUT_TOKENS",
        )?,
        allow_insecure_loopback: environment
            .get("MINIMAX_CODEX_ALLOW_INSECURE_LOOPBACK")
            .map(|value| match value.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(RuntimeErrorCode::Configuration),
            })
            .transpose()?,
    })
}

fn parse_environment_number<T>(
    environment: &BTreeMap<String, String>,
    key: &str,
) -> Result<Option<T>, RuntimeErrorCode>
where
    T: std::str::FromStr,
{
    environment
        .get(key)
        .map(|value| {
            value
                .parse::<T>()
                .map_err(|_| RuntimeErrorCode::Configuration)
        })
        .transpose()
}

fn normalize_endpoint(raw: &str, allow_loopback: bool) -> Result<String, RuntimeErrorCode> {
    let mut endpoint = reqwest::Url::parse(raw).map_err(|_| RuntimeErrorCode::Configuration)?;
    if !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(RuntimeErrorCode::Configuration);
    }
    let loopback = endpoint.host_str().is_some_and(is_loopback_host);
    if endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && allow_loopback && loopback)
    {
        return Err(RuntimeErrorCode::Configuration);
    }
    let normalized_path = endpoint.path().trim_end_matches('/').to_owned();
    endpoint.set_path(if normalized_path.is_empty() {
        "/"
    } else {
        &normalized_path
    });
    let mut normalized = endpoint.to_string();
    if endpoint.path() == "/" {
        normalized = normalized.trim_end_matches('/').to_owned();
    }
    Ok(normalized)
}

fn is_loopback_host(host: &str) -> bool {
    if matches!(host, "localhost" | "::1") {
        return true;
    }
    let mut octets = host.split('.');
    matches!(octets.next(), Some("127"))
        && octets.count() == 3
        && host.split('.').all(|part| part.parse::<u8>().is_ok())
}

fn validate_environment_key(value: &str) -> Result<(), RuntimeErrorCode> {
    if value.len() < 2
        || value.len() > 64
        || !value.bytes().enumerate().all(|(index, byte)| {
            (index == 0 && byte.is_ascii_uppercase())
                || (index > 0
                    && (byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'))
        })
    {
        return Err(RuntimeErrorCode::Configuration);
    }
    Ok(())
}
