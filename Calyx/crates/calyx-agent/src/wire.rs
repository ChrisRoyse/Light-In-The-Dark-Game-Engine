//! The provider's chat-completions wire contract, and its strict validation.
//!
//! Every field the provider promised is checked. A response that deviates in ANY way —
//! wrong `object`, a substituted model, zero or many choices, a non-`stop` finish reason,
//! missing or blank content — is a structured error, never a best-effort value. This is the
//! fail-closed rule: a plausible-looking wrong answer is worse than a loud failure.

use serde::{Deserialize, Serialize};

use crate::error::{AgentError, Result, codes};

/// Token accounting reported by the provider. Real evidence for FSV and for cost telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentUsage {
    /// Tokens consumed by the prompt.
    pub prompt_tokens: Option<u64>,
    /// Tokens produced in the completion.
    pub completion_tokens: Option<u64>,
    /// Provider-reported total.
    pub total_tokens: Option<u64>,
}

/// A fully validated completion. Its existence is proof the wire contract held.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCompletion {
    /// The provider's request id. Trace this in sanitized logs and issue evidence.
    pub id: String,
    /// The model the provider actually served (checked to equal the model requested).
    pub model: String,
    /// Always `stop` — any other value was rejected.
    pub finish_reason: String,
    /// The assistant message content, guaranteed non-blank.
    pub content: String,
    /// Provider token accounting.
    pub usage: AgentUsage,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatCompletionPayload {
    id: String,
    object: String,
    model: String,
    choices: Vec<ChoicePayload>,
    usage: Option<UsagePayload>,
}

#[derive(Debug, Deserialize)]
struct ChoicePayload {
    finish_reason: String,
    message: MessagePayload,
}

#[derive(Debug, Deserialize)]
struct MessagePayload {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsagePayload {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

impl ChatCompletionPayload {
    /// Enforce every clause of the wire contract against the model that was requested.
    pub(crate) fn into_completion(self, expected_model: &str) -> Result<AgentCompletion> {
        if self.object != "chat.completion" {
            return Err(AgentError::transport(
                codes::TRANSPORT_OBJECT_INVALID,
                format!(
                    "provider object was {:?}, expected \"chat.completion\"",
                    self.object
                ),
            ));
        }
        if self.model != expected_model {
            return Err(AgentError::transport(
                codes::TRANSPORT_MODEL_MISMATCH,
                format!(
                    "provider served model {:?}, expected {expected_model:?}",
                    self.model
                ),
            ));
        }
        if self.choices.len() != 1 {
            return Err(AgentError::transport(
                codes::TRANSPORT_CHOICE_COUNT_INVALID,
                format!(
                    "expected exactly one choice, provider returned {}",
                    self.choices.len()
                ),
            ));
        }
        let choice = self
            .choices
            .into_iter()
            .next()
            .expect("choice count checked above");
        if choice.finish_reason != "stop" {
            return Err(AgentError::transport(
                codes::TRANSPORT_FINISH_REASON_INVALID,
                format!(
                    "provider finish_reason was {:?}, expected \"stop\" (request id {})",
                    choice.finish_reason, self.id
                ),
            ));
        }
        let content = choice.message.content.ok_or_else(|| {
            AgentError::transport(
                codes::TRANSPORT_CONTENT_MISSING,
                format!(
                    "provider message content was absent (request id {})",
                    self.id
                ),
            )
        })?;
        if content.trim().is_empty() {
            return Err(AgentError::transport(
                codes::TRANSPORT_CONTENT_EMPTY,
                format!(
                    "provider message content was blank (request id {})",
                    self.id
                ),
            ));
        }
        let usage = self
            .usage
            .map_or_else(AgentUsage::default, |usage| AgentUsage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            });
        Ok(AgentCompletion {
            id: self.id,
            model: self.model,
            finish_reason: choice.finish_reason,
            content,
            usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MODEL_FLASH;

    fn payload(json: serde_json::Value) -> ChatCompletionPayload {
        serde_json::from_value(json).expect("fixture decodes")
    }

    fn valid_json() -> serde_json::Value {
        serde_json::json!({
            "id": "2c15207d-09a0-46a1-b59c-ef42bc517ba1",
            "object": "chat.completion",
            "model": MODEL_FLASH,
            "choices": [{
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": "{\"sum\": 4}"}
            }],
            "usage": {"prompt_tokens": 56, "completion_tokens": 35, "total_tokens": 91}
        })
    }

    #[test]
    fn valid_response_yields_a_completion() {
        let completion = payload(valid_json())
            .into_completion(MODEL_FLASH)
            .expect("contract holds");
        assert_eq!(completion.id, "2c15207d-09a0-46a1-b59c-ef42bc517ba1");
        assert_eq!(completion.finish_reason, "stop");
        assert_eq!(completion.usage.total_tokens, Some(91));
    }

    #[test]
    fn wrong_object_fails_closed() {
        let mut json = valid_json();
        json["object"] = serde_json::json!("text_completion");
        let err = payload(json).into_completion(MODEL_FLASH).unwrap_err();
        assert_eq!(err.code(), codes::TRANSPORT_OBJECT_INVALID);
    }

    #[test]
    fn substituted_model_fails_closed() {
        let err = payload(valid_json())
            .into_completion("deepseek-v4-pro")
            .unwrap_err();
        assert_eq!(err.code(), codes::TRANSPORT_MODEL_MISMATCH);
    }

    #[test]
    fn choice_count_other_than_one_fails_closed() {
        for choices in [
            serde_json::json!([]),
            serde_json::json!([
                {"finish_reason": "stop", "message": {"content": "a"}},
                {"finish_reason": "stop", "message": {"content": "b"}}
            ]),
        ] {
            let mut json = valid_json();
            json["choices"] = choices;
            let err = payload(json).into_completion(MODEL_FLASH).unwrap_err();
            assert_eq!(err.code(), codes::TRANSPORT_CHOICE_COUNT_INVALID);
        }
    }

    #[test]
    fn truncated_completion_fails_closed() {
        let mut json = valid_json();
        json["choices"][0]["finish_reason"] = serde_json::json!("length");
        let err = payload(json).into_completion(MODEL_FLASH).unwrap_err();
        assert_eq!(err.code(), codes::TRANSPORT_FINISH_REASON_INVALID);
    }

    #[test]
    fn missing_and_blank_content_fail_closed() {
        let mut missing = valid_json();
        missing["choices"][0]["message"] = serde_json::json!({"role": "assistant"});
        assert_eq!(
            payload(missing)
                .into_completion(MODEL_FLASH)
                .unwrap_err()
                .code(),
            codes::TRANSPORT_CONTENT_MISSING
        );

        let mut blank = valid_json();
        blank["choices"][0]["message"]["content"] = serde_json::json!("   \n");
        assert_eq!(
            payload(blank)
                .into_completion(MODEL_FLASH)
                .unwrap_err()
                .code(),
            codes::TRANSPORT_CONTENT_EMPTY
        );
    }

    #[test]
    fn absent_usage_block_is_not_invented() {
        let mut json = valid_json();
        json.as_object_mut().expect("object").remove("usage");
        let completion = payload(json).into_completion(MODEL_FLASH).unwrap();
        assert_eq!(completion.usage, AgentUsage::default());
        assert!(completion.usage.total_tokens.is_none());
    }
}
