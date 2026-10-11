//! Constructors + validation for the editor map-entity value types (#1466 split from
//! `history.rs`). `EditorDocument`, the undo/redo stack, and the snapshot/serde helpers stay in
//! `history`; this holds the per-entity `new` / `with_property` / `validate` impls. As a child
//! module of `history` it sees the entity structs' fields directly, and calls the shared
//! `super::invalid_history` error constructor.

use std::collections::BTreeMap;

use litd_engine::{EngineError, math::Transform};

use super::{EditorObject, EditorRegion, EditorTerrainStamp, SelectionSet, invalid_history};

impl EditorObject {
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        parent: Option<String>,
        transform: Transform,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            parent,
            transform,
            properties: BTreeMap::new(),
        }
    }

    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    pub(super) fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() || self.kind.trim().is_empty() {
            return invalid_history("object", "id and kind must be non-empty");
        }
        if !self.transform.translation.is_finite()
            || !self.transform.rotation.is_finite()
            || !self.transform.scale.is_finite()
        {
            return invalid_history("object.transform", "transform must be finite");
        }
        Ok(())
    }
}

impl EditorRegion {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        center: litd_engine::math::Vec3,
        half_extents: litd_engine::math::Vec3,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            center,
            half_extents,
            properties: BTreeMap::new(),
        }
    }

    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    pub(super) fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return invalid_history("region", "id and name must be non-empty");
        }
        if !self.center.is_finite() || !self.half_extents.is_finite() {
            return invalid_history("region.bounds", "center and half_extents must be finite");
        }
        if self.half_extents.x <= 0.0 || self.half_extents.y <= 0.0 || self.half_extents.z <= 0.0 {
            return invalid_history("region.bounds", "half_extents must be positive");
        }
        Ok(())
    }
}

impl EditorTerrainStamp {
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        transform: Transform,
        width_tiles: u32,
        height_tiles: u32,
        height_delta: f32,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            transform,
            width_tiles,
            height_tiles,
            height_delta,
            properties: BTreeMap::new(),
        }
    }

    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    pub(super) fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty() || self.kind.trim().is_empty() {
            return invalid_history("terrain_stamp", "id and kind must be non-empty");
        }
        if self.width_tiles == 0 || self.height_tiles == 0 {
            return invalid_history("terrain_stamp.size", "width and height must be positive");
        }
        if !self.height_delta.is_finite() {
            return invalid_history("terrain_stamp.height_delta", "must be finite");
        }
        if !self.transform.translation.is_finite()
            || !self.transform.rotation.is_finite()
            || !self.transform.scale.is_finite()
        {
            return invalid_history("terrain_stamp.transform", "transform must be finite");
        }
        Ok(())
    }
}

impl SelectionSet {
    pub fn select_many(&mut self, ids: impl IntoIterator<Item = impl Into<String>>) {
        for id in ids {
            self.selected_ids.insert(id.into());
        }
    }

    pub fn toggle(&mut self, id: impl Into<String>) {
        let id = id.into();
        if !self.selected_ids.remove(&id) {
            self.selected_ids.insert(id);
        }
    }

    pub fn clear(&mut self) {
        self.selected_ids.clear();
    }

    pub fn summary(&self) -> String {
        self.selected_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("|")
    }
}
