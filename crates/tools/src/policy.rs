use minimax_protocol::{
    MAX_TOOL_ARGUMENT_BYTES, MAX_TOOL_RESULT_BYTES, ToolDefinition, ToolEffect, ToolInvocation,
    ToolValidationError, V1_TOOL_NAMES,
};
use serde_json::{Value, json};

use crate::error::{ToolDenial, ToolDenialCode};
use crate::path::validate_relative_path;

pub trait CancellationSignal: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NeverCancelled;

impl CancellationSignal for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl CancellationSignal for bool {
    fn is_cancelled(&self) -> bool {
        *self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolSpec {
    pub definition: ToolDefinition,
    pub effect: ToolEffect,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ToolRegistry;

impl ToolRegistry {
    pub fn specs() -> Result<Vec<ToolSpec>, ToolValidationError> {
        V1_TOOL_NAMES
            .iter()
            .map(|name| {
                let (description, parameters, effect) = schema_for(name);
                Ok(ToolSpec {
                    definition: ToolDefinition::new(*name, description, parameters)?,
                    effect,
                })
            })
            .collect()
    }

    pub fn find(name: &str) -> Result<Option<ToolSpec>, ToolValidationError> {
        Ok(Self::specs()?
            .into_iter()
            .find(|spec| spec.definition.name == name))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Preflight;

impl Preflight {
    pub fn check(
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationSignal,
    ) -> Result<ToolSpec, ToolDenial> {
        if cancellation.is_cancelled() {
            return Err(ToolDenial::cancelled());
        }
        if invocation.call.arguments_json.len() > MAX_TOOL_ARGUMENT_BYTES {
            return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
        }
        let spec = ToolRegistry::find(&invocation.call.name)
            .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?
            .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::UnknownTool))?;
        if spec.effect != invocation.effect {
            return Err(ToolDenial::rejected(ToolDenialCode::EffectMismatch));
        }
        spec.definition
            .validate_call(&invocation.call)
            .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
        let arguments = invocation
            .call
            .arguments_value()
            .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
        if !validate_schema_value(
            &Value::Object(arguments.clone()),
            &spec.definition.parameters,
        ) {
            return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
        }
        if let Some(path) = arguments.get("path") {
            let path = path
                .as_str()
                .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
            validate_relative_path(path)?;
            ensure_public_path(path)?;
        }
        scan_argument_content(&arguments)?;
        if cancellation.is_cancelled() {
            return Err(ToolDenial::cancelled());
        }
        Ok(spec)
    }

    pub fn ensure_safe_output(output: &str) -> Result<(), ToolDenial> {
        if output.len() > MAX_TOOL_RESULT_BYTES {
            return Err(ToolDenial::failed(ToolDenialCode::OutputLimit));
        }
        if output.contains('\0') {
            return Err(ToolDenial::failed(ToolDenialCode::BinaryFile));
        }
        if contains_secret(output) {
            return Err(ToolDenial::rejected(ToolDenialCode::SecretContent));
        }
        Ok(())
    }
}

pub(crate) fn ensure_public_path(path: &str) -> Result<(), ToolDenial> {
    let components: Vec<String> = std::path::Path::new(path)
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    if components.iter().any(|component| {
        matches!(
            component.as_str(),
            ".git" | ".minimax" | ".obsidian" | ".minimax-runtime"
        )
    }) {
        return Err(ToolDenial::rejected(ToolDenialCode::ProtectedPath));
    }
    if components.iter().any(|component| is_secret_name(component)) {
        return Err(ToolDenial::rejected(ToolDenialCode::SecretPath));
    }
    Ok(())
}

pub(crate) fn contains_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.contains("-----begin private key-----")
        || lower.contains("-----begin rsa private key-----")
        || lower.contains("-----begin openssh private key-----")
        || lower.contains("github_pat_")
        || lower.contains("ghp_")
        || lower.contains("npm_")
    {
        return true;
    }
    lower.lines().any(|line| {
        let trimmed = line.trim();
        [
            "api_key=",
            "api-key=",
            "client_secret=",
            "access_token=",
            "password=",
            "authorization: bearer ",
        ]
        .iter()
        .any(|prefix| {
            trimmed.strip_prefix(prefix).is_some_and(|secret| {
                let secret = secret.trim_matches(['\"', '\'', ' ']);
                secret.len() >= 12
                    && !matches!(
                        secret,
                        "placeholder" | "example-value" | "your-api-key" | "<redacted>"
                    )
            })
        })
    })
}

fn scan_argument_content(arguments: &serde_json::Map<String, Value>) -> Result<(), ToolDenial> {
    fn visit(value: &Value) -> bool {
        match value {
            Value::String(value) => contains_secret(value),
            Value::Array(values) => values.iter().any(visit),
            Value::Object(values) => values.values().any(visit),
            _ => false,
        }
    }
    if arguments
        .iter()
        .filter(|(key, _)| key.as_str() != "path")
        .any(|(_, value)| visit(value))
    {
        Err(ToolDenial::rejected(ToolDenialCode::SecretContent))
    } else {
        Ok(())
    }
}

fn is_secret_name(component: &str) -> bool {
    component == ".env"
        || component.starts_with(".env.")
        || matches!(
            component,
            ".npmrc"
                | ".pypirc"
                | ".netrc"
                | "id_rsa"
                | "id_ed25519"
                | "credentials"
                | "credentials.json"
                | "secrets.json"
                | "key.pem"
        )
        || component.ends_with(".key")
        || component.ends_with(".p12")
        || component.ends_with(".pfx")
}

fn validate_schema_value(value: &Value, schema: &Value) -> bool {
    let Some(schema) = schema.as_object() else {
        return false;
    };
    if let Some(values) = schema.get("enum").and_then(Value::as_array)
        && !values.contains(value)
    {
        return false;
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {
            let Some(value) = value.as_object() else {
                return false;
            };
            let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
                return false;
            };
            if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false)
                && value.keys().any(|key| !properties.contains_key(key))
            {
                return false;
            }
            if schema
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| {
                    required
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|key| !value.contains_key(key))
                })
            {
                return false;
            }
            value.iter().all(|(key, value)| {
                properties
                    .get(key)
                    .is_some_and(|property| validate_schema_value(value, property))
            })
        }
        Some("array") => {
            let Some(values) = value.as_array() else {
                return false;
            };
            if schema
                .get("minItems")
                .and_then(Value::as_u64)
                .is_some_and(|minimum| values.len() < minimum as usize)
            {
                return false;
            }
            schema.get("items").is_none_or(|items| {
                values
                    .iter()
                    .all(|value| validate_schema_value(value, items))
            })
        }
        Some("string") => {
            let Some(value) = value.as_str() else {
                return false;
            };
            if schema
                .get("minLength")
                .and_then(Value::as_u64)
                .is_some_and(|minimum| value.len() < minimum as usize)
                || schema
                    .get("maxLength")
                    .and_then(Value::as_u64)
                    .is_some_and(|maximum| value.len() > maximum as usize)
            {
                return false;
            }
            match schema.get("pattern").and_then(Value::as_str) {
                Some("^[0-9a-f]{64}$") => {
                    value.len() == 64
                        && value
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }
                Some(_) => false,
                None => true,
            }
        }
        Some("integer") => value.as_u64().is_some_and(|value| {
            !schema
                .get("minimum")
                .and_then(Value::as_u64)
                .is_some_and(|minimum| value < minimum)
                && !schema
                    .get("maximum")
                    .and_then(Value::as_u64)
                    .is_some_and(|maximum| value > maximum)
        }),
        Some("boolean") => value.is_boolean(),
        _ => false,
    }
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "additionalProperties": false,
        "properties": properties,
        "required": required,
        "type": "object"
    })
}

fn schema_for(name: &str) -> (&'static str, Value, ToolEffect) {
    match name {
        "read_file" => (
            "Read one bounded UTF-8 workspace file.",
            object_schema(json!({"path": {"type": "string"}}), &["path"]),
            ToolEffect::Read,
        ),
        "list_directory" => (
            "List one bounded workspace directory.",
            object_schema(json!({"path": {"type": "string"}}), &["path"]),
            ToolEffect::Read,
        ),
        "apply_patch" => (
            "Apply ordered exact edits to one workspace file.",
            object_schema(
                json!({
                    "edits": {
                        "items": {
                            "additionalProperties": false,
                            "properties": {
                                "expected_occurrences": {"minimum": 1, "type": "integer"},
                                "new_text": {"type": "string"},
                                "old_text": {"minLength": 1, "type": "string"}
                            },
                            "required": ["old_text", "new_text", "expected_occurrences"],
                            "type": "object"
                        },
                        "minItems": 1,
                        "type": "array"
                    },
                    "expected_sha256": {"pattern": "^[0-9a-f]{64}$", "type": "string"},
                    "path": {"type": "string"}
                }),
                &["path", "expected_sha256", "edits"],
            ),
            ToolEffect::Write,
        ),
        "write_file" => (
            "Create or conflict-aware replace one workspace file.",
            object_schema(
                json!({
                    "content": {"type": "string"},
                    "expected_sha256": {"pattern": "^[0-9a-f]{64}$", "type": "string"},
                    "mode": {"enum": ["create", "replace"], "type": "string"},
                    "path": {"type": "string"}
                }),
                &["path", "mode", "content"],
            ),
            ToolEffect::Write,
        ),
        "run_diagnostic" => (
            "Run one finite local diagnostic action.",
            object_schema(
                json!({
                    "action": {"enum": ["cargo_check", "cargo_test", "cargo_clippy", "cargo_fmt_check", "node_check", "rg_search"], "type": "string"},
                    "max_results": {"maximum": 500, "minimum": 1, "type": "integer"},
                    "path": {"type": "string"},
                    "pattern": {"maxLength": 1024, "minLength": 1, "type": "string"}
                }),
                &["action"],
            ),
            ToolEffect::Process,
        ),
        "git_status" => (
            "Inspect bounded Git status without mutation.",
            object_schema(json!({"path": {"type": "string"}}), &[]),
            ToolEffect::Process,
        ),
        "git_diff" => (
            "Inspect bounded Git diff without mutation.",
            object_schema(
                json!({
                    "cached": {"type": "boolean"},
                    "path": {"type": "string"}
                }),
                &[],
            ),
            ToolEffect::Process,
        ),
        "npm_diagnostic" => (
            "Run one validated existing npm diagnostic script.",
            object_schema(json!({"script": {"type": "string"}}), &["script"]),
            ToolEffect::Process,
        ),
        _ => unreachable!("V1_TOOL_NAMES contains only registered tools"),
    }
}
