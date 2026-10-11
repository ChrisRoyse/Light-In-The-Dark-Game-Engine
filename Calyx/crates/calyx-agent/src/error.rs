//! Structured, fail-closed error type for the Calyx agent transport atom.
//!
//! Every failure carries a stable machine-readable `code` plus a human `message`.
//! There is no `Ok`-with-degraded-output path anywhere in this crate: an agent call either
//! returns a fully validated completion or an [`AgentError`].
//!
//! Secret discipline: every message is passed through [`redact_secret`] at construction time,
//! so no `sk-` bearer token can ever reach a log, a panic message, or an FSV artifact — even
//! if a provider or a transport library echoes the Authorization header back at us.

use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable machine-readable error codes emitted by this crate.
///
/// These are part of the crate's public contract: callers may branch on them, and FSV
/// evidence asserts against them. Never renumber or reuse one.
pub mod codes {
    // ── Secret contract (Infisical-injected process environment) ────────────────
    /// The Infisical source coordinates were blank or malformed.
    pub const SECRET_SOURCE_INVALID: &str = "CALYX_AGENT_SECRET_SOURCE_INVALID";
    /// The named secret was not injected into the process environment at all.
    pub const SECRET_ENV_MISSING: &str = "CALYX_AGENT_SECRET_ENV_MISSING";
    /// The named environment variable was not valid UTF-8.
    pub const SECRET_ENV_NON_UNICODE: &str = "CALYX_AGENT_SECRET_ENV_NON_UNICODE";
    /// The secret was present but empty.
    pub const SECRET_EMPTY: &str = "CALYX_AGENT_SECRET_EMPTY";
    /// The secret had leading or trailing whitespace (a copy/paste corruption).
    pub const SECRET_HAS_WHITESPACE: &str = "CALYX_AGENT_SECRET_HAS_WHITESPACE";
    /// The secret did not carry the `sk-` bearer-key prefix.
    pub const SECRET_INVALID_PREFIX: &str = "CALYX_AGENT_SECRET_INVALID_PREFIX";
    /// The provider base URL was not an absolute, https, trailing-slash-free origin.
    pub const SECRET_BASE_URL_INVALID: &str = "CALYX_AGENT_SECRET_BASE_URL_INVALID";

    // ── Request contract (validated before a single byte leaves the process) ────
    /// The model id is not one of the provider's real, currently-served model ids.
    pub const MODEL_UNSUPPORTED: &str = "CALYX_AGENT_MODEL_UNSUPPORTED";
    /// A prompt field was blank.
    pub const REQUEST_PROMPT_EMPTY: &str = "CALYX_AGENT_REQUEST_PROMPT_EMPTY";
    /// `max_tokens` was zero.
    pub const REQUEST_MAX_TOKENS_INVALID: &str = "CALYX_AGENT_REQUEST_MAX_TOKENS_INVALID";
    /// `timeout_secs` was zero.
    pub const REQUEST_TIMEOUT_INVALID: &str = "CALYX_AGENT_REQUEST_TIMEOUT_INVALID";
    /// `temperature` was non-finite or outside the provider's accepted range.
    pub const REQUEST_TEMPERATURE_INVALID: &str = "CALYX_AGENT_REQUEST_TEMPERATURE_INVALID";

    // ── Transport contract (the call, and the shape of what came back) ──────────
    /// The HTTP call could not be completed (DNS, TLS, connect, timeout, ...).
    pub const TRANSPORT_HTTP_FAILED: &str = "CALYX_AGENT_TRANSPORT_HTTP_FAILED";
    /// The provider answered with a non-2xx status. The provider's own error body is surfaced.
    pub const TRANSPORT_HTTP_STATUS: &str = "CALYX_AGENT_TRANSPORT_HTTP_STATUS";
    /// The response body could not be read off the socket.
    pub const TRANSPORT_BODY_READ_FAILED: &str = "CALYX_AGENT_TRANSPORT_BODY_READ_FAILED";
    /// The response body was not the expected chat-completion JSON.
    pub const TRANSPORT_RESPONSE_DECODE_FAILED: &str =
        "CALYX_AGENT_TRANSPORT_RESPONSE_DECODE_FAILED";
    /// `object` was not `chat.completion`.
    pub const TRANSPORT_OBJECT_INVALID: &str = "CALYX_AGENT_TRANSPORT_OBJECT_INVALID";
    /// The provider served a different model than the one requested.
    pub const TRANSPORT_MODEL_MISMATCH: &str = "CALYX_AGENT_TRANSPORT_MODEL_MISMATCH";
    /// The provider returned other than exactly one choice.
    pub const TRANSPORT_CHOICE_COUNT_INVALID: &str = "CALYX_AGENT_TRANSPORT_CHOICE_COUNT_INVALID";
    /// `finish_reason` was not `stop` (truncated, filtered, tool-call, ...).
    pub const TRANSPORT_FINISH_REASON_INVALID: &str = "CALYX_AGENT_TRANSPORT_FINISH_REASON_INVALID";
    /// The single choice carried no message content.
    pub const TRANSPORT_CONTENT_MISSING: &str = "CALYX_AGENT_TRANSPORT_CONTENT_MISSING";
    /// The single choice carried whitespace-only message content.
    pub const TRANSPORT_CONTENT_EMPTY: &str = "CALYX_AGENT_TRANSPORT_CONTENT_EMPTY";
}

/// Which contract was violated. Coarse classification on top of the exact [`codes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorKind {
    /// The Infisical-injected secret contract was violated. No request was built.
    Secret,
    /// The caller-supplied request was invalid. No byte left the process.
    Request,
    /// The provider call failed, or its response violated the wire contract.
    Transport,
}

impl AgentErrorKind {
    /// Stable snake_case discriminant, for logs and JSON evidence.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Secret => "secret",
            Self::Request => "request",
            Self::Transport => "transport",
        }
    }
}

impl fmt::Display for AgentErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A fail-closed agent error: a stable code, a redacted message, and a coarse kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentError {
    kind: AgentErrorKind,
    code: String,
    message: String,
}

impl AgentError {
    /// Build an error, redacting any bearer-key material from the message.
    pub fn new(kind: AgentErrorKind, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: code.into(),
            message: redact_secret(&message.into()),
        }
    }

    /// A violation of the Infisical secret contract.
    pub fn secret(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::Secret, code, message)
    }

    /// A violation of the caller-supplied request contract.
    pub fn request(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::Request, code, message)
    }

    /// A violation of the provider transport / wire contract.
    pub fn transport(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AgentErrorKind::Transport, code, message)
    }

    /// The coarse contract that was violated.
    pub const fn kind(&self) -> AgentErrorKind {
        self.kind
    }

    /// The stable machine-readable code. Branch on this, never on the message.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The human-readable, secret-redacted explanation.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}/{}] {}", self.kind, self.code, self.message)
    }
}

impl Error for AgentError {}

/// Replace any whitespace-delimited token that looks like a bearer key with a placeholder.
///
/// Applied to every [`AgentError`] message at construction. Also handles keys embedded in
/// JSON/quoted contexts by scanning for the `sk-` marker inside each token.
pub fn redact_secret(message: &str) -> String {
    message
        .split_whitespace()
        .map(|token| {
            if token.contains("sk-") {
                "<redacted-secret>".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The crate-wide fail-closed result alias.
pub type Result<T> = std::result::Result<T, AgentError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_message_redacts_bearer_key() {
        let err = AgentError::transport(
            codes::TRANSPORT_HTTP_FAILED,
            "auth failed for Bearer sk-abcdef0123456789 at api.deepseek.com",
        );
        assert!(!err.message().contains("sk-"));
        assert!(err.message().contains("<redacted-secret>"));
        assert!(err.message().contains("api.deepseek.com"));
    }

    #[test]
    fn error_message_redacts_quoted_and_json_embedded_key() {
        let err = AgentError::secret(codes::SECRET_INVALID_PREFIX, "{\"key\":\"sk-deadbeef\"}");
        assert!(!err.message().contains("sk-"));
    }

    #[test]
    fn display_carries_kind_and_code() {
        let err = AgentError::request(codes::REQUEST_PROMPT_EMPTY, "user prompt was blank");
        assert_eq!(
            err.to_string(),
            "[request/CALYX_AGENT_REQUEST_PROMPT_EMPTY] user prompt was blank"
        );
        assert_eq!(err.kind(), AgentErrorKind::Request);
    }
}
