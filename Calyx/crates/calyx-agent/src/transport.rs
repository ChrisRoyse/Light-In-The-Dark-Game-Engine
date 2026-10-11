//! The fail-closed chat-completions transport.
//!
//! One call, one completion, or one structured error. Explicitly NOT present, by doctrine:
//!
//! * no fallback provider,
//! * no stub / canned / degraded local text,
//! * no silent retry (there is no retry at all — a failure is returned to the caller, who owns
//!   the decision to re-issue),
//! * no coercion of an unsupported model to a supported one.
//!
//! The provider's own error body is surfaced verbatim (redacted) on a non-2xx, because a
//! caller debugging a rejection needs the provider's reason, not ours.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{AgentError, Result, codes};
use crate::model::AgentModel;
use crate::secrets::AgentSecrets;
use crate::wire::{AgentCompletion, ChatCompletionPayload};

/// Upper bound on the response body we will read off the socket, in bytes.
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
/// How much of a provider error body to surface in an error message.
const MAX_SURFACED_ERROR_BYTES: usize = 2048;

/// Default sampling temperature: deterministic-leaning, as befits a grounded agent.
pub const DEFAULT_TEMPERATURE: f64 = 0.0;
/// Default output budget.
pub const DEFAULT_MAX_TOKENS: u32 = 1024;
/// Default wall-clock budget for the whole call.
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// A caller-owned chat-completions request.
///
/// `model` is a raw `String` on purpose: it is validated in [`AgentTransport::complete`] so that
/// a caller who assembles a model id dynamically (from a tier table, a config, an NPC record)
/// hits the SAME fail-closed validation as everyone else, rather than being able to bypass it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRequest {
    /// The system prompt: role, constraints, output contract.
    pub system_prompt: String,
    /// The user prompt: the actual task.
    pub user_prompt: String,
    /// The model id. Validated against [`AgentModel`] before the call.
    pub model: String,
    /// Output token budget. Must be > 0; not capped locally — the provider owns its own limit.
    pub max_tokens: u32,
    /// Sampling temperature, in `[0.0, 2.0]`.
    pub temperature: f64,
    /// Wall-clock budget for the entire call. Must be > 0.
    pub timeout_secs: u64,
    /// Ask the provider to constrain output to a JSON object.
    pub json_response: bool,
}

impl AgentRequest {
    /// Build a request against a validated model tier.
    pub fn new(
        model: AgentModel,
        system_prompt: impl Into<String>,
        user_prompt: impl Into<String>,
    ) -> Self {
        Self::with_model_id(model.as_str(), system_prompt, user_prompt)
    }

    /// Build a request against a raw model id (validated at call time, never coerced).
    pub fn with_model_id(
        model_id: impl Into<String>,
        system_prompt: impl Into<String>,
        user_prompt: impl Into<String>,
    ) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            user_prompt: user_prompt.into(),
            model: model_id.into(),
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: DEFAULT_TEMPERATURE,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            json_response: true,
        }
    }

    /// Override the output token budget.
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Override the sampling temperature.
    #[must_use]
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }

    /// Override the wall-clock budget.
    #[must_use]
    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Toggle JSON-object response formatting.
    #[must_use]
    pub fn with_json_response(mut self, json_response: bool) -> Self {
        self.json_response = json_response;
        self
    }

    /// Validate the caller's request and resolve the model. Runs before any socket is opened.
    pub fn validate(&self) -> Result<AgentModel> {
        let model = AgentModel::parse(&self.model)?;
        for (label, prompt) in [
            ("system_prompt", &self.system_prompt),
            ("user_prompt", &self.user_prompt),
        ] {
            if prompt.trim().is_empty() {
                return Err(AgentError::request(
                    codes::REQUEST_PROMPT_EMPTY,
                    format!("{label} was blank"),
                ));
            }
        }
        if self.max_tokens == 0 {
            return Err(AgentError::request(
                codes::REQUEST_MAX_TOKENS_INVALID,
                "max_tokens must be greater than zero",
            ));
        }
        if self.timeout_secs == 0 {
            return Err(AgentError::request(
                codes::REQUEST_TIMEOUT_INVALID,
                "timeout_secs must be greater than zero",
            ));
        }
        if !self.temperature.is_finite() || !(0.0..=2.0).contains(&self.temperature) {
            return Err(AgentError::request(
                codes::REQUEST_TEMPERATURE_INVALID,
                format!(
                    "temperature must be a finite value in [0.0, 2.0], got {}",
                    self.temperature
                ),
            ));
        }
        Ok(model)
    }

    fn body(&self) -> serde_json::Value {
        let mut body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": self.system_prompt},
                {"role": "user", "content": self.user_prompt}
            ],
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "stream": false
        });
        if self.json_response {
            body["response_format"] = json!({"type": "json_object"});
        }
        body
    }
}

/// The LLM call site. Holds validated credentials; issues validated requests.
#[derive(Debug)]
pub struct AgentTransport {
    secrets: AgentSecrets,
}

impl AgentTransport {
    /// Bind a transport to validated credentials.
    pub fn new(secrets: AgentSecrets) -> Self {
        Self { secrets }
    }

    /// The credentials this transport is bound to (for non-secret [`metadata`] evidence).
    ///
    /// [`metadata`]: AgentSecrets::metadata
    pub fn secrets(&self) -> &AgentSecrets {
        &self.secrets
    }

    /// Issue one chat completion. Returns a fully validated completion, or fails closed.
    ///
    /// This is [`complete_raw`] followed by the wire contract. The socket is opened in exactly one
    /// place; this method adds validation, never a second call path.
    ///
    /// [`complete_raw`]: AgentTransport::complete_raw
    pub fn complete(&self, request: &AgentRequest) -> Result<AgentCompletion> {
        let model = AgentModel::parse(&request.model)?;
        let raw = self.complete_raw(request)?;
        let payload: ChatCompletionPayload = serde_json::from_str(&raw.body).map_err(|err| {
            AgentError::transport(
                codes::TRANSPORT_RESPONSE_DECODE_FAILED,
                format!(
                    "decode chat-completion JSON from the provider: {err}; body: {}",
                    surfaced_body(raw.body.as_bytes())
                ),
            )
        })?;
        payload.into_completion(model.as_str())
    }

    /// Issue one chat completion and return the provider's response **before** the wire contract
    /// is applied.
    ///
    /// This exists so a caller can prove what the provider *actually sent* — including the bodies
    /// [`complete`] correctly refuses: truncated JSON (`finish_reason == "length"`), blank content,
    /// a substituted model. Without it, verifying the refusal path would require *fabricating* a
    /// truncated body, and a fabricated body proves nothing about the real provider.
    ///
    /// It is NOT a bypass. It performs the same request validation and the same non-2xx handling;
    /// it simply stops before interpreting the payload. Anything that consumes the result is
    /// responsible for its own validation — which, for the engine, is
    /// `litd_engine::agent_response_gate::admit_agent_response`.
    ///
    /// [`complete`]: AgentTransport::complete
    pub fn complete_raw(&self, request: &AgentRequest) -> Result<AgentRawResponse> {
        request.validate()?;
        let url = self.secrets.chat_completions_url();

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(request.timeout_secs)))
            // Read the body ourselves on a non-2xx so the PROVIDER's error reaches the caller.
            .http_status_as_error(false)
            .build()
            .into();

        let authorization = self.secrets.bearer_authorization();
        let mut response = agent
            .post(&url)
            .header("Authorization", authorization.as_str())
            .header("Content-Type", "application/json")
            .send_json(request.body())
            .map_err(|err| {
                AgentError::transport(
                    codes::TRANSPORT_HTTP_FAILED,
                    format!("POST {url} failed before a response was received: {err}"),
                )
            })?;

        let status = response.status().as_u16();
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_vec()
            .map_err(|err| {
                AgentError::transport(
                    codes::TRANSPORT_BODY_READ_FAILED,
                    format!("read response body from POST {url} (HTTP {status}): {err}"),
                )
            })?;

        if !(200..300).contains(&status) {
            return Err(AgentError::transport(
                codes::TRANSPORT_HTTP_STATUS,
                format!(
                    "POST {url} returned HTTP {status}; provider said: {}",
                    surfaced_body(&bytes)
                ),
            ));
        }

        let body = String::from_utf8(bytes).map_err(|err| {
            AgentError::transport(
                codes::TRANSPORT_BODY_READ_FAILED,
                format!("provider body from POST {url} was not valid UTF-8: {err}"),
            )
        })?;
        let sha256 = sha256_hex(body.as_bytes());
        Ok(AgentRawResponse {
            http_status: status,
            body,
            sha256,
        })
    }
}

/// The provider's response body as it arrived, before any interpretation.
///
/// Evidence, not a value. Its whole purpose is to let a verifier show the exact bytes a refusal
/// was made on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRawResponse {
    /// The HTTP status (always 2xx — a non-2xx is an error, never a raw response).
    pub http_status: u16,
    /// The response body, verbatim.
    pub body: String,
    /// SHA-256 of `body`. The forensic anchor.
    pub sha256: String,
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Render a provider body for inclusion in an error message: lossy UTF-8, bounded, one line.
fn surfaced_body(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "<empty body>".to_string();
    }
    let mut end = trimmed.len().min(MAX_SURFACED_ERROR_BYTES);
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    let clipped = &trimmed[..end];
    let flattened = clipped.replace(['\n', '\r'], " ");
    if end < trimmed.len() {
        format!("{flattened}...<truncated>")
    } else {
        flattened
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MODEL_FLASH, MODEL_PRO};

    fn request() -> AgentRequest {
        AgentRequest::new(AgentModel::Flash, "system", "user")
    }

    #[test]
    fn defaults_match_the_live_wire_contract() {
        let body = request().body();
        assert_eq!(body["model"], MODEL_FLASH);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["response_format"]["type"], "json_object");
        assert_eq!(body["stream"], false);
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(body["temperature"], DEFAULT_TEMPERATURE);
    }

    #[test]
    fn json_response_can_be_disabled_and_the_field_disappears() {
        let body = request().with_json_response(false).body();
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn validate_resolves_the_model_tier() {
        assert_eq!(request().validate().unwrap(), AgentModel::Flash);
        let pro = AgentRequest::with_model_id(MODEL_PRO, "s", "u");
        assert_eq!(pro.validate().unwrap(), AgentModel::Pro);
    }

    #[test]
    fn unsupported_model_is_rejected_not_coerced() {
        let err = AgentRequest::with_model_id("deepseek-reasoner", "s", "u")
            .validate()
            .unwrap_err();
        assert_eq!(err.code(), codes::MODEL_UNSUPPORTED);
    }

    #[test]
    fn blank_prompts_are_rejected() {
        for req in [
            AgentRequest::new(AgentModel::Flash, "  ", "user"),
            AgentRequest::new(AgentModel::Flash, "system", "\n\t"),
        ] {
            assert_eq!(
                req.validate().unwrap_err().code(),
                codes::REQUEST_PROMPT_EMPTY
            );
        }
    }

    #[test]
    fn zero_budgets_are_rejected() {
        assert_eq!(
            request().with_max_tokens(0).validate().unwrap_err().code(),
            codes::REQUEST_MAX_TOKENS_INVALID
        );
        assert_eq!(
            request()
                .with_timeout_secs(0)
                .validate()
                .unwrap_err()
                .code(),
            codes::REQUEST_TIMEOUT_INVALID
        );
    }

    #[test]
    fn out_of_range_temperature_is_rejected() {
        for bad in [-0.1, 2.1, f64::NAN, f64::INFINITY] {
            assert_eq!(
                request()
                    .with_temperature(bad)
                    .validate()
                    .unwrap_err()
                    .code(),
                codes::REQUEST_TEMPERATURE_INVALID
            );
        }
    }

    #[test]
    fn oversized_max_tokens_is_not_capped_locally() {
        // The PROVIDER owns its own token ceiling. Silently clamping here would turn a loud
        // provider rejection into a quietly different request. FSV edge case 3 depends on this.
        let req = request().with_max_tokens(u32::MAX);
        assert!(req.validate().is_ok());
        assert_eq!(req.body()["max_tokens"], u32::MAX);
    }

    #[test]
    fn surfaced_body_is_bounded_and_single_line() {
        assert_eq!(surfaced_body(b"  "), "<empty body>");
        assert_eq!(
            surfaced_body(b"{\"error\":\n \"bad\"}"),
            "{\"error\":  \"bad\"}"
        );
        let long = vec![b'x'; MAX_SURFACED_ERROR_BYTES + 64];
        let rendered = surfaced_body(&long);
        assert!(rendered.ends_with("...<truncated>"));
        assert!(rendered.len() < long.len() + 32);
    }
}
