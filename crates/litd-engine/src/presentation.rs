//! One-way extraction from deterministic sim snapshots into presentation/render data.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Aabb, FixedVec3, Transform, Vec3},
    render::batching::{RenderInstance, ScreenRect},
    sim::SimBodyId,
    sim_replay::SimSnapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresentationEntityKind {
    Unit,
    Doodad,
    Destructible,
    Effect,
}

impl PresentationEntityKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Doodad => "doodad",
            Self::Destructible => "destructible",
            Self::Effect => "effect",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationBodyStyle {
    pub body_id: SimBodyId,
    pub entity_kind: PresentationEntityKind,
    pub mesh_id: String,
    pub material_id: String,
    pub half_extents: Vec3,
    pub screen_rect: ScreenRect,
    pub nearest_depth: f32,
    pub index_count: u32,
}

impl PresentationBodyStyle {
    pub fn validate(&self, index: usize) -> Result<(), EngineError> {
        if self.mesh_id.trim().is_empty() {
            return invalid_presentation(format!("styles[{index}].mesh_id"), "must be non-empty");
        }
        if self.material_id.trim().is_empty() {
            return invalid_presentation(
                format!("styles[{index}].material_id"),
                "must be non-empty",
            );
        }
        if !self.half_extents.is_finite()
            || self.half_extents.x <= 0.0
            || self.half_extents.y <= 0.0
            || self.half_extents.z <= 0.0
        {
            return invalid_presentation(
                format!("styles[{index}].half_extents"),
                "must be finite and positive on every axis",
            );
        }
        if self.index_count == 0 {
            return invalid_presentation(
                format!("styles[{index}].index_count"),
                "must be non-zero",
            );
        }
        if !self.screen_rect.min.is_finite()
            || !self.screen_rect.max.is_finite()
            || self.screen_rect.min.x < 0.0
            || self.screen_rect.min.y < 0.0
            || self.screen_rect.max.x > 1.0
            || self.screen_rect.max.y > 1.0
            || self.screen_rect.min.x > self.screen_rect.max.x
            || self.screen_rect.min.y > self.screen_rect.max.y
        {
            return invalid_presentation(
                format!("styles[{index}].screen_rect"),
                "must be finite, normalized, and ordered inside 0..=1",
            );
        }
        if !self.nearest_depth.is_finite() || !(0.0..=1.0).contains(&self.nearest_depth) {
            return invalid_presentation(
                format!("styles[{index}].nearest_depth"),
                "must be finite and normalized inside 0..=1",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationInstance {
    pub body_id: SimBodyId,
    pub entity_kind: PresentationEntityKind,
    pub transform: Transform,
    pub velocity: Vec3,
    pub render_instance: RenderInstance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationFrame {
    pub source_tick: u64,
    pub source_state_hash: u64,
    pub instances: Vec<PresentationInstance>,
}

impl PresentationFrame {
    pub fn render_instances(&self) -> Vec<RenderInstance> {
        self.instances
            .iter()
            .map(|instance| instance.render_instance.clone())
            .collect()
    }

    pub fn lifecycle_diff_from(
        &self,
        previous: Option<&PresentationFrame>,
    ) -> Result<PresentationLifecycleDiff, EngineError> {
        diff_presentation_frames(previous, self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationLifecycleEntry {
    pub body_id: SimBodyId,
    pub entity_kind: PresentationEntityKind,
    pub render_instance_id: u64,
    pub mesh_id: String,
    pub material_id: String,
    pub transform: Transform,
}

impl PresentationLifecycleEntry {
    fn from_instance(instance: &PresentationInstance) -> Self {
        Self {
            body_id: instance.body_id,
            entity_kind: instance.entity_kind,
            render_instance_id: instance.render_instance.id,
            mesh_id: instance.render_instance.mesh_id.clone(),
            material_id: instance.render_instance.material_id.clone(),
            transform: instance.transform,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationLifecycleDiff {
    pub previous_tick: Option<u64>,
    pub current_tick: u64,
    pub created: Vec<PresentationLifecycleEntry>,
    pub updated: Vec<PresentationLifecycleEntry>,
    pub destroyed: Vec<PresentationLifecycleEntry>,
}

impl PresentationLifecycleDiff {
    pub fn is_empty(&self) -> bool {
        self.created.is_empty() && self.updated.is_empty() && self.destroyed.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "created={};updated={};destroyed={}",
            self.created.len(),
            self.updated.len(),
            self.destroyed.len()
        )
    }
}

pub fn diff_presentation_frames(
    previous: Option<&PresentationFrame>,
    current: &PresentationFrame,
) -> Result<PresentationLifecycleDiff, EngineError> {
    validate_unique_instances("current", &current.instances)?;
    let previous_tick = previous.map(|frame| frame.source_tick);
    let mut previous_by_body = BTreeMap::new();
    if let Some(previous) = previous {
        validate_unique_instances("previous", &previous.instances)?;
        for instance in &previous.instances {
            previous_by_body.insert(instance.body_id.index(), instance);
        }
    }

    let mut created = Vec::new();
    let mut updated = Vec::new();
    for instance in &current.instances {
        match previous_by_body.remove(&instance.body_id.index()) {
            None => created.push(PresentationLifecycleEntry::from_instance(instance)),
            Some(previous) if previous != instance => {
                updated.push(PresentationLifecycleEntry::from_instance(instance));
            }
            Some(_) => {}
        }
    }
    let destroyed = previous_by_body
        .values()
        .map(|instance| PresentationLifecycleEntry::from_instance(instance))
        .collect();

    Ok(PresentationLifecycleDiff {
        previous_tick,
        current_tick: current.source_tick,
        created,
        updated,
        destroyed,
    })
}

pub fn extract_presentation_frame(
    snapshot: &SimSnapshot,
    styles: &[PresentationBodyStyle],
) -> Result<PresentationFrame, EngineError> {
    let mut style_by_body = BTreeMap::new();
    for (index, style) in styles.iter().enumerate() {
        style.validate(index)?;
        if style_by_body.insert(style.body_id.index(), style).is_some() {
            return invalid_presentation(
                format!("styles[{index}].body_id"),
                "must be unique per sim body",
            );
        }
    }

    let mut instances = Vec::with_capacity(snapshot.bodies.len());
    for body in &snapshot.bodies {
        let style = style_by_body
            .get(&body.id.index())
            .copied()
            .ok_or_else(|| {
                invalid_presentation_error(
                    "styles",
                    format!("missing presentation style for body {}", body.id.index()),
                )
            })?;
        let translation = fixed_vec3_to_vec3(body.position);
        let velocity = fixed_vec3_to_vec3(body.velocity);
        let world_bounds = Aabb::from_center_half_extents(translation, style.half_extents)?;
        let render_instance = RenderInstance {
            id: u64::from(body.id.index()) + 1,
            mesh_id: style.mesh_id.clone(),
            material_id: style.material_id.clone(),
            world_bounds,
            screen_rect: style.screen_rect,
            nearest_depth: style.nearest_depth,
            index_count: style.index_count,
        };
        render_instance.validate(instances.len())?;
        instances.push(PresentationInstance {
            body_id: body.id,
            entity_kind: style.entity_kind,
            transform: Transform::from_translation(translation),
            velocity,
            render_instance,
        });
    }

    Ok(PresentationFrame {
        source_tick: snapshot.tick,
        source_state_hash: snapshot.state_hash,
        instances,
    })
}

fn fixed_vec3_to_vec3(value: FixedVec3) -> Vec3 {
    Vec3::new(
        value.x.to_f64() as f32,
        value.y.to_f64() as f32,
        value.z.to_f64() as f32,
    )
}

fn validate_unique_instances(
    label: &'static str,
    instances: &[PresentationInstance],
) -> Result<(), EngineError> {
    let mut seen = BTreeMap::new();
    for (index, instance) in instances.iter().enumerate() {
        if seen.insert(instance.body_id.index(), ()).is_some() {
            return invalid_presentation(
                format!("lifecycle.{label}[{index}].body_id"),
                "must be unique per frame",
            );
        }
    }
    Ok(())
}

fn invalid_presentation<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_presentation_error(field, reason))
}

fn invalid_presentation_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::BatchingInvalidConfig {
        field: format!("presentation.{}", field.into()),
        reason: reason.into(),
    }
}
