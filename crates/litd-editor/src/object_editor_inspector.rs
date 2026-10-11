//! #1485 finding 6: memoized view model for the Object Editor inspector panel.
//!
//! `object_controls` is immediate-mode and runs every frame the Objects inspector is visible.
//! It used to rebuild, every frame, a `Vec<(id, name, kind)>` over ALL objects, clone the
//! selected object, and run `validate_unit_object_for_lowering` on it — pure waste, because the
//! output changes only when the object set or the selection is edited. This cache stores the
//! built view keyed by a monotonic generation counter; it rebuilds only when the generation
//! advances (bumped by `mark_object_editor_dirty` at every object add/remove/edit/selection
//! change), mirroring the `ObjectManagerIndexCache` / `TriggerGraphRonBytesCache` pattern.

use crate::history::{EditorDocument, EditorObject};
use crate::object_editor::{ObjectEditorLiveValidation, validate_unit_object_for_lowering};

/// The fully-resolved data the inspector panel renders for one generation of the document +
/// selection. Cheap to hand back by reference; the panel only reads it.
#[derive(Debug, Clone, Default)]
pub struct ObjectEditorInspectorView {
    /// One row per object: `(id, display_name, kind)`. Deterministic order (the document's
    /// `objects` map is a `BTreeMap`, so this is sorted by id).
    pub browser_rows: Vec<(String, String, String)>,
    /// The selected object's display name, or `None` when the selection no longer resolves
    /// (the panel then shows "Missing selection").
    pub selected_display_name: Option<String>,
    /// Lowering validation for the selected object, or `None` when the selection is missing.
    pub validation: Option<ObjectEditorLiveValidation>,
}

fn display_name(object: &EditorObject) -> String {
    object
        .properties
        .get("object.display_name")
        .cloned()
        .unwrap_or_else(|| object.id.clone())
}

fn build_view(document: &EditorDocument, selected_id: &str) -> ObjectEditorInspectorView {
    let browser_rows = document
        .objects
        .values()
        .map(|object| (object.id.clone(), display_name(object), object.kind.clone()))
        .collect();
    let (selected_display_name, validation) = match document.objects.get(selected_id) {
        Some(object) => (
            Some(display_name(object)),
            Some(validate_unit_object_for_lowering(object)),
        ),
        None => (None, None),
    };
    ObjectEditorInspectorView {
        browser_rows,
        selected_display_name,
        validation,
    }
}

/// Generation-keyed cache of the inspector view. Rebuilds only when the generation differs
/// from the cached one (or nothing is cached yet); an unchanged generation returns the stored
/// view with no rebuild, no clones, and no validation pass.
#[derive(Debug, Default, Clone)]
pub struct ObjectEditorInspectorCache {
    cached: Option<(u64, ObjectEditorInspectorView)>,
}

impl ObjectEditorInspectorCache {
    /// Return the inspector view for `document`/`selected_id` at `generation`, rebuilding only
    /// when the cached generation differs. Callers MUST bump the generation on every mutation
    /// of the object set or selection, or the cache would serve a stale inspector.
    pub fn get_or_build(
        &mut self,
        generation: u64,
        document: &EditorDocument,
        selected_id: &str,
    ) -> &ObjectEditorInspectorView {
        let stale = match &self.cached {
            Some((cached_generation, _)) => *cached_generation != generation,
            None => true,
        };
        if stale {
            let view = build_view(document, selected_id);
            self.cached = Some((generation, view));
        }
        &self
            .cached
            .as_ref()
            .expect("cache populated above when stale")
            .1
    }

    /// The generation whose view is currently cached, or `None` if nothing is cached. Exposed
    /// for tests that need to prove a rebuild did or did not happen.
    pub fn cached_generation(&self) -> Option<u64> {
        self.cached.as_ref().map(|(generation, _)| *generation)
    }
}

#[cfg(test)]
#[path = "object_editor_inspector_tests.rs"]
mod object_editor_inspector_tests;
