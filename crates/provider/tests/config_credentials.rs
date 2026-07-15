use std::cell::Cell;
use std::collections::BTreeMap;

use minimax_protocol::{ProviderProtocolKind, RuntimeErrorCode};
use minimax_provider::{
    ConfigDocument, ConfigLayer, ConfigSource, CredentialError, CredentialMode, CredentialResolver,
    CredentialSource, KeyringBackend, parse_config_document, resolve_config,
};

fn document(model: &str) -> ConfigDocument {
    parse_config_document(&format!(r#"{{"schemaVersion":1,"model":"{model}"}}"#)).expect("document")
}

#[test]
fn configuration_precedence_is_cli_environment_project_user_defaults() {
    let environment = BTreeMap::from([(
        "MINIMAX_CODEX_MODEL".to_owned(),
        "environment-model".to_owned(),
    )]);
    let resolved = resolve_config(
        Some(document("user-model")),
        Some(document("project-model")),
        &environment,
        ConfigLayer {
            model: Some("cli-model".to_owned()),
            ..ConfigLayer::default()
        },
    )
    .expect("resolved");
    assert_eq!(resolved.model_id.as_str(), "cli-model");
    assert_eq!(resolved.source, ConfigSource::Cli);

    let resolved = resolve_config(
        Some(document("user-model")),
        Some(document("project-model")),
        &environment,
        ConfigLayer::default(),
    )
    .expect("environment resolved");
    assert_eq!(resolved.model_id.as_str(), "environment-model");
    assert_eq!(resolved.source, ConfigSource::Environment);

    let resolved = resolve_config(
        Some(document("user-model")),
        Some(document("project-model")),
        &BTreeMap::new(),
        ConfigLayer::default(),
    )
    .expect("project resolved");
    assert_eq!(resolved.model_id.as_str(), "project-model");
    assert_eq!(resolved.source, ConfigSource::Project);
}

#[test]
fn strict_documents_and_unsafe_values_fail_before_provider_work() {
    assert_eq!(
        parse_config_document(r#"{"schemaVersion":1,"unknown":true}"#),
        Err(RuntimeErrorCode::Configuration)
    );
    assert_eq!(
        parse_config_document(r#"{"schemaVersion":2}"#),
        Err(RuntimeErrorCode::Configuration)
    );
    for endpoint in [
        "https://user:pass@example.com/v1",
        "https://example.com/v1?secret=x",
        "https://example.com/v1#fragment",
        "http://example.com/v1",
    ] {
        assert!(
            resolve_config(
                None,
                None,
                &BTreeMap::new(),
                ConfigLayer {
                    provider_id: Some("custom".to_owned()),
                    endpoint: Some(endpoint.to_owned()),
                    ..ConfigLayer::default()
                },
            )
            .is_err()
        );
    }
    let loopback = resolve_config(
        None,
        None,
        &BTreeMap::new(),
        ConfigLayer {
            provider_id: Some("local".to_owned()),
            endpoint: Some("http://127.0.0.1:8080/v1/".to_owned()),
            allow_insecure_loopback: Some(true),
            protocol: Some(ProviderProtocolKind::ChatCompletions),
            ..ConfigLayer::default()
        },
    )
    .expect("explicit loopback");
    assert_eq!(loopback.endpoint, "http://127.0.0.1:8080/v1");
    for layer in [
        ConfigLayer {
            provider_id: Some("ambiguous".to_owned()),
            ..ConfigLayer::default()
        },
        ConfigLayer {
            timeout_ms: Some(99),
            ..ConfigLayer::default()
        },
        ConfigLayer {
            max_output_tokens: Some(0),
            ..ConfigLayer::default()
        },
        ConfigLayer {
            environment_key: Some("bad-key".to_owned()),
            ..ConfigLayer::default()
        },
    ] {
        assert!(resolve_config(None, None, &BTreeMap::new(), layer).is_err());
    }
}

struct FakeKeyring {
    calls: Cell<u64>,
    result: Result<Option<String>, CredentialError>,
}

impl KeyringBackend for FakeKeyring {
    fn get_password(
        &self,
        _service: &str,
        _account: &str,
    ) -> Result<Option<String>, CredentialError> {
        self.calls.set(self.calls.get() + 1);
        self.result.clone()
    }
}

#[test]
fn environment_wins_and_headless_never_queries_keyring() {
    let config = resolve_config(None, None, &BTreeMap::new(), ConfigLayer::default())
        .expect("default config");
    let marker = "sk-DO_NOT_PERSIST_THIS_SECRET";
    let environment = BTreeMap::from([("MINIMAX_API_KEY".to_owned(), marker.to_owned())]);
    let keyring = FakeKeyring {
        calls: Cell::new(0),
        result: Err(CredentialError::Denied),
    };
    let credential = CredentialResolver::new(&environment, Some(&keyring))
        .resolve(&config, CredentialMode::Interactive)
        .expect("environment credential");
    assert_eq!(credential.source(), CredentialSource::Environment);
    assert_eq!(keyring.calls.get(), 0);
    assert!(!format!("{credential:?}").contains(marker));

    let empty = BTreeMap::new();
    assert_eq!(
        CredentialResolver::new(&empty, Some(&keyring))
            .resolve(&config, CredentialMode::Headless)
            .expect_err("headless missing"),
        CredentialError::Missing
    );
    assert_eq!(keyring.calls.get(), 0);
}

#[test]
fn interactive_keyring_states_are_typed_and_secret_free() {
    let config = resolve_config(None, None, &BTreeMap::new(), ConfigLayer::default())
        .expect("default config");
    let environment = BTreeMap::new();
    for error in [
        CredentialError::Unavailable,
        CredentialError::Locked,
        CredentialError::Denied,
        CredentialError::Unknown,
    ] {
        let keyring = FakeKeyring {
            calls: Cell::new(0),
            result: Err(error),
        };
        let actual = CredentialResolver::new(&environment, Some(&keyring))
            .resolve(&config, CredentialMode::Interactive)
            .expect_err("typed keyring failure");
        assert_eq!(actual, error);
        assert_eq!(keyring.calls.get(), 1);
        assert!(!format!("{actual:?}:{actual}").contains("DO_NOT_PERSIST"));
    }
    let keyring = FakeKeyring {
        calls: Cell::new(0),
        result: Ok(Some("sk-DO_NOT_PERSIST_KEYRING".to_owned())),
    };
    let credential = CredentialResolver::new(&environment, Some(&keyring))
        .resolve(&config, CredentialMode::Interactive)
        .expect("keyring credential");
    assert_eq!(credential.source(), CredentialSource::OsKeyring);
    assert!(!format!("{credential:?}").contains("DO_NOT_PERSIST"));
    let public_json = serde_json::to_string(&config).expect("public config JSON");
    assert!(!public_json.contains("DO_NOT_PERSIST"));
}
