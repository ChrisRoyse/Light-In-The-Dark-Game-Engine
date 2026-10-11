//! Generic, Infisical-bound secret contract for LLM providers.
//!
//! The Infisical coordinates (project / environment / path / secret name) are **parameters**,
//! not constants: this atom is domain-neutral and any project can bind its own vault. A
//! [`AgentSecrets::gameeditor`] constructor binds the GameEditor coordinates.
//!
//! Secrets are only ever read from the **process environment**, which is expected to have been
//! populated by:
//!
//! ```text
//! infisical run --projectId=<id> --env=<env> --path=<path> -- <command>
//! ```
//!
//! The key is held in [`Zeroizing`] and is never returned, logged, serialized, or `Debug`-printed.
//! [`AgentSecrets::metadata`] exposes only non-secret evidence.

use std::env;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::{AgentError, Result, codes};
use crate::model::AgentModel;

/// The provider's OpenAI-compatible API origin. Not a secret; a constant.
pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";

/// GameEditor Infisical project id.
pub const GAMEEDITOR_PROJECT_ID: &str = "c2d7e44c-d7d1-4b27-aa23-2ed5a97fa0b5";
/// GameEditor Infisical environment slug.
pub const GAMEEDITOR_ENVIRONMENT: &str = "prod";
/// GameEditor Infisical secret path.
pub const GAMEEDITOR_SECRET_PATH: &str = "/gameeditor";
/// GameEditor Infisical secret name, injected as an environment variable of the same name.
pub const GAMEEDITOR_API_KEY_NAME: &str = "GAMEEDITOR_DEEPSEEK_API_KEY";

/// Non-secret Infisical coordinates identifying WHERE a key comes from.
///
/// Recorded verbatim in [`AgentSecretMetadata`] so FSV evidence can prove which vault path the
/// running process was actually bound to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfisicalSource {
    /// Infisical project id.
    pub project_id: String,
    /// Infisical environment slug (e.g. `prod`).
    pub environment: String,
    /// Infisical secret path (e.g. `/gameeditor`).
    pub secret_path: String,
    /// The secret's name, which is also the injected environment-variable name.
    pub api_key_name: String,
}

impl InfisicalSource {
    /// Bind an arbitrary project's coordinates.
    pub fn new(
        project_id: impl Into<String>,
        environment: impl Into<String>,
        secret_path: impl Into<String>,
        api_key_name: impl Into<String>,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            environment: environment.into(),
            secret_path: secret_path.into(),
            api_key_name: api_key_name.into(),
        }
    }

    /// The GameEditor coordinates.
    pub fn gameeditor() -> Self {
        Self::new(
            GAMEEDITOR_PROJECT_ID,
            GAMEEDITOR_ENVIRONMENT,
            GAMEEDITOR_SECRET_PATH,
            GAMEEDITOR_API_KEY_NAME,
        )
    }

    /// The injection command a human/agent must run to populate the environment.
    ///
    /// Non-secret: it names coordinates only. Emitted as remediation in FSV evidence.
    pub fn injection_command(&self) -> String {
        format!(
            "infisical run --projectId={} --env={} --path={} -- <command>",
            self.project_id, self.environment, self.secret_path
        )
    }

    fn validate(&self) -> Result<()> {
        let fields = [
            ("project_id", &self.project_id),
            ("environment", &self.environment),
            ("secret_path", &self.secret_path),
            ("api_key_name", &self.api_key_name),
        ];
        for (label, value) in fields {
            if value.trim().is_empty() || value.trim() != value.as_str() {
                return Err(AgentError::secret(
                    codes::SECRET_SOURCE_INVALID,
                    format!(
                        "Infisical source field {label} must be non-empty and free of \
                         leading/trailing whitespace"
                    ),
                ));
            }
        }
        if !self.secret_path.starts_with('/') {
            return Err(AgentError::secret(
                codes::SECRET_SOURCE_INVALID,
                format!(
                    "Infisical secret_path must be absolute (start with '/'), got {:?}",
                    self.secret_path
                ),
            ));
        }
        Ok(())
    }
}

/// Non-secret readback, safe for FSV artifacts, logs, and GitHub issue evidence.
///
/// Carries the key's *shape* (present / length / sha256 prefix) and never the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSecretMetadata {
    /// Infisical project id the key came from.
    pub project_id: String,
    /// Infisical environment slug.
    pub environment: String,
    /// Infisical secret path.
    pub secret_path: String,
    /// The injected environment-variable name.
    pub api_key_name: String,
    /// Always `true` when metadata exists — an [`AgentSecrets`] cannot be built without a key.
    pub key_present: bool,
    /// Byte length of the key.
    pub key_length: usize,
    /// Whether the key carries the `sk-` bearer prefix (always `true` post-validation).
    pub key_has_sk_prefix: bool,
    /// First 12 hex chars of SHA-256(key). Identifies the key without disclosing it.
    pub key_sha256_prefix: String,
    /// The provider origin.
    pub base_url: String,
    /// The model the caller selected for this call.
    pub model: String,
    /// The exact endpoint the transport will POST to.
    pub chat_completions_url: String,
}

/// Validated provider credentials. `Debug` intentionally omits the key.
pub struct AgentSecrets {
    source: InfisicalSource,
    api_key: Zeroizing<String>,
    base_url: String,
}

impl fmt::Debug for AgentSecrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentSecrets")
            .field("source", &self.source)
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl AgentSecrets {
    /// Load the GameEditor DeepSeek key from the Infisical-injected process environment.
    pub fn gameeditor() -> Result<Self> {
        Self::from_infisical_env(InfisicalSource::gameeditor())
    }

    /// Load a key named by arbitrary Infisical coordinates from the process environment.
    pub fn from_infisical_env(source: InfisicalSource) -> Result<Self> {
        source.validate()?;
        let api_key = read_required_env(&source)?;
        Self::from_values(source, api_key, DEEPSEEK_BASE_URL)
    }

    /// Validate explicit values. Used by env loading and by FSV edge cases.
    pub fn from_values(
        source: InfisicalSource,
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self> {
        source.validate()?;
        let api_key = api_key.into();
        let base_url = base_url.into();

        if api_key.is_empty() {
            return Err(AgentError::secret(
                codes::SECRET_EMPTY,
                format!("{} was empty", source.api_key_name),
            ));
        }
        if api_key.trim() != api_key.as_str() {
            return Err(AgentError::secret(
                codes::SECRET_HAS_WHITESPACE,
                format!("{} has leading or trailing whitespace", source.api_key_name),
            ));
        }
        if !api_key.starts_with("sk-") {
            return Err(AgentError::secret(
                codes::SECRET_INVALID_PREFIX,
                format!("{} must start with sk-", source.api_key_name),
            ));
        }
        validate_base_url(&base_url)?;

        Ok(Self {
            source,
            api_key: Zeroizing::new(api_key),
            base_url,
        })
    }

    /// The Infisical coordinates this key was bound to.
    pub fn source(&self) -> &InfisicalSource {
        &self.source
    }

    /// The provider origin.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The chat-completions endpoint.
    pub fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    /// Build the `Authorization` header value without leaving the key in ordinary drop memory.
    pub fn bearer_authorization(&self) -> Zeroizing<String> {
        Zeroizing::new(format!("Bearer {}", self.api_key.as_str()))
    }

    /// Non-secret evidence for FSV artifacts, for a given caller-selected model tier.
    pub fn metadata(&self, model: AgentModel) -> AgentSecretMetadata {
        AgentSecretMetadata {
            project_id: self.source.project_id.clone(),
            environment: self.source.environment.clone(),
            secret_path: self.source.secret_path.clone(),
            api_key_name: self.source.api_key_name.clone(),
            key_present: true,
            key_length: self.api_key.len(),
            key_has_sk_prefix: self.api_key.starts_with("sk-"),
            key_sha256_prefix: sha256_prefix(self.api_key.as_str(), 12),
            base_url: self.base_url.clone(),
            model: model.as_str().to_string(),
            chat_completions_url: self.chat_completions_url(),
        }
    }
}

fn read_required_env(source: &InfisicalSource) -> Result<String> {
    let name = source.api_key_name.as_str();
    match env::var(name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Err(AgentError::secret(
            codes::SECRET_ENV_MISSING,
            format!(
                "{name} was not injected into the process environment; run: {}",
                source.injection_command()
            ),
        )),
        Err(env::VarError::NotUnicode(_)) => Err(AgentError::secret(
            codes::SECRET_ENV_NON_UNICODE,
            format!("{name} was not valid UTF-8"),
        )),
    }
}

fn validate_base_url(base_url: &str) -> Result<()> {
    let invalid = base_url.is_empty()
        || base_url.trim() != base_url
        || !base_url.starts_with("https://")
        || base_url.ends_with('/')
        || base_url.len() <= "https://".len();
    if invalid {
        return Err(AgentError::secret(
            codes::SECRET_BASE_URL_INVALID,
            format!(
                "provider base URL must be an absolute https origin with no trailing slash, \
                 got {base_url:?}"
            ),
        ));
    }
    Ok(())
}

fn sha256_prefix(value: &str, len: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    hex[..len].to_string()
}

#[cfg(test)]
impl AgentSecrets {
    /// Test-only: bind a synthetic bearer key to a loopback listener origin, bypassing the
    /// production https-origin requirement.
    ///
    /// This exists solely so the REAL transport HTTP path can be exercised end-to-end against a
    /// local `TcpListener` serving captured provider JSON (see `transport_http_tests`). It is
    /// `#[cfg(test)]`, so it is NEVER compiled into a release build and cannot become a runtime
    /// escape hatch. It constructs the struct directly rather than relaxing
    /// [`validate_base_url`], so the production origin contract is untouched.
    pub(crate) fn for_local_listener(base_url: impl Into<String>) -> Self {
        Self {
            source: InfisicalSource::gameeditor(),
            api_key: Zeroizing::new("sk-localtestkey00000000000000000000".to_string()),
            base_url: base_url.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_secrets() -> AgentSecrets {
        AgentSecrets::from_values(
            InfisicalSource::gameeditor(),
            "sk-0123456789abcdef",
            DEEPSEEK_BASE_URL,
        )
        .expect("valid synthetic secrets")
    }

    #[test]
    fn gameeditor_source_carries_the_real_coordinates() {
        let source = InfisicalSource::gameeditor();
        assert_eq!(source.project_id, GAMEEDITOR_PROJECT_ID);
        assert_eq!(source.environment, GAMEEDITOR_ENVIRONMENT);
        assert_eq!(source.secret_path, GAMEEDITOR_SECRET_PATH);
        assert_eq!(source.api_key_name, GAMEEDITOR_API_KEY_NAME);
        assert!(source.injection_command().contains("--path=/gameeditor"));
    }

    #[test]
    fn debug_never_leaks_the_key() {
        let debug = format!("{:?}", ok_secrets());
        assert!(!debug.contains("sk-"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn metadata_is_non_secret_and_identifies_the_key() {
        let metadata = ok_secrets().metadata(AgentModel::Flash);
        assert!(metadata.key_present);
        assert!(metadata.key_has_sk_prefix);
        assert_eq!(metadata.key_length, 19);
        assert_eq!(metadata.key_sha256_prefix.len(), 12);
        assert_eq!(
            metadata.chat_completions_url,
            "https://api.deepseek.com/chat/completions"
        );
        let json = serde_json::to_string(&metadata).expect("metadata serializes");
        assert!(!json.contains("sk-0123456789abcdef"));
    }

    #[test]
    fn empty_key_fails_closed() {
        let err = AgentSecrets::from_values(InfisicalSource::gameeditor(), "", DEEPSEEK_BASE_URL)
            .unwrap_err();
        assert_eq!(err.code(), codes::SECRET_EMPTY);
    }

    #[test]
    fn whitespace_padded_key_fails_closed() {
        let err = AgentSecrets::from_values(
            InfisicalSource::gameeditor(),
            " sk-abc\n",
            DEEPSEEK_BASE_URL,
        )
        .unwrap_err();
        assert_eq!(err.code(), codes::SECRET_HAS_WHITESPACE);
    }

    #[test]
    fn key_without_sk_prefix_fails_closed() {
        let err = AgentSecrets::from_values(
            InfisicalSource::gameeditor(),
            "not-a-bearer-key",
            DEEPSEEK_BASE_URL,
        )
        .unwrap_err();
        assert_eq!(err.code(), codes::SECRET_INVALID_PREFIX);
    }

    #[test]
    fn non_https_or_trailing_slash_base_url_fails_closed() {
        for bad in [
            "http://api.deepseek.com",
            "https://api.deepseek.com/",
            "",
            "api.deepseek.com",
        ] {
            let err = AgentSecrets::from_values(InfisicalSource::gameeditor(), "sk-abc", bad)
                .unwrap_err();
            assert_eq!(err.code(), codes::SECRET_BASE_URL_INVALID, "base_url={bad}");
        }
    }

    #[test]
    fn blank_source_coordinates_fail_closed() {
        let err = AgentSecrets::from_values(
            InfisicalSource::new("", "prod", "/gameeditor", "K"),
            "sk-abc",
            DEEPSEEK_BASE_URL,
        )
        .unwrap_err();
        assert_eq!(err.code(), codes::SECRET_SOURCE_INVALID);
    }

    #[test]
    fn missing_env_var_fails_closed_with_remediation() {
        let source = InfisicalSource::new(
            GAMEEDITOR_PROJECT_ID,
            GAMEEDITOR_ENVIRONMENT,
            GAMEEDITOR_SECRET_PATH,
            "CALYX_AGENT_DEFINITELY_ABSENT_ENV_VAR_FOR_TESTS",
        );
        let err = AgentSecrets::from_infisical_env(source).unwrap_err();
        assert_eq!(err.code(), codes::SECRET_ENV_MISSING);
        assert!(err.message().contains("infisical run"));
    }
}
