use std::path::PathBuf;

use litd_editor::{
    fsv_support::artifact_dir_from_env,
    mcp_harness_tools::{
        HarnessRunRecord, HarnessSnapshotBranchRecord, READ_HARNESS_TOOL, RUN_HEADLESS_TOOL,
        SNAPSHOT_BRANCH_TOOL, call_mcp_harness_tool, read_mcp_harness_state,
    },
    mcp_project_tools::{
        NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, READ_PROJECT_TOOL, RUN_HEADLESS_PLAYTEST_TOOL,
        call_mcp_project_tool, inspect_litdworld_archive, read_litdworld_project_readback,
        read_mcp_project_state,
    },
    play::PlayCameraRig,
    tower_defense_fsv::{
        assert_issue743_windows_reruns_identical, build_issue743_leak_all_towers_sold_project,
        build_issue743_tower_defense_project, run_issue743_leak_loss,
        run_issue743_tower_defense_victory,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (run, root) = artifact_dir_from_env(
        "LITD_ISSUE743_FSV_ROOT",
        "issue743_tower_defense_full_editor",
        "manual",
    )?;
    let paths = Paths::new(root);
    let build = build_issue743_tower_defense_project(
        "issue743.tower_defense.fsv",
        "Issue 743 No-Code Tower Defense",
        "issue743-fsv-rev-1",
    )?;

    let trigger_readback = run.write_json_artifact(
        "manual/issue743-trigger-project.json",
        &serde_json::to_value(&build.clone.trigger_project)?,
    )?;
    let save = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue743-save-litdworld",
            "requested_by": "litd-editor-issue743-fsv",
            "state_path": paths.project_state,
            "state_id": "issue743-project-state",
            "ledger_path": paths.project_ledger,
            "ledger_id": "issue743-project-ledger",
            "action": ProjectLifecycleAction::New,
            "project_id": build.clone.project.project_id,
            "title": build.clone.project.title,
            "version": build.clone.project.version,
            "tags": build.clone.project.tags,
            "project_path": paths.project,
            "snapshot_label": build.clone.project.snapshot_label,
            "document": build.clone.project.document
        }),
    )?;
    let archive = inspect_litdworld_archive(&paths.project)?;
    let project_readback = read_litdworld_project_readback(&paths.project)?;
    let project_bytes = std::fs::read(&paths.project)?;
    let archive_verified = archive.zip_magic == "PK0304"
        && archive.zip_entry_count == 2
        && archive.project.document_fingerprint == build.acceptance.document_fingerprint
        && project_readback.sha256 == sha256_hex(&project_bytes);

    let playtest = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": "issue743-project-playtest",
            "requested_by": "litd-editor-issue743-fsv",
            "state_path": paths.project_state,
            "state_id": "issue743-project-state",
            "ledger_path": paths.playtest_ledger,
            "ledger_id": "issue743-playtest-ledger",
            "project_path": paths.project,
            "camera_rig": PlayCameraRig::TopDown,
            "ticks": 64
        }),
    )?;

    let mut exported_clone = build.clone.clone();
    exported_clone.project = archive.project.clone();
    let victory = run_issue743_tower_defense_victory(&exported_clone, 512)?;
    let reruns = assert_issue743_windows_reruns_identical(&exported_clone, 512, 2)?;
    let loss_clone = build_issue743_leak_all_towers_sold_project(&build)?;
    let loss = run_issue743_leak_loss(&loss_clone, 512)?;
    let save_load = verify_save_load_resume(&paths)?;

    let read_project = call_mcp_project_tool(
        READ_PROJECT_TOOL,
        json!({"state_path": paths.project_state}),
    )?;
    let project_state = read_mcp_project_state(&paths.project_state)?;
    let harness_read = call_mcp_harness_tool(
        READ_HARNESS_TOOL,
        json!({"state_path": paths.harness_state}),
    )?;
    let harness_state = read_mcp_harness_state(&paths.harness_state)?;
    let project_state_persisted = project_state.project_count == 1
        && project_state.playtest_count == 1
        && project_state.state.projects[0].document_fingerprint
            == archive.project.document_fingerprint;
    let playtest_record = &project_state.state.playtests[0];
    let headless_project_passed = playtest_record.verdict == "passed"
        && playtest_record.spawned_bodies > 0
        && playtest_record.visible_instance_count > 0
        && playtest_record.draw_batches > 0
        && playtest_record.initial_state_hash != playtest_record.final_state_hash;
    let windows_reruns_identical =
        reruns[0].final_state_hash == reruns[1].final_state_hash && reruns[0].deterministic;
    let accepted = archive_verified
        && project_state_persisted
        && headless_project_passed
        && victory.deterministic
        && windows_reruns_identical
        && loss.deterministic
        && loss.base_health <= 0
        && save_load["passed"] == true;
    if !accepted {
        return Err("issue743 FSV acceptance predicates failed".into());
    }

    let report = json!({
        "schema": "litd.issue743.tower_defense_full_editor_fsv.v1",
        "issue": 743,
        "source_of_truth": "exported .litdworld ZIP archive readback, persisted trigger-editor graph artifact, Calyx Aster project/harness states, starter-template match results, and deterministic state hashes",
        "readme_references_inspected": [
            "docs/readme/ED03-object-editor.png",
            "docs/readme/ED04-trigger-editor.png",
            "docs/readme/ED09-test-in-editor.png",
            "docs/readme/ED11-world-view.png",
            "docs/readme/ED12-play-topdown.png"
        ],
        "paths": paths.to_json(),
        "acceptance": {
            "archive_verified": archive_verified,
            "project_state_persisted": project_state_persisted,
            "headless_project_passed": headless_project_passed,
            "td_match_victory": victory.deterministic,
            "windows_reruns_identical": windows_reruns_identical,
            "leak_all_towers_sold_loss": loss.deterministic && loss.base_health <= 0,
            "save_load_mid_match_resume": save_load["passed"],
            "second_os_parity": "n/a_windows_only"
        },
        "build": {
            "operations": build.operations,
            "acceptance": build.acceptance,
            "trigger_project_artifact": {
                "path": trigger_readback.path,
                "bytes": trigger_readback.bytes.len(),
                "sha256": trigger_readback.sha256,
                "latest_manifest_path": trigger_readback.latest_manifest_path
            }
        },
        "archive": {
            "path": archive.path,
            "bytes": archive.bytes,
            "sha256": archive.sha256,
            "zip_magic": archive.zip_magic,
            "zip_entry_count": archive.zip_entry_count,
            "zip_entries": archive.zip_entries,
            "document_fingerprint": archive.project.document_fingerprint,
            "manifest": archive.manifest,
            "project_file_sha256": sha256_hex(&project_bytes),
            "project_readback": project_readback
        },
        "project_mcp": {
            "save": save.result,
            "playtest": playtest.result,
            "read_project": read_project.result,
            "state_sha256": project_state.sha256,
            "project_count": project_state.project_count,
            "playtest_count": project_state.playtest_count
        },
        "match": {
            "victory": victory,
            "windows_reruns": reruns,
            "leak_all_towers_sold": loss
        },
        "save_load_resume": save_load,
        "harness": {
            "read": harness_read.result,
            "state_sha256": harness_state.sha256,
            "run_count": harness_state.run_count,
            "snapshot_count": harness_state.state.snapshots.len()
        }
    });
    let readback = run.write_json_artifact("manual/issue743-tower-defense-fsv.json", &report)?;
    println!(
        "ISSUE743_TOWER_DEFENSE_FSV report={} report_sha256={} archive_sha256={} victory_hash={} loss_hash={} resume_hash={} accepted={}",
        readback.path.display(),
        readback.sha256,
        archive.sha256,
        victory.final_state_hash,
        loss.final_state_hash,
        save_load["branch_final_state_hash"],
        accepted
    );
    Ok(())
}

fn verify_save_load_resume(paths: &Paths) -> Result<Value, Box<dyn std::error::Error>> {
    let direct = call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        harness_context(
            paths,
            "direct-32-ledger",
            json!({
                "run_id": "issue743-direct-32",
                "project_path": paths.project,
                "camera_rig": "TopDown",
                "ticks": 32
            }),
        ),
    )?;
    let source = call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        harness_context(
            paths,
            "source-16-ledger",
            json!({
                "run_id": "issue743-source-16",
                "project_path": paths.project,
                "camera_rig": "TopDown",
                "ticks": 16
            }),
        ),
    )?;
    let branch = call_mcp_harness_tool(
        SNAPSHOT_BRANCH_TOOL,
        harness_context(
            paths,
            "branch-16-ledger",
            json!({
                "run_id": "issue743-source-16",
                "snapshot_id": "issue743-midmatch-snapshot",
                "branch_id": "issue743-midmatch-branch",
                "branch_ticks": 16
            }),
        ),
    )?;
    let direct_run: HarnessRunRecord = serde_json::from_value(direct.result["run"].clone())?;
    let source_run: HarnessRunRecord = serde_json::from_value(source.result["run"].clone())?;
    let branch_run: HarnessSnapshotBranchRecord =
        serde_json::from_value(branch.result["snapshot_branch"].clone())?;
    let passed = source_run.final_tick == 16
        && branch_run.restored_tick == 16
        && branch_run.branch_final_tick == direct_run.final_tick
        && branch_run.branch_final_state_hash == direct_run.final_state_hash;
    Ok(json!({
        "passed": passed,
        "continuous_run_id": direct_run.run_id,
        "source_run_id": source_run.run_id,
        "snapshot_id": branch_run.snapshot_id,
        "branch_id": branch_run.branch_id,
        "restored_tick": branch_run.restored_tick,
        "branch_final_tick": branch_run.branch_final_tick,
        "continuous_final_state_hash": direct_run.final_state_hash,
        "branch_final_state_hash": branch_run.branch_final_state_hash,
        "direct": direct.result,
        "source": source.result,
        "branch": branch.result
    }))
}

fn harness_context(paths: &Paths, ledger_id: &str, mut value: Value) -> Value {
    let object = value.as_object_mut().expect("harness value must be object");
    object.insert("request_id".to_string(), json!(ledger_id));
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue743-fsv"),
    );
    object.insert("state_path".to_string(), json!(paths.harness_state));
    object.insert("state_id".to_string(), json!("issue743-harness-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(paths.root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue743-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(paths.harness_aster));
    value
}

#[derive(Debug, Clone)]
struct Paths {
    root: PathBuf,
    project_state: PathBuf,
    project_ledger: PathBuf,
    playtest_ledger: PathBuf,
    project: PathBuf,
    harness_state: PathBuf,
    harness_aster: PathBuf,
}

impl Paths {
    fn new(root: PathBuf) -> Self {
        Self {
            project_state: root.join("project-state.json"),
            project_ledger: root.join("project-ledger.json"),
            playtest_ledger: root.join("playtest-ledger.json"),
            project: root.join("issue743-no-code-tower-defense.litdworld"),
            harness_state: root.join("harness-state.json"),
            harness_aster: root.join("harness-aster"),
            root,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "root": self.root,
            "project_state": self.project_state,
            "project_ledger": self.project_ledger,
            "playtest_ledger": self.playtest_ledger,
            "project": self.project,
            "harness_state": self.harness_state,
            "harness_aster": self.harness_aster
        })
    }
}
