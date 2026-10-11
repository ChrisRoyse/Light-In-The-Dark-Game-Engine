use egui::RichText;
use litd_editor::object_manager::{ObjectManagerEntry, find_references, search_entries};

use super::main_editor_frame::panel_width;
use super::main_editor_object_manager_crossref::{
    ObjectManagerCrossRef, cross_ref_kind_label, object_manager_cross_ref,
};
use super::*;

impl EditorShellRuntime {
    /// Invalidate the memoized Object Manager index. Call after ANY mutation of
    /// `object_manager_project` so the next draw rebuilds it. O(1) — the actual rebuild is
    /// deferred to the next `object_manager_panel` call (#1461).
    ///
    /// `object_manager_project` has no edit path in the current editor (it is a fixed
    /// sample), so this invalidation hook is wired but not yet called — the day an edit
    /// path is added it must call this, or the panel would serve a stale index. Kept
    /// `pub(super)` and allowed as the explicit contract for that future mutation site.
    #[allow(dead_code)]
    pub(super) fn mark_object_manager_dirty(&mut self) {
        self.object_manager_generation = self.object_manager_generation.wrapping_add(1);
    }

    pub(super) fn object_manager_panel(&mut self, ui: &mut egui::Ui) {
        // #1461: memoized — rebuilds only when object_manager_generation advances, not every
        // frame. The clone hands the panel body an owned index so it can still mutate other
        // `self` fields (selection); it is a shallow copy of already-built vectors, far
        // cheaper than the O(project) rebuild + fingerprint serialization it replaces.
        let index = match self
            .object_manager_index_cache
            .get_or_build(self.object_manager_generation, &self.object_manager_project)
        {
            Ok(index) => index.clone(),
            Err(error) => {
                ui.label(RichText::new(format!("Object Manager error: {error}")).color(ERROR));
                return;
            }
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("Object Manager").strong().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} entries", index.entries.len()))
                        .size(11.0)
                        .color(TEXT_DIM),
                );
            });
        });
        ui.label(
            RichText::new(format!(
                "Units {}  Abilities {}  Triggers {}  Regions {}  Assets {}",
                count(&index.counts, "unit"),
                count(&index.counts, "ability"),
                count(&index.counts, "triggers"),
                count(&index.counts, "regions"),
                count(&index.counts, "assets")
            ))
            .size(11.0)
            .color(TEXT_DIM),
        );
        ui.add_sized(
            [panel_width(ui), 24.0],
            egui::TextEdit::singleline(&mut self.object_manager_query).hint_text("Search"),
        );
        let results = results(&index.entries, &self.object_manager_query);
        egui::ScrollArea::vertical()
            .id_salt("object_manager_results")
            .max_height(86.0)
            .show(ui, |ui| {
                for entry in results.iter().take(8) {
                    let selected = self.object_manager_selected_id == entry.id;
                    let label = format!(
                        "{}  /  {}  refs={}",
                        entry.kind, entry.label, entry.reference_count
                    );
                    if ui.selectable_label(selected, label).clicked() {
                        self.object_manager_selected_id = entry.id.clone();
                    }
                }
            });
        if !index
            .entries
            .iter()
            .any(|entry| entry.id == self.object_manager_selected_id)
            && let Some(entry) = results.first()
        {
            self.object_manager_selected_id = entry.id.clone();
        }
        let refs = find_references(&index, &self.object_manager_selected_id);
        ui.label(
            RichText::new(format!("References: {}", self.object_manager_selected_id))
                .size(11.0)
                .strong()
                .color(GOLD),
        );
        if refs.is_empty() {
            ui.label(RichText::new("No references").size(11.0).color(TEXT_DIM));
        } else {
            for reference in refs.iter().take(4) {
                ui.label(
                    RichText::new(format!("{} / {}", reference.source_id, reference.field))
                        .size(10.0)
                        .color(TEXT_DIM),
                );
            }
        }
        let orphan_text = if index.orphan_ids.is_empty() {
            "Orphans: none".to_string()
        } else {
            format!("Orphans: {}", index.orphan_ids.join(","))
        };
        ui.label(RichText::new(orphan_text).size(10.0).color(TEXT_DIM));
        // #835 / #736: trigger cross-reference for the selected entry, recomputed
        // from the project's trigger graphs (no side index; #1348). A trigger row
        // lists the objects it points at; an object row lists the triggers that
        // point at it. All computation lives in the data-provider layer.
        let selected_kind = index
            .entries
            .iter()
            .find(|entry| entry.id == self.object_manager_selected_id)
            .map(|entry| entry.kind.clone())
            .unwrap_or_default();
        self.object_manager_cross_ref_section(ui, &selected_kind);
    }

    fn object_manager_cross_ref_section(&self, ui: &mut egui::Ui, selected_kind: &str) {
        ui.label(
            RichText::new("Trigger cross-reference")
                .size(11.0)
                .strong()
                .color(GOLD),
        );
        let answer = object_manager_cross_ref(
            &self.object_manager_project.triggers,
            &self.object_manager_selected_id,
            selected_kind,
        );
        match answer {
            ObjectManagerCrossRef::TriggerReferences { objects, .. } => {
                if objects.is_empty() {
                    ui.label(
                        RichText::new("References no objects")
                            .size(10.0)
                            .color(TEXT_DIM),
                    );
                } else {
                    for object in objects.iter().take(6) {
                        ui.label(
                            RichText::new(format!(
                                "-> {} {}",
                                cross_ref_kind_label(object.object_kind),
                                object.object_id
                            ))
                            .size(10.0)
                            .color(TEXT_DIM),
                        );
                    }
                }
            }
            ObjectManagerCrossRef::ObjectReferencedBy { trigger_ids, .. } => {
                if trigger_ids.is_empty() {
                    ui.label(
                        RichText::new("No triggers reference this object")
                            .size(10.0)
                            .color(TEXT_DIM),
                    );
                } else {
                    for trigger_id in trigger_ids.iter().take(6) {
                        ui.label(
                            RichText::new(format!("<- {trigger_id}"))
                                .size(10.0)
                                .color(TEXT_DIM),
                        );
                    }
                }
            }
            ObjectManagerCrossRef::NotCrossReferenceable { .. } => {
                ui.label(
                    RichText::new("Not trigger-cross-referenceable")
                        .size(10.0)
                        .color(TEXT_DIM),
                );
            }
            ObjectManagerCrossRef::Error(reason) => {
                ui.label(
                    RichText::new(format!("Cross-ref error: {reason}"))
                        .size(10.0)
                        .color(ERROR),
                );
            }
        }
    }
}

fn results(entries: &[ObjectManagerEntry], query: &str) -> Vec<ObjectManagerEntry> {
    if query.trim().is_empty() {
        entries.iter().take(8).cloned().collect()
    } else {
        let index = litd_editor::object_manager::ObjectManagerIndex {
            counts: Default::default(),
            entries: entries.to_vec(),
            references: Vec::new(),
            orphan_ids: Vec::new(),
            fingerprint: String::new(),
        };
        search_entries(&index, query, 8)
    }
}

fn count(counts: &std::collections::BTreeMap<String, usize>, key: &str) -> usize {
    counts.get(key).copied().unwrap_or(0)
}
