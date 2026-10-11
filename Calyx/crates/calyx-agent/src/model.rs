//! The provider's real, currently-served model ids — and nothing else.
//!
//! Model selection is the CALLER's decision (a tier choice), not a secret and not a config
//! file. The only thing this module enforces is that the id the caller picked is one the
//! provider actually serves. An unknown id fails closed with
//! [`codes::MODEL_UNSUPPORTED`][crate::error::codes::MODEL_UNSUPPORTED]; it is NEVER silently
//! coerced to a "close enough" model, because a silently-substituted model is a silently-wrong
//! answer.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{AgentError, Result, codes};

/// The fast/cheap tier.
pub const MODEL_FLASH: &str = "deepseek-v4-flash";
/// The high-capability tier.
pub const MODEL_PRO: &str = "deepseek-v4-pro";

/// A validated provider model id.
///
/// Constructing one of these is the only way to name a model in a typed request, so an
/// unsupported id cannot reach the wire by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentModel {
    /// `deepseek-v4-flash`
    Flash,
    /// `deepseek-v4-pro`
    Pro,
}

impl AgentModel {
    /// Every model the provider serves. Live-verified against `GET /models`.
    pub const ALL: [AgentModel; 2] = [AgentModel::Flash, AgentModel::Pro];

    /// The exact wire id sent to the provider.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flash => MODEL_FLASH,
            Self::Pro => MODEL_PRO,
        }
    }

    /// Validate a raw model id string against the real served ids.
    ///
    /// Fails closed on anything else — including legacy aliases such as `deepseek-chat`,
    /// which the provider deprecates and which must never be silently accepted.
    pub fn parse(id: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|model| model.as_str() == id)
            .ok_or_else(|| {
                AgentError::request(
                    codes::MODEL_UNSUPPORTED,
                    format!(
                        "model id {id:?} is not served by the provider; the only valid ids are {MODEL_FLASH:?} and {MODEL_PRO:?}"
                    ),
                )
            })
    }
}

impl fmt::Display for AgentModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_two_real_model_ids() {
        assert_eq!(AgentModel::parse(MODEL_FLASH).unwrap(), AgentModel::Flash);
        assert_eq!(AgentModel::parse(MODEL_PRO).unwrap(), AgentModel::Pro);
    }

    #[test]
    fn rejects_legacy_alias_without_coercion() {
        let err = AgentModel::parse("deepseek-chat").unwrap_err();
        assert_eq!(err.code(), codes::MODEL_UNSUPPORTED);
        assert!(err.message().contains("deepseek-chat"));
    }

    #[test]
    fn rejects_empty_and_near_miss_ids() {
        for bad in ["", "deepseek-v4", "DEEPSEEK-V4-PRO", " deepseek-v4-pro"] {
            let err = AgentModel::parse(bad).unwrap_err();
            assert_eq!(err.code(), codes::MODEL_UNSUPPORTED);
        }
    }
}
