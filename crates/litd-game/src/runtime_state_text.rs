//! Persisted packaged-host text evidence for exact-HWND Full State Verification.
//!
//! Pixels prove presentation; this state separately proves which font and text
//! plan the live renderer submitted. It is carried by the existing runtime-state
//! heartbeat, whose Aster write/readback/hash discipline is the source of truth.

use serde::{Deserialize, Serialize};

pub const RUNTIME_TEXT_STATE_SCHEMA: &str = "litd.game.runtime_text.state.v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTextErrorState {
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTextState {
    pub schema: String,
    pub font_source_path: String,
    pub font_source_version: String,
    pub font_source_bytes: usize,
    pub font_source_sha256: String,
    pub font_license: String,
    pub font_license_notice_sha256: String,
    pub font_atlas_fnv1a64: String,
    pub session_tick: u64,
    pub plan_fnv1a64: Option<String>,
    pub label_count: usize,
    pub damage_number_count: usize,
    pub rendered_count: usize,
    pub empty_count: usize,
    pub off_screen_count: usize,
    pub planned_glyph_count: usize,
    pub submitted_glyph_count: u32,
    pub submitted_vertex_count: u32,
    pub errors: Vec<RuntimeTextErrorState>,
}

impl RuntimeTextState {
    pub(crate) fn new(
        font_source_path: String,
        font_source_version: String,
        font_source_bytes: usize,
        font_source_sha256: String,
        font_license: String,
        font_license_notice_sha256: String,
        font_atlas_fnv1a64: String,
    ) -> Self {
        Self {
            schema: RUNTIME_TEXT_STATE_SCHEMA.to_string(),
            font_source_path,
            font_source_version,
            font_source_bytes,
            font_source_sha256,
            font_license,
            font_license_notice_sha256,
            font_atlas_fnv1a64,
            session_tick: 0,
            plan_fnv1a64: None,
            label_count: 0,
            damage_number_count: 0,
            rendered_count: 0,
            empty_count: 0,
            off_screen_count: 0,
            planned_glyph_count: 0,
            submitted_glyph_count: 0,
            submitted_vertex_count: 0,
            errors: Vec::new(),
        }
    }

    pub(crate) fn begin_frame(
        &mut self,
        session_tick: u64,
        label_count: usize,
        damage_number_count: usize,
    ) {
        self.session_tick = session_tick;
        self.plan_fnv1a64 = None;
        self.label_count = label_count;
        self.damage_number_count = damage_number_count;
        self.rendered_count = 0;
        self.empty_count = 0;
        self.off_screen_count = 0;
        self.planned_glyph_count = 0;
        self.submitted_glyph_count = 0;
        self.submitted_vertex_count = 0;
        self.errors.clear();
    }

    pub(crate) fn record_plan(
        &mut self,
        fnv1a64: String,
        rendered_count: usize,
        empty_count: usize,
        off_screen_count: usize,
        glyph_count: usize,
    ) {
        self.plan_fnv1a64 = Some(fnv1a64);
        self.rendered_count = rendered_count;
        self.empty_count = empty_count;
        self.off_screen_count = off_screen_count;
        self.planned_glyph_count = glyph_count;
    }

    pub(crate) fn record_submission(&mut self, glyph_count: u32, vertex_count: u32) {
        self.submitted_glyph_count = glyph_count;
        self.submitted_vertex_count = vertex_count;
    }

    pub(crate) fn record_error(&mut self, stage: &'static str, message: String) {
        self.errors.push(RuntimeTextErrorState {
            stage: stage.to_string(),
            message,
        });
    }
}

#[cfg(test)]
#[path = "runtime_state_text_tests.rs"]
mod tests;
