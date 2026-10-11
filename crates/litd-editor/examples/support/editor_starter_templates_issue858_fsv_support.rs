use std::{fs, path::PathBuf};

use litd_editor::{
    history::stable_fingerprint,
    starter_templates::{
        StarterTemplateKind, new_project_from_template, run_starter_template_headless,
        starter_template, validate_starter_template,
    },
    trigger_wasm_authoring::{
        WASM_GUI_SCHEMA, WasmGuiOperation, WasmGuiProgram, lower_gui_program,
    },
};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub state: PathBuf,
    pub projects: PathBuf,
}

impl Paths {
    pub fn new(root: PathBuf) -> Self {
        Self {
            state: root.join("project-state"),
            projects: root.join("projects"),
            root,
        }
    }

    pub fn project_path(&self, safe: &str) -> PathBuf {
        self.projects.join(format!("{safe}.litdworld"))
    }

    pub fn to_json(&self) -> Value {
        json!({
            "root": self.root,
            "state": self.state,
            "projects": self.projects
        })
    }
}

pub fn safe_kind(kind: StarterTemplateKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}

pub fn file_sha256(path: impl Into<PathBuf>) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path.into())?))
}

pub fn clone_independence_edge() -> Result<Value, Box<dyn std::error::Error>> {
    let template = starter_template(StarterTemplateKind::Orpg)?;
    let template_before = stable_fingerprint(&template.document)?;
    let mut clone = new_project_from_template(
        StarterTemplateKind::Orpg,
        "issue858.clone.edge",
        "Issue 858 Clone Edge",
        "issue858-clone-edge-rev-1",
    )?;
    let clone_before = stable_fingerprint(&clone.project.document)?;
    clone
        .project
        .document
        .objects
        .get_mut("orpg_hero")
        .ok_or("missing orpg_hero")?
        .properties
        .insert("stat.damage".to_string(), "9".to_string());
    let clone_after = stable_fingerprint(&clone.project.document)?;
    let template_after = stable_fingerprint(&template.document)?;
    let passed = template_before == template_after && clone_before != clone_after;
    Ok(json!({
        "edge": "clone_independent_after_copy_tweak",
        "passed": passed,
        "template_before": template_before,
        "template_after": template_after,
        "clone_before": clone_before,
        "clone_after": clone_after,
        "mutated_property": "orpg_hero.stat.damage"
    }))
}

pub fn wasm_escape_rejected_edge() -> Result<Value, Box<dyn std::error::Error>> {
    let mut template = starter_template(StarterTemplateKind::MobaAos)?;
    template.trigger_project.graphs[0].wasm_escape = Some(lower_gui_program(&WasmGuiProgram {
        schema: WASM_GUI_SCHEMA.to_string(),
        script_id: "issue858_bad_escape".to_string(),
        entry_point: "run".to_string(),
        fuel_budget: 1_000,
        approved_reason: "starter templates must stay no-code".to_string(),
        operations: vec![WasmGuiOperation::SetFixed { slot: 0, raw: 1 }],
    })?);
    let error = validate_starter_template(&template)
        .unwrap_err()
        .to_string();
    Ok(json!({
        "edge": "wasm_escape_rejected",
        "passed": error.contains("editor.starter_templates.eca.wasm_escape"),
        "error": error
    }))
}

pub fn unsupported_object_rejected_edge() -> Result<Value, Box<dyn std::error::Error>> {
    let mut template = starter_template(StarterTemplateKind::HeroDefenseSurvival)?;
    template
        .document
        .objects
        .get_mut("defense_paladin")
        .ok_or("missing defense_paladin")?
        .kind = "RawScript".to_string();
    let error = validate_starter_template(&template)
        .unwrap_err()
        .to_string();
    Ok(json!({
        "edge": "unsupported_object_kind_rejected",
        "passed": error.contains("editor.starter_templates.object.kind"),
        "error": error
    }))
}

pub fn invalid_stat_rejected_edge() -> Result<Value, Box<dyn std::error::Error>> {
    let mut clone = new_project_from_template(
        StarterTemplateKind::MiniGameParty,
        "issue858.invalid-stat.edge",
        "Issue 858 Invalid Stat Edge",
        "issue858-invalid-stat-rev-1",
    )?;
    clone
        .project
        .document
        .objects
        .get_mut("party_runner")
        .ok_or("missing party_runner")?
        .properties
        .insert("stat.health".to_string(), "-1".to_string());
    let error = run_starter_template_headless(&clone, 512)
        .unwrap_err()
        .to_string();
    Ok(json!({
        "edge": "negative_stat_rejected_by_runner",
        "passed": error.contains("editor.starter_templates.run.stat"),
        "error": error
    }))
}

pub fn unknown_template_id_rejected_edge() -> Result<Value, Box<dyn std::error::Error>> {
    let mut clone = new_project_from_template(
        StarterTemplateKind::MazeEscape,
        "issue858.unknown-template.edge",
        "Issue 858 Unknown Template Edge",
        "issue858-unknown-template-rev-1",
    )?;
    clone.template_id = "starter.unknown.bad".to_string();
    let error = run_starter_template_headless(&clone, 512)
        .unwrap_err()
        .to_string();
    Ok(json!({
        "edge": "unknown_template_id_rejected_by_runner",
        "passed": error.contains("editor.starter_templates.run.template_id"),
        "error": error
    }))
}
