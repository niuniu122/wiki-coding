use std::collections::BTreeMap;
use std::fmt;

use secrecy::SecretString;
use serde::Serialize;

use crate::ResolvedConfig;

const KEYRING_SERVICE: &str = "minimax-codex-rust";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialMode {
    Headless,
    Interactive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    Environment,
    OsKeyring,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialError {
    Missing,
    Unavailable,
    Locked,
    Denied,
    Unknown,
}

impl fmt::Display for CredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Missing => "a Provider credential is required",
            Self::Unavailable => "the OS credential store is unavailable",
            Self::Locked => "the OS credential store is locked",
            Self::Denied => "OS credential access was denied",
            Self::Unknown => "OS credential access failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CredentialError {}

pub struct ResolvedCredential {
    secret: SecretString,
    source: CredentialSource,
}

impl ResolvedCredential {
    #[must_use]
    pub const fn source(&self) -> CredentialSource {
        self.source
    }

    #[must_use]
    pub const fn secret(&self) -> &SecretString {
        &self.secret
    }
}

impl fmt::Debug for ResolvedCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedCredential")
            .field("source", &self.source)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

pub trait KeyringBackend {
    fn get_password(&self, service: &str, account: &str)
    -> Result<Option<String>, CredentialError>;
}

pub struct OsKeyringBackend;

impl KeyringBackend for OsKeyringBackend {
    fn get_password(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<String>, CredentialError> {
        let entry = keyring::Entry::new(service, account).map_err(map_keyring_error)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

pub struct CredentialResolver<'a> {
    environment: &'a BTreeMap<String, String>,
    keyring: Option<&'a dyn KeyringBackend>,
}

impl<'a> CredentialResolver<'a> {
    #[must_use]
    pub const fn new(
        environment: &'a BTreeMap<String, String>,
        keyring: Option<&'a dyn KeyringBackend>,
    ) -> Self {
        Self {
            environment,
            keyring,
        }
    }

    pub fn resolve(
        &self,
        config: &ResolvedConfig,
        mode: CredentialMode,
    ) -> Result<ResolvedCredential, CredentialError> {
        if let Some(value) = self
            .environment
            .get(&config.environment_key)
            .and_then(|value| normalize_secret(value))
        {
            return Ok(ResolvedCredential {
                secret: SecretString::from(value),
                source: CredentialSource::Environment,
            });
        }
        if mode == CredentialMode::Headless {
            return Err(CredentialError::Missing);
        }
        let keyring = self.keyring.ok_or(CredentialError::Unavailable)?;
        let account = credential_account(config);
        let value = keyring
            .get_password(KEYRING_SERVICE, &account)?
            .and_then(|value| normalize_secret(&value))
            .ok_or(CredentialError::Missing)?;
        Ok(ResolvedCredential {
            secret: SecretString::from(value),
            source: CredentialSource::OsKeyring,
        })
    }
}

fn normalize_secret(value: &str) -> Option<String> {
    let mut normalized = value.trim();
    if normalized.len() >= 2
        && ((normalized.starts_with('"') && normalized.ends_with('"'))
            || (normalized.starts_with('\'') && normalized.ends_with('\'')))
    {
        normalized = &normalized[1..normalized.len() - 1];
    }
    if let (Some(prefix), Some(rest)) = (normalized.get(..6), normalized.get(6..))
        && prefix.eq_ignore_ascii_case("bearer")
        && (rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace))
    {
        normalized = rest.trim();
    }
    (!normalized.is_empty()).then(|| normalized.to_owned())
}

fn credential_account(config: &ResolvedConfig) -> String {
    let identity = format!("{}:{}", config.provider_id.as_str(), config.endpoint);
    format!("v1-{:016x}", stable_hash(identity.as_bytes()))
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn map_keyring_error(error: keyring::Error) -> CredentialError {
    match error {
        keyring::Error::NoEntry => CredentialError::Missing,
        keyring::Error::NoStorageAccess(_) => CredentialError::Locked,
        keyring::Error::PlatformFailure(_) => CredentialError::Unavailable,
        keyring::Error::Invalid(_, _) | keyring::Error::TooLong(_, _) => CredentialError::Denied,
        _ => CredentialError::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_secret;
    use secrecy::ExposeSecret as _;

    #[test]
    fn normalization_is_bounded_to_wrapper_syntax() {
        assert_eq!(
            normalize_secret(" Bearer secret ").as_deref(),
            Some("secret")
        );
        assert_eq!(normalize_secret("'secret'").as_deref(), Some("secret"));
        assert_eq!(
            normalize_secret("BearerSecret").as_deref(),
            Some("BearerSecret")
        );
        assert_eq!(
            normalize_secret("🔐🔐secret").as_deref(),
            Some("🔐🔐secret")
        );
        assert_eq!(normalize_secret(" "), None);
        let secret = secrecy::SecretString::from("secret".to_owned());
        assert_eq!(secret.expose_secret(), "secret");
    }
}
