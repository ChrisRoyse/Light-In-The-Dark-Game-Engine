use egui::RichText;

use super::main_editor_frame::panel_width;
use super::*;

/// A button press captured while the inspector cache is borrowed, applied afterwards so the
/// `&mut self` edit never overlaps the immutable borrow of the cache.
enum PendingObjectEdit {
    MoveSpeed,
    AddLeap,
    Revert,
}

impl EditorShellRuntime {
    /// #1485 finding 6: invalidate the memoized Object Editor inspector view. MUST be called
    /// after ANY mutation of the object set OR the current selection, or the next draw would
    /// serve a stale browser list / validation. O(1); the rebuild is deferred to the next
    /// `object_controls` call. Mirrors `mark_object_manager_dirty` / `mark_trigger_editor_dirty`.
    pub(super) fn mark_object_editor_dirty(&mut self) {
        self.object_editor_generation = self.object_editor_generation.wrapping_add(1);
    }

    pub(super) fn object_controls(&mut self, ui: &mut egui::Ui) {
        let compact = panel_width(ui) < 260.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Object Browser").strong().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Duplicate").clicked() {
                    self.duplicate_object_editor_selection();
                }
            });
        });

        // #1485 finding 6: the browser rows come from the generation-keyed cache instead of a
        // per-frame rebuild. The row list is borrowed (never cloned); a click is recorded into a
        // local and applied after the borrow ends, so no `&mut self` call overlaps the borrow.
        let generation = self.object_editor_generation;
        let mut newly_selected: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("object_editor_browser")
            .max_height(if compact { 88.0 } else { 116.0 })
            .show(ui, |ui| {
                let view = self.object_editor_inspector_cache.get_or_build(
                    generation,
                    &self.object_editor_document,
                    &self.object_editor_selected_object,
                );
                for (id, label, kind) in &view.browser_rows {
                    let selected = self.object_editor_selected_object == *id;
                    let text = if compact {
                        format!("{label}  {kind}")
                    } else {
                        format!("{label}  /  {kind}")
                    };
                    if ui.selectable_label(selected, text).clicked() {
                        newly_selected = Some(id.clone());
                    }
                }
            });
        if let Some(id) = newly_selected {
            self.object_editor_selected_object = id;
            self.mark_object_editor_dirty();
        }
        ui.separator();
        self.object_manager_panel(ui);
        ui.separator();

        // Selected-object inspector, also served from the cache (validation is NOT re-run per
        // frame). Everything that reads the cached view stays inside the borrow scope; only the
        // button-press outcome escapes, applied after the borrow ends.
        let mut pending: Option<PendingObjectEdit> = None;
        let missing_selection = {
            let view = self.object_editor_inspector_cache.get_or_build(
                self.object_editor_generation,
                &self.object_editor_document,
                &self.object_editor_selected_object,
            );
            match (
                view.selected_display_name.as_ref(),
                view.validation.as_ref(),
            ) {
                (Some(display_name), Some(validation)) => {
                    ui.label(
                        RichText::new(display_name.as_str())
                            .size(13.0)
                            .strong()
                            .color(GOLD),
                    );
                    let status = if validation.valid {
                        ("LowerUnit valid", GOOD)
                    } else {
                        ("LowerUnit blocked", ERROR)
                    };
                    ui.label(RichText::new(status.0).color(status.1).size(12.0));
                    egui::Grid::new("object_editor_fields")
                        .num_columns(3)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            for field in &validation.fields {
                                ui.label(
                                    RichText::new(field.field.as_str())
                                        .size(11.0)
                                        .color(TEXT_DIM),
                                );
                                ui.label(
                                    RichText::new(field.value.as_str()).size(11.0).color(TEXT),
                                );
                                ui.label(
                                    RichText::new(if field.valid { "ok" } else { "bad" })
                                        .size(11.0)
                                        .color(if field.valid { GOOD } else { ERROR }),
                                );
                                ui.end_row();
                            }
                        });
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Move 310").clicked() {
                            pending = Some(PendingObjectEdit::MoveSpeed);
                        }
                        if ui.button("Add Leap").clicked() {
                            pending = Some(PendingObjectEdit::AddLeap);
                        }
                        if ui.button("Revert").clicked() {
                            pending = Some(PendingObjectEdit::Revert);
                        }
                    });
                    false
                }
                _ => true,
            }
        };
        if missing_selection {
            ui.label(RichText::new("Missing selection").color(ERROR));
            return;
        }
        match pending {
            Some(PendingObjectEdit::MoveSpeed) => {
                self.set_object_editor_property("stats.move_speed", "310");
            }
            Some(PendingObjectEdit::AddLeap) => {
                self.set_object_editor_property(
                    "abilities.normal",
                    "ability.issue732.defend,ability.issue732.guardian_leap",
                );
            }
            Some(PendingObjectEdit::Revert) => self.revert_object_editor_selection(),
            None => {}
        }
        ui.label(
            RichText::new(format!("Action: {}", self.object_editor_last_action))
                .size(11.0)
                .color(TEXT_DIM),
        );
    }
}
