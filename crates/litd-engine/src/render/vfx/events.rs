use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::Vec3,
    render::vfx::{
        VfxRenderCue, VfxResolvedRenderCue, invalid_vfx, resolve_render_cue_effect,
        validate_render_cue,
    },
    sim::SimBodyId,
    sim_replay::SimSnapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfxRenderEventKind {
    Spawn,
    Impact,
    Death,
    SpellCue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxRenderEvent {
    pub event_id: String,
    pub kind: VfxRenderEventKind,
    pub cue: VfxRenderCue,
    pub world_position: Vec3,
    pub source_tick: u64,
    pub source_state_hash: u64,
    pub source_body_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxRenderEventDrop {
    pub event_id: String,
    pub kind: VfxRenderEventKind,
    pub effect_id: String,
    pub reason: String,
    pub capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxRenderEventEnqueueReport {
    pub accepted: bool,
    pub pending_events: usize,
    pub dropped_total: u64,
    pub dropped: Option<VfxRenderEventDrop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxRenderEventDrain {
    pub events: Vec<VfxRenderEvent>,
    pub drained_count: usize,
    pub dropped_total: u64,
    pub source_of_truth: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxResolvedRenderEvent {
    pub event: VfxRenderEvent,
    pub cue_resolution: VfxResolvedRenderCue,
    pub source_of_truth: String,
}

#[derive(Debug, Clone)]
pub struct VfxRenderEventQueue {
    capacity: usize,
    events: Vec<VfxRenderEvent>,
    dropped_total: u64,
}

impl VfxRenderEvent {
    pub fn from_snapshot(
        snapshot: &SimSnapshot,
        kind: VfxRenderEventKind,
        event_id: impl Into<String>,
        cue: VfxRenderCue,
        world_position: Vec3,
        source_body: Option<SimBodyId>,
    ) -> Result<Self, EngineError> {
        let event = Self {
            event_id: event_id.into(),
            kind,
            cue,
            world_position,
            source_tick: snapshot.tick,
            source_state_hash: snapshot.state_hash,
            source_body_id: source_body.map(SimBodyId::index),
        };
        validate_render_event(&event)?;
        Ok(event)
    }
}

impl VfxRenderEventQueue {
    pub fn new(capacity: usize) -> Result<Self, EngineError> {
        if capacity == 0 {
            return invalid_vfx("render_event_queue.capacity", "capacity must be non-zero");
        }
        Ok(Self {
            capacity,
            events: Vec::with_capacity(capacity),
            dropped_total: 0,
        })
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn pending_count(&self) -> usize {
        self.events.len()
    }

    pub fn dropped_total(&self) -> u64 {
        self.dropped_total
    }

    pub fn enqueue(
        &mut self,
        event: VfxRenderEvent,
    ) -> Result<VfxRenderEventEnqueueReport, EngineError> {
        validate_render_event(&event)?;
        if self.events.len() == self.capacity {
            self.dropped_total = self.dropped_total.checked_add(1).ok_or_else(|| {
                crate::render::vfx::invalid_vfx_error(
                    "render_event_queue.dropped_total",
                    "drop counter overflowed",
                )
            })?;
            return Ok(VfxRenderEventEnqueueReport {
                accepted: false,
                pending_events: self.events.len(),
                dropped_total: self.dropped_total,
                dropped: Some(VfxRenderEventDrop {
                    event_id: event.event_id,
                    kind: event.kind,
                    effect_id: event.cue.effect.effect_id,
                    reason: "queue_capacity_exceeded".to_string(),
                    capacity: self.capacity,
                }),
            });
        }
        self.events.push(event);
        Ok(VfxRenderEventEnqueueReport {
            accepted: true,
            pending_events: self.events.len(),
            dropped_total: self.dropped_total,
            dropped: None,
        })
    }

    pub fn drain(&mut self) -> VfxRenderEventDrain {
        let events = std::mem::take(&mut self.events);
        VfxRenderEventDrain {
            drained_count: events.len(),
            events,
            dropped_total: self.dropped_total,
            source_of_truth: "render-only VFX event queue".to_string(),
        }
    }
}

pub fn resolve_render_event_effect(
    root: impl AsRef<Path>,
    event: &VfxRenderEvent,
) -> Result<VfxResolvedRenderEvent, EngineError> {
    validate_render_event(event)?;
    let cue_resolution = resolve_render_cue_effect(root, &event.cue)?;
    Ok(VfxResolvedRenderEvent {
        event: event.clone(),
        cue_resolution,
        source_of_truth: "drained VFX render event + verified content-store cue".to_string(),
    })
}

pub fn validate_render_event(event: &VfxRenderEvent) -> Result<(), EngineError> {
    validate_id("render_event.event_id", &event.event_id)?;
    validate_render_cue(&event.cue)?;
    if !event.world_position.is_finite() {
        return invalid_vfx(
            "render_event.world_position",
            "world position must be finite",
        );
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_vfx(field, "id must be non-empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return invalid_vfx(
            field,
            "id must contain only ASCII letters, digits, '.', '_', '-', or ':'",
        );
    }
    Ok(())
}
