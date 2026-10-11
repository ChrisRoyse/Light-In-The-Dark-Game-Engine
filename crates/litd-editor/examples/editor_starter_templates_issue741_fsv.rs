use std::{fs, path::PathBuf};

use litd_editor::{
    history::stable_fingerprint,
    mcp_project_tools::{
        NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, READ_PROJECT_TOOL, RUN_HEADLESS_PLAYTEST_TOOL,
        call_mcp_project_tool, read_litdworld_project_readback,
    },
    play::PlayCameraRig,
    starter_templates::{
        StarterTemplateKind, StarterTerminalResult, new_project_from_template,
        run_starter_template_headless, starter_template, validate_starter_template,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE741_FSV_ROOT",
        "issue741_starter_templates",
        "manual",
    )?;
    let paths = Paths::new(root);
    let mut template_reports = Vec::new();
    for kind in [
        StarterTemplateKind::TowerDefense,
        StarterTemplateKind::MeleeSkirmish,
        StarterTemplateKind::RpgArena,
    ] {
        let report = verify_template(&paths, kind)?;
        template_reports.push(report);
    }
    let clone_edge = clone_independence_edge()?;
    let state_readback =
        call_mcp_project_tool(READ_PROJECT_TOOL, json!({"state_path": paths.state}))?;
    let all_terminal = template_reports
        .iter()
        .all(|report| report["starter_run"]["terminal_result"] == "win");
    let all_deterministic = template_reports
        .iter()
        .all(|report| report["starter_run"]["deterministic"] == true);
    let all_supported = template_reports
        .iter()
        .all(|report| report["validation"]["supported_data_only"] == true);
    if !(all_terminal && all_deterministic && all_supported && clone_edge["passed"] == true) {
        return Err("issue741 FSV acceptance predicates failed".into());
    }
    let report = json!({
        "schema": "litd.issue741.starter_templates_fsv.v1",
        "issue": 741,
        "source_of_truth": "physical .litdworld ZIP archives, re-imported project readbacks, Calyx Aster project state, existing headless playtest records, and starter-template terminal simulation reports",
        "readme_references_inspected": [
            "docs/readme/ED01-editor-shell.png",
            "docs/readme/ED04-trigger-editor.png",
            "docs/readme/ED09-test-in-editor.png",
            "docs/readme/ED11-world-view.png",
            "docs/readme/ED12-play-topdown.png"
        ],
        "paths": paths.to_json(),
        "acceptance": {
            "all_templates_supported_data_only": all_supported,
            "all_templates_terminal": all_terminal,
            "all_templates_deterministic": all_deterministic,
            "clone_independence": clone_edge["passed"]
        },
        "templates": template_reports,
        "edges": [clone_edge],
        "project_state": {
            "path": state_readback.state_readback.path,
            "sha256": state_readback.state_readback.sha256,
            "project_count": state_readback.state_readback.project_count,
            "playtest_count": state_readback.state_readback.playtest_count,
            "package_count": state_readback.state_readback.package_count
        }
    });
    let readback =
        run.write_json_artifact("manual/issue741-starter-templates-fsv.json", &report)?;
    println!(
        "ISSUE741_STARTER_TEMPLATES_FSV report={} report_sha256={} templates={} terminal={} deterministic={} supported_data_only={}",
        readback.path.display(),
        readback.sha256,
        template_reports.len(),
        all_terminal,
        all_deterministic,
        all_supported
    );
    Ok(())
}

fn verify_template(
    paths: &Paths,
    kind: StarterTemplateKind,
) -> Result<Value, Box<dyn std::error::Error>> {
    let safe = safe_kind(kind);
    let project_id = format!("issue741.{safe}");
    let title = format!("Issue 741 {}", kind.default_title());
    let template = starter_template(kind)?;
    let validation = validate_starter_template(&template)?;
    let clone = new_project_from_template(kind, &project_id, &title, "issue741-template-rev-1")?;
    let project_path = paths.project_path(&safe);
    let save_readback = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": format!("issue741-save-{safe}"),
            "requested_by": "litd-editor-issue741-fsv",
            "state_path": paths.state,
            "state_id": "issue741-starter-template-state",
            "ledger_path": paths.root.join(format!("{safe}-save-ledger")),
            "ledger_id": format!("issue741-{safe}-save-ledger"),
            "action": ProjectLifecycleAction::New,
            "project_id": project_id,
            "title": title,
            "version": "0.1.0",
            "tags": ["starter-template", safe.as_str()],
            "project_path": project_path,
            "snapshot_label": format!("{safe}-new-from-template"),
            "document": clone.project.document
        }),
    )?;
    let archive = read_litdworld_project_readback(&project_path)?;
    let project_bytes = fs::read(&project_path)?;
    let playtest_readback = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": format!("issue741-play-{safe}"),
            "requested_by": "litd-editor-issue741-fsv",
            "state_path": paths.state,
            "state_id": "issue741-starter-template-state",
            "ledger_path": paths.root.join(format!("{safe}-play-ledger")),
            "ledger_id": format!("issue741-{safe}-play-ledger"),
            "project_path": project_path,
            "camera_rig": PlayCameraRig::TopDown,
            "ticks": 64
        }),
    )?;
    let starter_run = run_starter_template_headless(&clone, 512)?;
    if starter_run.terminal_result != StarterTerminalResult::Win || !starter_run.deterministic {
        return Err(format!("starter run failed acceptance for {safe}").into());
    }
    if archive.project.document_fingerprint != stable_fingerprint(&clone.project.document)? {
        return Err(format!("archive document fingerprint mismatch for {safe}").into());
    }
    Ok(json!({
        "kind": format!("{kind:?}"),
        "template_id": template.template_id,
        "validation": {
            "supported_data_only": validation.supported_data_only,
            "no_wasm_escape": validation.no_wasm_escape,
            "object_count": validation.object_count,
            "unit_count": validation.unit_count,
            "region_count": validation.region_count,
            "graph_count": validation.graph_count,
            "event_count": validation.event_count,
            "condition_count": validation.condition_count,
            "action_count": validation.action_count,
            "tooltip_count": validation.tooltip_count,
            "document_fingerprint": validation.document_fingerprint,
            "trigger_fingerprint": validation.trigger_fingerprint,
            "scenario_fingerprint": validation.scenario_fingerprint,
            "world_validation_errors": validation.world_validation.error_count
        },
        "clone": {
            "project_id": clone.project.project_id,
            "template_fingerprint": clone.template_fingerprint,
            "clone_fingerprint": clone.clone_fingerprint,
            "clone_independent": clone.clone_independent,
            "tooltip_count": clone.guided_first_edit_tooltips.len()
        },
        "archive": {
            "path": archive.path,
            "bytes": archive.bytes,
            "sha256": archive.sha256,
            "project_id": archive.project.project_id,
            "document_fingerprint": archive.project.document_fingerprint,
            "zip_entry_count": archive.graph_rows,
            "file_sha256": sha256_hex(&project_bytes),
            "file_bytes": project_bytes.len()
        },
        "mcp_state": {
            "save_state_sha256": save_readback.state_readback.sha256,
            "save_project_count": save_readback.state_readback.project_count,
            "playtest_state_sha256": playtest_readback.state_readback.sha256,
            "playtest_count": playtest_readback.state_readback.playtest_count
        },
        "starter_run": starter_run
    }))
}

fn clone_independence_edge() -> Result<Value, Box<dyn std::error::Error>> {
    let template = starter_template(StarterTemplateKind::TowerDefense)?;
    let template_before = stable_fingerprint(&template.document)?;
    let mut clone = new_project_from_template(
        StarterTemplateKind::TowerDefense,
        "issue741.clone.edge",
        "Issue 741 Clone Edge",
        "issue741-clone-edge-rev-1",
    )?;
    let clone_before = stable_fingerprint(&clone.project.document)?;
    clone
        .project
        .document
        .objects
        .get_mut("td_tower_alpha")
        .ok_or("missing td_tower_alpha")?
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
        "mutated_property": "td_tower_alpha.stat.damage"
    }))
}

#[derive(Debug, Clone)]
struct Paths {
    root: PathBuf,
    state: PathBuf,
    projects: PathBuf,
}

impl Paths {
    fn new(root: PathBuf) -> Self {
        Self {
            state: root.join("project-state"),
            projects: root.join("projects"),
            root,
        }
    }

    fn project_path(&self, safe: &str) -> PathBuf {
        self.projects.join(format!("{safe}.litdworld"))
    }

    fn to_json(&self) -> Value {
        json!({
            "root": self.root,
            "state": self.state,
            "projects": self.projects
        })
    }
}

fn safe_kind(kind: StarterTemplateKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}
