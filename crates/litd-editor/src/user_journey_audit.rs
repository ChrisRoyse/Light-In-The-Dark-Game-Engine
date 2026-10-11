//! Ordered #905 user-journey audit state for exact-HWND Synapse walkthroughs.
//!
//! This module does not claim visual verification by itself. It records the
//! operator's manual observations and rejects structurally incomplete evidence
//! so a persisted report cannot silently skip a journey stage.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

pub const USER_JOURNEY_AUDIT_SCHEMA: &str = "litd.editor.user_journey_audit.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserJourneyStep {
    FirstLaunch,
    ChooseTemplate,
    MakeEdit,
    SeeLiveChange,
    CharacterAndStory,
    Playtest,
    Share,
}

impl UserJourneyStep {
    pub const ORDER: [Self; 7] = [
        Self::FirstLaunch,
        Self::ChooseTemplate,
        Self::MakeEdit,
        Self::SeeLiveChange,
        Self::CharacterAndStory,
        Self::Playtest,
        Self::Share,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::FirstLaunch => "First launch",
            Self::ChooseTemplate => "Choose a template",
            Self::MakeEdit => "Make one change",
            Self::SeeLiveChange => "See the change live",
            Self::CharacterAndStory => "Add character and story",
            Self::Playtest => "Playtest",
            Self::Share => "Share",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserJourneyEvidence {
    /// Stable artifact label or repo-relative evidence path, never an HWND-free claim.
    pub bitmap_artifact: String,
    pub bitmap_sha256: String,
    /// Physical source used for the separate runtime or persisted-state readback.
    pub state_source: String,
    pub state_readback: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserJourneyObservation {
    pub step: UserJourneyStep,
    pub exact_hwnd: u64,
    pub observed_unix_ms: u64,
    pub duration_ms: u64,
    pub surface_id: String,
    pub action: String,
    pub outcome: String,
    pub obvious: bool,
    pub fast: bool,
    pub rewarding: bool,
    pub dead_end: bool,
    pub evidence: UserJourneyEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserJourneyVerdict {
    InProgress,
    Failed,
    Passed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserJourneyAuditSummary {
    pub schema: String,
    pub session_id: String,
    pub exact_hwnd: u64,
    pub recorded_step_count: usize,
    pub required_step_count: usize,
    pub next_step: Option<UserJourneyStep>,
    pub elapsed_ms: u64,
    pub verdict: UserJourneyVerdict,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserJourneyAuditSession {
    pub schema: String,
    pub session_id: String,
    pub exact_hwnd: u64,
    pub started_unix_ms: u64,
    pub observations: Vec<UserJourneyObservation>,
}

impl UserJourneyAuditSession {
    pub fn start(
        session_id: impl Into<String>,
        exact_hwnd: u64,
        started_unix_ms: u64,
    ) -> Result<Self, EngineError> {
        let session_id = session_id.into();
        require_text("session_id", &session_id)?;
        if exact_hwnd == 0 {
            return Err(audit_error(
                "exact_hwnd",
                "walkthrough must bind a non-zero live window handle",
            ));
        }
        if started_unix_ms == 0 {
            return Err(audit_error(
                "started_unix_ms",
                "walkthrough start timestamp must be non-zero",
            ));
        }
        Ok(Self {
            schema: USER_JOURNEY_AUDIT_SCHEMA.to_string(),
            session_id,
            exact_hwnd,
            started_unix_ms,
            observations: Vec::with_capacity(UserJourneyStep::ORDER.len()),
        })
    }

    pub fn next_step(&self) -> Option<UserJourneyStep> {
        UserJourneyStep::ORDER.get(self.observations.len()).copied()
    }

    pub fn record(&mut self, observation: UserJourneyObservation) -> Result<(), EngineError> {
        self.validate()?;
        let Some(expected) = self.next_step() else {
            return Err(audit_error(
                "record",
                "walkthrough is already complete; no extra steps are accepted",
            ));
        };
        if observation.step != expected {
            return Err(audit_error(
                "step",
                format!(
                    "expected {} at index {}, got {}",
                    expected.label(),
                    self.observations.len(),
                    observation.step.label()
                ),
            ));
        }
        if observation.exact_hwnd != self.exact_hwnd {
            return Err(audit_error(
                "exact_hwnd",
                format!(
                    "step {} targeted HWND {}, session is bound to {}",
                    observation.step.label(),
                    observation.exact_hwnd,
                    self.exact_hwnd
                ),
            ));
        }
        let previous_time = self
            .observations
            .last()
            .map_or(self.started_unix_ms, |entry| entry.observed_unix_ms);
        if observation.observed_unix_ms < previous_time {
            return Err(audit_error(
                "observed_unix_ms",
                format!(
                    "step {} timestamp {} precedes previous journey timestamp {}",
                    observation.step.label(),
                    observation.observed_unix_ms,
                    previous_time
                ),
            ));
        }
        if observation.duration_ms == 0 {
            return Err(audit_error(
                "duration_ms",
                format!(
                    "step {} duration must be measured",
                    observation.step.label()
                ),
            ));
        }
        validate_observation(&observation)?;
        self.observations.push(observation);
        Ok(())
    }

    pub fn summary(&self, now_unix_ms: u64) -> Result<UserJourneyAuditSummary, EngineError> {
        self.validate()?;
        if now_unix_ms < self.started_unix_ms {
            return Err(audit_error(
                "now_unix_ms",
                "summary timestamp precedes walkthrough start",
            ));
        }
        let mut failures = Vec::new();
        for observation in &self.observations {
            let label = observation.step.label();
            if !observation.obvious {
                failures.push(format!("{label}: next action was not obvious"));
            }
            if !observation.fast {
                failures.push(format!("{label}: step was not fast"));
            }
            if !observation.rewarding {
                failures.push(format!("{label}: outcome was not rewarding"));
            }
            if observation.dead_end {
                failures.push(format!("{label}: dead end encountered"));
            }
        }
        let verdict = if !failures.is_empty() {
            UserJourneyVerdict::Failed
        } else if self.next_step().is_some() {
            UserJourneyVerdict::InProgress
        } else {
            UserJourneyVerdict::Passed
        };
        Ok(UserJourneyAuditSummary {
            schema: USER_JOURNEY_AUDIT_SCHEMA.to_string(),
            session_id: self.session_id.clone(),
            exact_hwnd: self.exact_hwnd,
            recorded_step_count: self.observations.len(),
            required_step_count: UserJourneyStep::ORDER.len(),
            next_step: self.next_step(),
            elapsed_ms: now_unix_ms - self.started_unix_ms,
            verdict,
            failures,
        })
    }

    /// Validate both newly-created and deserialized sessions before use.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema != USER_JOURNEY_AUDIT_SCHEMA {
            return Err(audit_error(
                "schema",
                format!("expected {USER_JOURNEY_AUDIT_SCHEMA}, got {}", self.schema),
            ));
        }
        require_text("session_id", &self.session_id)?;
        if self.exact_hwnd == 0 {
            return Err(audit_error(
                "exact_hwnd",
                "walkthrough must remain bound to a non-zero live window handle",
            ));
        }
        if self.started_unix_ms == 0 {
            return Err(audit_error(
                "started_unix_ms",
                "walkthrough start timestamp must be non-zero",
            ));
        }
        if self.observations.len() > UserJourneyStep::ORDER.len() {
            return Err(audit_error(
                "observations",
                format!(
                    "recorded {} steps but journey defines only {}",
                    self.observations.len(),
                    UserJourneyStep::ORDER.len()
                ),
            ));
        }

        let mut previous_time = self.started_unix_ms;
        for (index, observation) in self.observations.iter().enumerate() {
            let expected = UserJourneyStep::ORDER[index];
            if observation.step != expected {
                return Err(audit_error(
                    "observations.step",
                    format!(
                        "expected {} at index {index}, got {}",
                        expected.label(),
                        observation.step.label()
                    ),
                ));
            }
            if observation.exact_hwnd != self.exact_hwnd {
                return Err(audit_error(
                    "observations.exact_hwnd",
                    format!(
                        "step {} targeted HWND {}, session is bound to {}",
                        observation.step.label(),
                        observation.exact_hwnd,
                        self.exact_hwnd
                    ),
                ));
            }
            if observation.observed_unix_ms < previous_time {
                return Err(audit_error(
                    "observations.observed_unix_ms",
                    format!(
                        "step {} timestamp {} precedes previous journey timestamp {}",
                        observation.step.label(),
                        observation.observed_unix_ms,
                        previous_time
                    ),
                ));
            }
            if observation.duration_ms == 0 {
                return Err(audit_error(
                    "observations.duration_ms",
                    format!(
                        "step {} duration must be measured",
                        observation.step.label()
                    ),
                ));
            }
            validate_observation(observation)?;
            previous_time = observation.observed_unix_ms;
        }
        Ok(())
    }
}

fn validate_observation(observation: &UserJourneyObservation) -> Result<(), EngineError> {
    require_text("surface_id", &observation.surface_id)?;
    require_text("action", &observation.action)?;
    require_text("outcome", &observation.outcome)?;
    require_text(
        "evidence.bitmap_artifact",
        &observation.evidence.bitmap_artifact,
    )?;
    require_sha256(
        "evidence.bitmap_sha256",
        &observation.evidence.bitmap_sha256,
    )?;
    require_text("evidence.state_source", &observation.evidence.state_source)?;
    require_text(
        "evidence.state_readback",
        &observation.evidence.state_readback,
    )
}

fn require_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return Err(audit_error(field, "value must not be empty"));
    }
    Ok(())
}

fn require_sha256(field: &str, value: &str) -> Result<(), EngineError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(audit_error(
            field,
            "expected a 64-character SHA-256 hex digest",
        ));
    }
    Ok(())
}

fn audit_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.user_journey_audit.{field}"),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "user_journey_audit_tests.rs"]
mod tests;
