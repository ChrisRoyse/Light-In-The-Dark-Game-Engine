//! #899 first-launch coachmarks: progressive tips + 5-minute budget.
//!
//! Source of truth is the session struct (and optional JSON persistence). No mock
//! tips: callers must supply real gallery/starter tips or use the shipped melee
//! first-launch set. Fail loud on empty tips, invalid advance, or expired budget
//! when claiming "under 5 minutes".

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::starter_templates::GuidedFirstEditTooltip;

/// Five-minute first-launch budget (WC3-style on-ramp claim).
pub const FIRST_LAUNCH_BUDGET_MS: u64 = 5 * 60 * 1000;

pub const FIRST_LAUNCH_COACH_SCHEMA: &str = "litd.editor.first_launch_coach.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstLaunchCoachSession {
    pub schema: String,
    pub template_id: String,
    pub tips: Vec<GuidedFirstEditTooltip>,
    /// Index of the active tip; equals `tips.len()` when completed.
    pub current_index: usize,
    pub started_unix_ms: u64,
    pub budget_ms: u64,
    pub dismissed: bool,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstLaunchCoachSnapshot {
    pub schema: String,
    pub template_id: String,
    pub tip_count: usize,
    pub current_index: usize,
    pub current_surface: Option<String>,
    pub current_target_id: Option<String>,
    pub current_prompt: Option<String>,
    pub remaining_budget_ms: u64,
    pub within_budget: bool,
    pub dismissed: bool,
    pub completed: bool,
    pub status_line: String,
}

fn coach_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.first_launch_coach.{field}"),
        reason: reason.into(),
    }
}

/// Shipped first-launch tips for the gallery melee starter (#899 / #901).
pub fn melee_skirmish_first_launch_tips() -> Vec<GuidedFirstEditTooltip> {
    vec![
        GuidedFirstEditTooltip {
            surface: "world_view".to_string(),
            // Real region id from template_gallery::build_melee_skirmish.
            target_id: "region.player_one_base".to_string(),
            prompt: "Select Player One Base and nudge its size — one change, see it immediately."
                .to_string(),
            suggested_edit: "scale_region_extents".to_string(),
        },
        GuidedFirstEditTooltip {
            surface: "object_inspector".to_string(),
            // Real start location id from template_gallery::build_melee_skirmish.
            target_id: "start.player_1".to_string(),
            prompt: "Move Player 1 start location so the spawn feels right.".to_string(),
            suggested_edit: "translate_start_location".to_string(),
        },
        GuidedFirstEditTooltip {
            surface: "play_mode".to_string(),
            target_id: "play.instant".to_string(),
            prompt: "Hit Play to see your change live in under five minutes.".to_string(),
            suggested_edit: "enter_play_mode".to_string(),
        },
    ]
}

/// Tips for the story/cinematic gallery template.
pub fn story_cinematic_first_launch_tips() -> Vec<GuidedFirstEditTooltip> {
    vec![
        GuidedFirstEditTooltip {
            surface: "cinematic_timeline".to_string(),
            target_id: "cine.intro".to_string(),
            prompt: "Open the intro cutscene and tweak the subtitle text.".to_string(),
            suggested_edit: "edit_subtitle".to_string(),
        },
        GuidedFirstEditTooltip {
            surface: "camera".to_string(),
            target_id: "camera.establishing".to_string(),
            prompt: "Frame the establishing camera on the stage.".to_string(),
            suggested_edit: "aim_named_camera".to_string(),
        },
        GuidedFirstEditTooltip {
            surface: "play_mode".to_string(),
            target_id: "play.instant".to_string(),
            prompt: "Play the intro and confirm the cutscene reads.".to_string(),
            suggested_edit: "enter_play_mode".to_string(),
        },
    ]
}

pub fn tips_for_gallery_template(
    template_id: &str,
) -> Result<Vec<GuidedFirstEditTooltip>, EngineError> {
    match template_id {
        crate::template_gallery::MELEE_SKIRMISH_TEMPLATE => Ok(melee_skirmish_first_launch_tips()),
        crate::template_gallery::STORY_CINEMATIC_TEMPLATE => {
            Ok(story_cinematic_first_launch_tips())
        }
        other => Err(coach_error(
            "template_id",
            format!("no first-launch tips for unknown template_id={other}"),
        )),
    }
}

impl FirstLaunchCoachSession {
    pub fn start(
        template_id: impl Into<String>,
        tips: Vec<GuidedFirstEditTooltip>,
        started_unix_ms: u64,
    ) -> Result<Self, EngineError> {
        if tips.is_empty() {
            return Err(coach_error(
                "tips",
                "first-launch coach requires at least one tip; empty tips fail loud",
            ));
        }
        if tips.len() < 3 {
            return Err(coach_error(
                "tips",
                format!(
                    "first-launch coach requires >=3 tips for progressive chrome (got {})",
                    tips.len()
                ),
            ));
        }
        for (i, tip) in tips.iter().enumerate() {
            if tip.surface.trim().is_empty()
                || tip.target_id.trim().is_empty()
                || tip.prompt.trim().is_empty()
                || tip.suggested_edit.trim().is_empty()
            {
                return Err(coach_error(
                    "tips.entry",
                    format!("tip[{i}] has empty surface/target/prompt/suggested_edit"),
                ));
            }
        }
        Ok(Self {
            schema: FIRST_LAUNCH_COACH_SCHEMA.to_string(),
            template_id: template_id.into(),
            tips,
            current_index: 0,
            started_unix_ms,
            budget_ms: FIRST_LAUNCH_BUDGET_MS,
            dismissed: false,
            completed: false,
        })
    }

    pub fn start_for_gallery_template(
        template_id: &str,
        started_unix_ms: u64,
    ) -> Result<Self, EngineError> {
        Self::start(
            template_id,
            tips_for_gallery_template(template_id)?,
            started_unix_ms,
        )
    }

    pub fn current_tip(&self) -> Option<&GuidedFirstEditTooltip> {
        if self.dismissed || self.completed {
            return None;
        }
        self.tips.get(self.current_index)
    }

    pub fn remaining_budget_ms(&self, now_unix_ms: u64) -> u64 {
        let elapsed = now_unix_ms.saturating_sub(self.started_unix_ms);
        self.budget_ms.saturating_sub(elapsed)
    }

    pub fn within_budget(&self, now_unix_ms: u64) -> bool {
        self.remaining_budget_ms(now_unix_ms) > 0
    }

    /// Advance after the author completes the current tip. Fails if dismissed,
    /// already complete, or past the last tip without complete.
    pub fn advance(&mut self) -> Result<&GuidedFirstEditTooltip, EngineError> {
        if self.dismissed {
            return Err(coach_error(
                "advance",
                "coach was dismissed; cannot advance",
            ));
        }
        if self.completed {
            return Err(coach_error("advance", "coach already completed"));
        }
        if self.current_index + 1 >= self.tips.len() {
            self.current_index = self.tips.len();
            self.completed = true;
            return Err(coach_error(
                "advance",
                "no further tips; coach marked completed — call snapshot instead",
            ));
        }
        self.current_index += 1;
        self.tips
            .get(self.current_index)
            .ok_or_else(|| coach_error("advance", "internal tip index out of range after advance"))
    }

    /// Mark the final tip done (completes session). Fail loud if not on last tip.
    pub fn complete_final_tip(&mut self) -> Result<(), EngineError> {
        if self.dismissed {
            return Err(coach_error("complete", "coach was dismissed"));
        }
        if self.completed {
            return Err(coach_error("complete", "coach already completed"));
        }
        if self.current_index + 1 != self.tips.len() {
            return Err(coach_error(
                "complete",
                format!(
                    "complete_final_tip only valid on last tip (index={} len={})",
                    self.current_index,
                    self.tips.len()
                ),
            ));
        }
        self.current_index = self.tips.len();
        self.completed = true;
        Ok(())
    }

    pub fn dismiss(&mut self) -> Result<(), EngineError> {
        if self.completed {
            return Err(coach_error("dismiss", "cannot dismiss a completed coach"));
        }
        self.dismissed = true;
        Ok(())
    }

    pub fn status_line(&self, now_unix_ms: u64) -> String {
        if self.dismissed {
            return "First-launch coach dismissed".to_string();
        }
        if self.completed {
            let remaining = self.remaining_budget_ms(now_unix_ms);
            return format!(
                "First-launch complete ({}/{} tips) budget_remaining_ms={remaining}",
                self.tips.len(),
                self.tips.len()
            );
        }
        let tip_n = self.current_index + 1;
        let total = self.tips.len();
        let remaining = self.remaining_budget_ms(now_unix_ms);
        let prompt = self.current_tip().map(|t| t.prompt.as_str()).unwrap_or("");
        format!("First-launch tip {tip_n}/{total} ({remaining}ms left): {prompt}")
    }

    pub fn snapshot(&self, now_unix_ms: u64) -> FirstLaunchCoachSnapshot {
        let tip = self.current_tip();
        FirstLaunchCoachSnapshot {
            schema: FIRST_LAUNCH_COACH_SCHEMA.to_string(),
            template_id: self.template_id.clone(),
            tip_count: self.tips.len(),
            current_index: self.current_index,
            current_surface: tip.map(|t| t.surface.clone()),
            current_target_id: tip.map(|t| t.target_id.clone()),
            current_prompt: tip.map(|t| t.prompt.clone()),
            remaining_budget_ms: self.remaining_budget_ms(now_unix_ms),
            within_budget: self.within_budget(now_unix_ms),
            dismissed: self.dismissed,
            completed: self.completed,
            status_line: self.status_line(now_unix_ms),
        }
    }

    /// Persist session JSON for independent SoT readback.
    pub fn write_to_path(&self, path: &std::path::Path) -> Result<(), EngineError> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| coach_error("persist", format!("serialize coach session failed: {e}")))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                coach_error(
                    "persist",
                    format!("create parent for {}: {e}", path.display()),
                )
            })?;
        }
        std::fs::write(path, bytes)
            .map_err(|e| coach_error("persist", format!("write {}: {e}", path.display())))?;
        Ok(())
    }

    pub fn read_from_path(path: &std::path::Path) -> Result<Self, EngineError> {
        let bytes = std::fs::read(path)
            .map_err(|e| coach_error("read", format!("read {}: {e}", path.display())))?;
        let session: Self = serde_json::from_slice(&bytes)
            .map_err(|e| coach_error("read", format!("parse {}: {e}", path.display())))?;
        if session.schema != FIRST_LAUNCH_COACH_SCHEMA {
            return Err(coach_error(
                "schema",
                format!(
                    "expected schema {FIRST_LAUNCH_COACH_SCHEMA} got {}",
                    session.schema
                ),
            ));
        }
        if session.tips.is_empty() {
            return Err(coach_error("read", "persisted coach has empty tips"));
        }
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn melee_coach_advances_and_completes_within_budget() {
        let t0 = 1_000_000u64;
        let mut session = FirstLaunchCoachSession::start_for_gallery_template(
            crate::template_gallery::MELEE_SKIRMISH_TEMPLATE,
            t0,
        )
        .expect("start");
        assert_eq!(session.current_index, 0);
        assert!(session.within_budget(t0 + 60_000));
        let tip1 = session.advance().expect("tip2");
        assert_eq!(tip1.target_id, "start.player_1");
        let tip2 = session.advance().expect("tip3");
        assert_eq!(tip2.target_id, "play.instant");
        session.complete_final_tip().expect("complete");
        assert!(session.completed);
        let snap = session.snapshot(t0 + 120_000);
        assert!(snap.within_budget);
        assert!(snap.status_line.contains("complete"));
    }

    #[test]
    fn empty_tips_fail_loud() {
        let err = FirstLaunchCoachSession::start("x", vec![], 0).unwrap_err();
        assert!(err.to_string().contains("first-launch coach"));
    }

    #[test]
    fn budget_expires() {
        let t0 = 0u64;
        let session = FirstLaunchCoachSession::start_for_gallery_template(
            crate::template_gallery::MELEE_SKIRMISH_TEMPLATE,
            t0,
        )
        .unwrap();
        assert!(!session.within_budget(FIRST_LAUNCH_BUDGET_MS + 1));
        assert_eq!(session.remaining_budget_ms(FIRST_LAUNCH_BUDGET_MS + 1), 0);
    }
}
