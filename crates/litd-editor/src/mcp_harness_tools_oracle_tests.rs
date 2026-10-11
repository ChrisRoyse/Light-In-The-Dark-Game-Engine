use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_harness_tools::{
        HarnessOracleKind, READ_HARNESS_TOOL, RUN_HEADLESS_TOOL, RUN_ORACLES_TOOL,
        call_mcp_harness_tool, read_mcp_harness_aster_ledger, read_mcp_harness_state,
    },
    mcp_project_tools::{NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, call_mcp_project_tool},
};
use litd_engine::math::{Transform, Vec3};

#[test]
fn issue1173_frame_oracles_persist_clean_and_single_failure_events() {
    let root = test_root("single_failures");
    clean_dir(&root);

    run_clean_oracle_case(&root);
    run_single_failure_case(
        &root,
        "soft-lock",
        ProjectProfile {
            world_props: vec![("oracle.soft_lock_min_bodies_updated", "999")],
            ..ProjectProfile::default()
        },
        8,
        HarnessOracleKind::SoftLockFree,
        "LITD_HARNESS_ORACLE_SOFT_LOCK",
    );
    run_single_failure_case(
        &root,
        "physics",
        ProjectProfile {
            world_props: vec![("oracle.physics_max_abs_milli", "500")],
            ..ProjectProfile::default()
        },
        8,
        HarnessOracleKind::PhysicsBounds,
        "LITD_HARNESS_ORACLE_PHYSICS_BOUNDS",
    );
    run_single_failure_case(
        &root,
        "perf",
        ProjectProfile {
            world_props: vec![("oracle.max_draw_batches", "0")],
            ..ProjectProfile::default()
        },
        8,
        HarnessOracleKind::PerfBudget,
        "LITD_HARNESS_ORACLE_PERF_BUDGET",
    );
    run_single_failure_case(
        &root,
        "economy",
        ProjectProfile {
            unit_props: vec![("gold_reward_milli", "250000")],
            ..ProjectProfile::default()
        },
        8,
        HarnessOracleKind::EconomyRewardSanity,
        "LITD_HARNESS_ORACLE_ECONOMY_REWARD",
    );
    run_single_failure_case(
        &root,
        "soak",
        ProjectProfile {
            world_props: vec![("oracle.soak_min_ticks", "32")],
            ..ProjectProfile::default()
        },
        8,
        HarnessOracleKind::SoakStable,
        "LITD_HARNESS_ORACLE_SOAK_DEGRADATION",
    );
}

#[test]
fn issue1173_frame_oracles_record_simultaneous_multi_oracle_trip() {
    let root = test_root("multi_trip");
    clean_dir(&root);
    let project_path = root.join("multi-trip.litdworld.json");
    let state_path = root.join("harness-state.json");
    let aster_dir = root.join("aster");
    create_project(
        &root,
        &project_path,
        ProjectProfile {
            world_props: vec![
                ("oracle.max_draw_batches", "0"),
                ("oracle.physics_max_abs_milli", "500"),
                ("oracle.soak_min_ticks", "32"),
            ],
            unit_props: vec![("gold_reward_milli", "250000")],
            ..ProjectProfile::default()
        },
    );

    print_state_summary("issue1173 multi before", &state_path, &aster_dir);
    run_headless(
        &root,
        &state_path,
        &aster_dir,
        "multi-run-ledger",
        "issue1173-multi-run",
        &project_path,
        8,
    );
    let oracle = run_oracles(
        &root,
        &state_path,
        &aster_dir,
        "multi-oracle-ledger",
        "issue1173-multi-oracles",
        "issue1173-multi-run",
        all_frame_oracles(),
    );
    let record = oracle
        .state_readback
        .as_ref()
        .unwrap()
        .state
        .oracle_runs
        .last()
        .unwrap();
    assert_eq!(record.failed_count, 4);
    let failed: Vec<_> = record
        .checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| check.oracle)
        .collect();
    assert_eq!(
        failed,
        vec![
            HarnessOracleKind::PhysicsBounds,
            HarnessOracleKind::PerfBudget,
            HarnessOracleKind::EconomyRewardSanity,
            HarnessOracleKind::SoakStable
        ]
    );
    let feedback_codes: Vec<_> = oracle
        .state_readback
        .as_ref()
        .unwrap()
        .state
        .events
        .last()
        .unwrap()
        .feedback
        .iter()
        .map(|record| record.code.as_str())
        .collect();
    assert_eq!(
        feedback_codes,
        vec![
            "LITD_HARNESS_ORACLE_PHYSICS_BOUNDS",
            "LITD_HARNESS_ORACLE_PERF_BUDGET",
            "LITD_HARNESS_ORACLE_ECONOMY_REWARD",
            "LITD_HARNESS_ORACLE_SOAK_DEGRADATION"
        ]
    );
    assert_aster_row(&aster_dir, "issue1173-multi-oracle-ledger", &oracle);
    print_state_summary("issue1173 multi after", &state_path, &aster_dir);
}

fn run_clean_oracle_case(root: &Path) {
    let case_root = root.join("clean");
    fs::create_dir_all(&case_root).unwrap();
    let project_path = case_root.join("clean.litdworld.json");
    let state_path = case_root.join("harness-state.json");
    let aster_dir = case_root.join("aster");
    create_project(
        &case_root,
        &project_path,
        ProjectProfile {
            world_props: vec![("oracle.soak_min_ticks", "8")],
            ..ProjectProfile::default()
        },
    );
    print_state_summary("issue1173 clean before", &state_path, &aster_dir);
    run_headless(
        &case_root,
        &state_path,
        &aster_dir,
        "clean-run-ledger",
        "issue1173-clean-run",
        &project_path,
        16,
    );
    let oracle = run_oracles(
        &case_root,
        &state_path,
        &aster_dir,
        "clean-oracle-ledger",
        "issue1173-clean-oracles",
        "issue1173-clean-run",
        all_frame_oracles(),
    );
    let record = oracle
        .state_readback
        .as_ref()
        .unwrap()
        .state
        .oracle_runs
        .last()
        .unwrap();
    assert_eq!(record.verdict, "passed");
    assert_eq!(record.check_count, 6);
    assert_eq!(record.failed_count, 0);
    assert_aster_row(&aster_dir, "issue1173-clean-oracle-ledger", &oracle);
    let read = call_mcp_harness_tool(READ_HARNESS_TOOL, json!({ "state_path": state_path }))
        .expect("oracle state readback must work");
    assert_eq!(read.state_readback.unwrap().state.oracle_runs.len(), 1);
    print_state_summary("issue1173 clean after", &state_path, &aster_dir);
}

fn run_single_failure_case(
    root: &Path,
    name: &str,
    profile: ProjectProfile,
    ticks: u64,
    expected_oracle: HarnessOracleKind,
    expected_feedback_code: &str,
) {
    let case_root = root.join(name);
    fs::create_dir_all(&case_root).unwrap();
    let project_path = case_root.join(format!("{name}.litdworld.json"));
    let state_path = case_root.join("harness-state.json");
    let aster_dir = case_root.join("aster");
    create_project(&case_root, &project_path, profile);
    print_state_summary(&format!("issue1173 {name} before"), &state_path, &aster_dir);
    run_headless(
        &case_root,
        &state_path,
        &aster_dir,
        &format!("{name}-run-ledger"),
        &format!("issue1173-{name}-run"),
        &project_path,
        ticks,
    );
    let oracle = run_oracles(
        &case_root,
        &state_path,
        &aster_dir,
        &format!("{name}-oracle-ledger"),
        &format!("issue1173-{name}-oracles"),
        &format!("issue1173-{name}-run"),
        all_frame_oracles(),
    );
    let state = oracle.state_readback.as_ref().unwrap();
    let record = state.state.oracle_runs.last().unwrap();
    assert_eq!(record.verdict, "failed");
    assert_eq!(record.failed_count, 1);
    let failed = record.checks.iter().find(|check| !check.passed).unwrap();
    assert_eq!(failed.oracle, expected_oracle);
    let event = state.state.events.last().unwrap();
    assert_eq!(event.feedback.len(), 1);
    assert_eq!(event.feedback[0].code, expected_feedback_code);
    assert_aster_row(
        &aster_dir,
        &format!("issue1173-{name}-oracle-ledger"),
        &oracle,
    );
    print_state_summary(&format!("issue1173 {name} after"), &state_path, &aster_dir);
}

fn run_headless(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    run_id: &str,
    project_path: &Path,
    ticks: u64,
) {
    call_mcp_harness_tool(
        RUN_HEADLESS_TOOL,
        with_context(
            root,
            state_path,
            aster_dir,
            ledger_id,
            json!({
                "run_id": run_id,
                "project_path": project_path,
                "camera_rig": "TopDown",
                "ticks": ticks
            }),
        ),
    )
    .unwrap();
}

fn run_oracles(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    oracle_run_id: &str,
    run_id: &str,
    oracles: Value,
) -> crate::mcp_harness_tools::McpHarnessToolCallReadback {
    call_mcp_harness_tool(
        RUN_ORACLES_TOOL,
        with_context(
            root,
            state_path,
            aster_dir,
            ledger_id,
            json!({
                "oracle_run_id": oracle_run_id,
                "run_id": run_id,
                "oracles": oracles
            }),
        ),
    )
    .unwrap()
}

fn assert_aster_row(
    aster_dir: &Path,
    ledger_id: &str,
    readback: &crate::mcp_harness_tools::McpHarnessToolCallReadback,
) {
    let aster = read_mcp_harness_aster_ledger(aster_dir, ledger_id)
        .unwrap()
        .expect("oracle ledger must exist in Aster");
    assert_eq!(
        aster.sha256,
        readback.ledger_readback.as_ref().unwrap().sha256
    );
    assert!(aster.graph_rows >= 2);
}

fn all_frame_oracles() -> Value {
    json!([
        "crash_free",
        "soft_lock_free",
        "physics_bounds",
        "perf_budget",
        "economy_reward_sanity",
        "soak_stable"
    ])
}

struct ProjectProfile {
    include_unit: bool,
    unit_x: f32,
    world_props: Vec<(&'static str, &'static str)>,
    unit_props: Vec<(&'static str, &'static str)>,
}

impl Default for ProjectProfile {
    fn default() -> Self {
        Self {
            include_unit: true,
            unit_x: 1.0,
            world_props: Vec::new(),
            unit_props: Vec::new(),
        }
    }
}

fn create_project(root: &Path, project_path: &Path, profile: ProjectProfile) {
    let document = fixture_document(profile);
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": "issue1173-project-save",
            "requested_by": "litd-editor-issue1173-test",
            "state_path": root.join("project-state.json"),
            "state_id": "issue1173-project-state",
            "ledger_path": root.join("project-ledger.json"),
            "ledger_id": "issue1173-project-ledger",
            "action": ProjectLifecycleAction::SaveSnapshot,
            "project_id": "project.harness_issue1173",
            "title": "Harness Issue 1173",
            "version": "1.0.0",
            "tags": ["agent_harness", "frame_oracles"],
            "project_path": project_path,
            "snapshot_label": "harness-source",
            "document": document
        }),
    )
    .unwrap();
}

fn fixture_document(profile: ProjectProfile) -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1173-frame-oracles", "issue1173-rev-1").unwrap();
    let mut world = EditorObject::new("world", "World", None, Transform::IDENTITY);
    for (key, value) in profile.world_props {
        world = world.with_property(key, value);
    }
    document.upsert_object(world).unwrap();
    if profile.include_unit {
        let mut unit = EditorObject::new(
            "unit_playtest_hero",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(profile.unit_x, 0.0, -2.0)),
        )
        .with_property("faction", "Dawnward");
        for (key, value) in profile.unit_props {
            unit = unit.with_property(key, value);
        }
        document.upsert_object(unit).unwrap();
        let record = AssetVaultCatalog::load_default_cached()
            .unwrap()
            .records()
            .iter()
            .find(|record| {
                record.relative_path
                    == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
            })
            .cloned()
            .expect("real vault must contain Quaternius Rock1");
        assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record).unwrap();
    }
    document
}

fn state_summary(state_path: &Path, aster_dir: &Path) -> Value {
    let state = read_mcp_harness_state(state_path).ok();
    json!({
        "state_exists": state_path.exists(),
        "state_sha": state.as_ref().map(|read| read.sha256.clone()),
        "run_count": state.as_ref().map(|read| read.run_count).unwrap_or_default(),
        "oracle_run_count": state.as_ref().map(|read| read.oracle_run_count).unwrap_or_default(),
        "event_count": state.as_ref().map(|read| read.event_count).unwrap_or_default(),
        "aster_exists": aster_dir.exists()
    })
}

fn print_state_summary(label: &str, state_path: &Path, aster_dir: &Path) {
    println!(
        "{label} {}",
        serde_json::to_string(&state_summary(state_path, aster_dir)).unwrap()
    );
}

fn with_context(
    root: &Path,
    state_path: &Path,
    aster_dir: &Path,
    ledger_id: &str,
    mut value: Value,
) -> Value {
    let object = value.as_object_mut().unwrap();
    object.insert("request_id".to_string(), json!(ledger_id));
    object.insert(
        "requested_by".to_string(),
        json!("litd-editor-issue1173-test"),
    );
    object.insert("state_path".to_string(), json!(state_path));
    object.insert("state_id".to_string(), json!("issue1173-harness-state"));
    object.insert(
        "ledger_path".to_string(),
        json!(root.join(format!("{ledger_id}.json"))),
    );
    object.insert(
        "ledger_id".to_string(),
        json!(format!("issue1173-{ledger_id}")),
    );
    object.insert("aster_dir".to_string(), json!(aster_dir));
    value
}

fn test_root(name: &str) -> PathBuf {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    crate::test_scratch::issue_scratch_dir("issue1173_frame_oracles", name)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
