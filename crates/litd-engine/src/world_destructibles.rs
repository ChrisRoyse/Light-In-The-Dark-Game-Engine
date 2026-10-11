//! Destructibles data model (#810): trees, gates, walls, breakables.
//!
//! A destructible is a placed, targetable, breakable world object with integer
//! HP/armor plus a model reference. The authored field persists through the real
//! Calyx content path and lowers into a frozen sim package; the deterministic
//! transition machine lives in [`crate::world_destructibles_sim`], which consumes
//! only the frozen data (never a live Calyx query) so a scripted sim run is
//! reproducible bit-for-bit.

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{
    calyx_content::{EngineContentKind, EngineContentRecord, EngineContentSource},
    world_authoring::{WorldAuthoringError, codec_failed, invalid},
};

pub const DESTRUCTIBLE_FIELD_SCHEMA: &str = "litd.world.destructible_field.v1";
pub const DESTRUCTIBLE_FIELD_SURFACE_KIND: &str = "destructible_field";

const MAX_DESTRUCTIBLES: usize = 4096;
const MAX_HP: u32 = 1_000_000;

/// The kind of destructible. Matches the WC3-parity families named in #810.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestructibleKind {
    Tree,
    Gate,
    Wall,
    Breakable,
}

impl DestructibleKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Gate => "gate",
            Self::Wall => "wall",
            Self::Breakable => "breakable",
        }
    }
}

/// A single placed destructible with its authored combat data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestructibleSpec {
    pub id: String,
    pub kind: DestructibleKind,
    pub tile_x: i32,
    pub tile_z: i32,
    pub yaw_deg: u32,
    /// Full hit points when intact (1..=1_000_000).
    pub max_hp: u32,
    /// Flat damage reduction applied to every incoming hit before HP loss.
    pub armor: u32,
    pub model_ref: String,
}

/// A placed set of destructibles authored as one editor stroke / region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestructibleField {
    pub field_id: String,
    pub display_name: String,
    pub destructibles: Vec<DestructibleSpec>,
}

/// The integer-only content payload persisted to Calyx and lowered for the sim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestructibleFieldPayload {
    pub schema: String,
    pub surface_kind: String,
    pub field_id: String,
    pub destructible_count: u32,
    pub total_max_hp: u64,
    pub destructibles: Vec<DestructibleSpec>,
}

impl DestructibleSpec {
    fn validate(&self) -> Result<(), WorldAuthoringError> {
        if self.id.trim().is_empty() || !is_id_safe(&self.id) {
            return invalid(
                "WORLD_DESTRUCTIBLE_ID",
                "destructibles.id",
                "id must be non-empty ASCII alnum, '_', '-', or '.'",
            );
        }
        if self.model_ref.trim().is_empty() {
            return invalid(
                "WORLD_DESTRUCTIBLE_MODEL",
                "destructibles.model_ref",
                "model_ref must be non-empty",
            );
        }
        if self.max_hp == 0 || self.max_hp > MAX_HP {
            return invalid(
                "WORLD_DESTRUCTIBLE_HP",
                "destructibles.max_hp",
                format!("max_hp must be 1..={MAX_HP}"),
            );
        }
        if self.armor >= self.max_hp {
            return invalid(
                "WORLD_DESTRUCTIBLE_ARMOR",
                "destructibles.armor",
                "armor must be strictly less than max_hp so the object is breakable",
            );
        }
        if self.yaw_deg > 359 {
            return invalid(
                "WORLD_DESTRUCTIBLE_YAW",
                "destructibles.yaw_deg",
                "yaw_deg must be 0..=359",
            );
        }
        Ok(())
    }
}

impl DestructibleField {
    pub fn validate(&self) -> Result<(), WorldAuthoringError> {
        if self.field_id.trim().is_empty() || !is_id_safe(&self.field_id) {
            return invalid(
                "WORLD_DESTRUCTIBLE_FIELD_ID",
                "field_id",
                "field_id must be non-empty ASCII alnum, '_', '-', or '.'",
            );
        }
        if self.display_name.trim().is_empty() {
            return invalid(
                "WORLD_DESTRUCTIBLE_FIELD_NAME",
                "display_name",
                "display_name must be non-empty",
            );
        }
        if self.destructibles.is_empty() || self.destructibles.len() > MAX_DESTRUCTIBLES {
            return invalid(
                "WORLD_DESTRUCTIBLE_COUNT",
                "destructibles",
                format!("destructible count must be 1..={MAX_DESTRUCTIBLES}"),
            );
        }
        let mut seen_ids = std::collections::BTreeSet::new();
        let mut seen_tiles = std::collections::BTreeSet::new();
        for spec in &self.destructibles {
            spec.validate()?;
            if !seen_ids.insert(spec.id.clone()) {
                return invalid(
                    "WORLD_DESTRUCTIBLE_DUP_ID",
                    "destructibles.id",
                    format!("duplicate destructible id {}", spec.id),
                );
            }
            if !seen_tiles.insert((spec.tile_x, spec.tile_z)) {
                return invalid(
                    "WORLD_DESTRUCTIBLE_DUP_TILE",
                    "destructibles.tile",
                    format!(
                        "two destructibles occupy tile {},{}",
                        spec.tile_x, spec.tile_z
                    ),
                );
            }
        }
        Ok(())
    }

    pub fn to_payload(&self) -> Result<DestructibleFieldPayload, WorldAuthoringError> {
        self.validate()?;
        let mut destructibles = self.destructibles.clone();
        destructibles.sort_by(|a, b| a.id.cmp(&b.id));
        let total_max_hp = destructibles
            .iter()
            .map(|spec| u64::from(spec.max_hp))
            .sum::<u64>();
        Ok(DestructibleFieldPayload {
            schema: DESTRUCTIBLE_FIELD_SCHEMA.to_string(),
            surface_kind: DESTRUCTIBLE_FIELD_SURFACE_KIND.to_string(),
            field_id: self.field_id.clone(),
            destructible_count: destructibles.len() as u32,
            total_max_hp,
            destructibles,
        })
    }

    pub fn to_content_record(
        &self,
        author: &str,
    ) -> Result<EngineContentRecord, WorldAuthoringError> {
        if author.trim().is_empty() {
            return invalid(
                "WORLD_DESTRUCTIBLE_AUTHOR",
                "author",
                "author must be non-empty",
            );
        }
        let payload = self.to_payload()?;
        let payload_value = serde_json::to_value(&payload).map_err(codec_failed)?;
        let canonical = serde_json::to_vec(&payload).map_err(codec_failed)?;
        Ok(EngineContentRecord {
            kind: EngineContentKind::Terrain,
            id: self.field_id.clone(),
            display_name: self.display_name.clone(),
            payload: payload_value,
            source: EngineContentSource {
                path: format!("editor://terrain/destructibles/{}", self.field_id),
                author: author.to_string(),
                source_hash: sha256_hex(&canonical),
            },
            anchors: Vec::new(),
        })
    }

    /// Reconstruct a field from a payload that was read back out of the Aster row
    /// (the frozen source of truth the sim consumes). This proves the persisted
    /// bytes -- not the in-memory author object -- drive the deterministic sim.
    pub fn from_payload(payload: &DestructibleFieldPayload) -> Result<Self, WorldAuthoringError> {
        if payload.schema != DESTRUCTIBLE_FIELD_SCHEMA {
            return invalid(
                "WORLD_DESTRUCTIBLE_PAYLOAD_SCHEMA",
                "schema",
                format!("expected {DESTRUCTIBLE_FIELD_SCHEMA}"),
            );
        }
        let field = Self {
            field_id: payload.field_id.clone(),
            display_name: payload.field_id.clone(),
            destructibles: payload.destructibles.clone(),
        };
        field.validate()?;
        Ok(field)
    }
}

fn is_id_safe(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[cfg(test)]
#[path = "world_destructibles_tests.rs"]
pub(crate) mod tests;
