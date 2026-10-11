//! Live Trigger Project Save/Open/Reopen controls over strict #803 persistence.

use std::path::{Path, PathBuf};

use egui::RichText;
use litd_editor::trigger_editor::{
    TriggerEditorReadback, read_trigger_editor_project, save_trigger_editor_project,
};

use super::{ERROR, EditorShellRuntime, GOOD, TEXT_DIM};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TriggerProjectFileState {
    pub(super) path_input: String,
    pub(super) opened_path: Option<PathBuf>,
    pub(super) dirty: bool,
}

impl Default for TriggerProjectFileState {
    fn default() -> Self {
        Self {
            path_input: "trigger-project.json".to_string(),
            opened_path: None,
            dirty: true,
        }
    }
}

pub(super) fn project_file_controls(ui: &mut egui::Ui, runtime: &mut EditorShellRuntime) {
    let mut save_requested = false;
    let mut open_requested = false;
    let mut reopen_requested = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Project file").size(10.0).color(TEXT_DIM));
        ui.add(
            egui::TextEdit::singleline(&mut runtime.trigger_editor_file.path_input)
                .desired_width(220.0),
        );
        if ui.small_button("Save").clicked() {
            save_requested = true;
        }
        let open_label = if runtime.trigger_editor_file.dirty {
            "Open / discard edits"
        } else {
            "Open"
        };
        if ui.small_button(open_label).clicked() {
            open_requested = true;
        }
        let reopen_label = if runtime.trigger_editor_file.dirty {
            "Reopen / discard edits"
        } else {
            "Reopen"
        };
        if ui
            .add_enabled(
                runtime.trigger_editor_file.opened_path.is_some(),
                egui::Button::new(reopen_label).small(),
            )
            .clicked()
        {
            reopen_requested = true;
        }
        ui.label(
            RichText::new(if runtime.trigger_editor_file.dirty {
                "unsaved"
            } else {
                "saved"
            })
            .size(10.0)
            .color(if runtime.trigger_editor_file.dirty {
                ERROR
            } else {
                GOOD
            }),
        );
    });
    if save_requested {
        save_project(runtime);
    } else if open_requested {
        open_project(runtime);
    } else if reopen_requested {
        reopen_project(runtime);
    }
}

pub(super) fn save_project(runtime: &mut EditorShellRuntime) {
    let Some(path) = input_path(runtime, "save") else {
        return;
    };
    match save_trigger_editor_project(&path, &runtime.trigger_editor_project) {
        Ok(readback) => {
            runtime.trigger_editor_file.path_input = readback.path.clone();
            runtime.trigger_editor_file.opened_path = Some(path);
            runtime.trigger_editor_file.dirty = false;
            runtime.trigger_editor_last_action = readback_status("saved", &readback);
        }
        Err(error) => persistence_failure(runtime, "save", &path, error.to_string()),
    }
}

pub(super) fn open_project(runtime: &mut EditorShellRuntime) {
    let Some(path) = input_path(runtime, "open") else {
        return;
    };
    load_project(runtime, path, "opened");
}

pub(super) fn reopen_project(runtime: &mut EditorShellRuntime) {
    let Some(path) = runtime.trigger_editor_file.opened_path.clone() else {
        persistence_failure(
            runtime,
            "reopen",
            &PathBuf::new(),
            "no Trigger Project has been saved or opened".to_string(),
        );
        return;
    };
    load_project(runtime, path, "reopened");
}

fn load_project(runtime: &mut EditorShellRuntime, path: PathBuf, verb: &str) {
    match read_trigger_editor_project(&path) {
        Ok(readback) => {
            let status = readback_status(verb, &readback);
            runtime.trigger_editor_project = readback.project;
            runtime.trigger_editor_selected_graph = 0;
            runtime.trigger_editor_generation = runtime.trigger_editor_generation.wrapping_add(1);
            runtime.trigger_editor_file.path_input = readback.path;
            runtime.trigger_editor_file.opened_path = Some(path);
            runtime.trigger_editor_file.dirty = false;
            runtime.trigger_editor_last_action = status;
        }
        Err(error) => persistence_failure(runtime, verb, &path, error.to_string()),
    }
}

fn input_path(runtime: &mut EditorShellRuntime, operation: &str) -> Option<PathBuf> {
    let raw = runtime.trigger_editor_file.path_input.trim();
    if raw.is_empty() {
        persistence_failure(
            runtime,
            operation,
            &PathBuf::new(),
            "Trigger Project path must not be empty".to_string(),
        );
        return None;
    }
    Some(PathBuf::from(raw))
}

fn readback_status(verb: &str, readback: &TriggerEditorReadback) -> String {
    format!(
        "{verb} Trigger Project {} ({} bytes, sha256={})",
        readback.path, readback.bytes, readback.sha256
    )
}

fn persistence_failure(
    runtime: &mut EditorShellRuntime,
    operation: &str,
    path: &Path,
    error: String,
) {
    runtime.trigger_editor_last_action = format!("Trigger Project {operation} failed: {error}");
    tracing::error!(
        operation,
        path = %path.display(),
        error = %error,
        "Trigger Project persistence failed"
    );
}

#[cfg(test)]
mod tests {
    use litd_engine::test_temp::TestTempDir;

    use super::*;
    use crate::main_editor_ui::EditorShellRuntime;

    #[test]
    fn save_then_reopen_restores_project_and_clears_dirty_state() {
        let root = TestTempDir::new("trigger-project-ui-save-reopen");
        let path = root.path().join("project.json");
        let mut runtime = EditorShellRuntime::new(1920, 1080).expect("runtime");
        runtime.trigger_editor_file.path_input = path.display().to_string();

        save_project(&mut runtime);

        assert!(path.is_file());
        assert!(!runtime.trigger_editor_file.dirty);
        assert_eq!(
            runtime.trigger_editor_file.opened_path.as_ref(),
            Some(&path)
        );
        let saved = runtime.trigger_editor_project.clone();
        runtime.trigger_editor_project.graphs[0].enabled = false;
        runtime.mark_trigger_editor_dirty();
        assert!(runtime.trigger_editor_file.dirty);

        reopen_project(&mut runtime);

        assert_eq!(runtime.trigger_editor_project, saved);
        assert!(!runtime.trigger_editor_file.dirty);
        assert!(runtime.trigger_editor_last_action.starts_with("reopened"));
    }

    #[test]
    fn open_adopts_strict_readback_and_authoritative_path() {
        let root = TestTempDir::new("trigger-project-ui-open");
        let path = root.path().join("project.json");
        let mut expected =
            litd_editor::trigger_editor::issue733_sample_project().expect("sample project");
        expected.graphs[0].display_name = "Opened Project".to_string();
        save_trigger_editor_project(&path, &expected).expect("seed project");
        let mut runtime = EditorShellRuntime::new(1920, 1080).expect("runtime");
        runtime.trigger_editor_project.graphs[0].display_name = "Unsaved".to_string();
        runtime.mark_trigger_editor_dirty();
        runtime.trigger_editor_file.path_input = path.display().to_string();

        open_project(&mut runtime);

        assert_eq!(runtime.trigger_editor_project, expected);
        assert_eq!(
            runtime.trigger_editor_file.opened_path.as_ref(),
            Some(&path)
        );
        assert!(!runtime.trigger_editor_file.dirty);
        assert!(runtime.trigger_editor_last_action.starts_with("opened"));
    }

    #[test]
    fn failed_open_preserves_live_project_and_file_state() {
        let root = TestTempDir::new("trigger-project-ui-failed-open");
        let missing = root.path().join("missing.json");
        let mut runtime = EditorShellRuntime::new(1920, 1080).expect("runtime");
        runtime.trigger_editor_file.path_input = missing.display().to_string();
        let project_before = runtime.trigger_editor_project.clone();
        let file_before = runtime.trigger_editor_file.clone();
        let generation_before = runtime.trigger_editor_generation;

        open_project(&mut runtime);

        assert_eq!(runtime.trigger_editor_project, project_before);
        assert_eq!(runtime.trigger_editor_file, file_before);
        assert_eq!(runtime.trigger_editor_generation, generation_before);
        assert!(
            runtime
                .trigger_editor_last_action
                .contains("Trigger Project open failed")
        );
    }

    #[test]
    fn empty_path_and_reopen_without_source_fail_loud_without_mutation() {
        let mut runtime = EditorShellRuntime::new(1920, 1080).expect("runtime");
        runtime.trigger_editor_file.path_input.clear();
        let project_before = runtime.trigger_editor_project.clone();
        let file_before = runtime.trigger_editor_file.clone();

        save_project(&mut runtime);
        assert!(
            runtime
                .trigger_editor_last_action
                .contains("must not be empty")
        );
        assert_eq!(runtime.trigger_editor_project, project_before);
        assert_eq!(runtime.trigger_editor_file, file_before);

        reopen_project(&mut runtime);
        assert!(
            runtime
                .trigger_editor_last_action
                .contains("no Trigger Project")
        );
        assert_eq!(runtime.trigger_editor_project, project_before);
        assert_eq!(runtime.trigger_editor_file, file_before);
    }
}
