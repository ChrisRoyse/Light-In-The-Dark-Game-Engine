use std::{collections::BTreeMap, fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

use super::*;
use crate::{
    EngineError,
    fsv_artifacts::{begin_issue_run, issue_root},
};

fn issue1322_behavior_tree_source_truth_fsv() {
    let run = begin_issue_run("issue1322_behavior_tree").unwrap();
    let run_path = run.run_dir().join("patrol-chase-attack-behavior-run-aster");
    let before = file_state(&run_path);

    let tree = patrol_chase_attack_tree();
    let tree_fnv1a64 = compute_behavior_tree_fnv1a64(&tree);
    let ticks = vec![
        tick_behavior_tree(&tree, &patrol_state(), 0).unwrap(),
        tick_behavior_tree(&tree, &chase_state(), 1).unwrap(),
        tick_behavior_tree(&tree, &attack_state(), 2).unwrap(),
    ];
    let repeat_ticks = vec![
        tick_behavior_tree(&tree, &patrol_state(), 0).unwrap(),
        tick_behavior_tree(&tree, &chase_state(), 1).unwrap(),
        tick_behavior_tree(&tree, &attack_state(), 2).unwrap(),
    ];
    assert_eq!(ticks, repeat_ticks);
    assert_eq!(
        status_trace(&ticks),
        vec![
            "0:root=Running",
            "0:attack_sequence=Failure",
            "0:in_attack_range=Failure",
            "0:chase_sequence=Failure",
            "0:sees_enemy=Failure",
            "0:patrol=Running",
            "1:root=Running",
            "1:attack_sequence=Failure",
            "1:in_attack_range=Failure",
            "1:chase_sequence=Running",
            "1:sees_enemy=Success",
            "1:chase=Running",
            "2:root=Success",
            "2:attack_sequence=Success",
            "2:in_attack_range=Success",
            "2:attack=Success",
        ]
    );

    let run_report = behavior_run(tree_fnv1a64.clone(), ticks.clone());
    let readback = write_behavior_run_json(&run_path, &run_report).unwrap();
    let disk_run = load_behavior_run_json(&run_path).unwrap().run;
    assert_eq!(disk_run, run_report);
    assert_eq!(
        readback.sha256,
        sha256_hex(&serde_json::to_vec_pretty(&run_report).unwrap())
    );
    assert_eq!(readback.run.ticks[0].root_status, BehaviorStatus::Running);
    assert_eq!(readback.run.ticks[2].root_status, BehaviorStatus::Success);

    let max_tree = max_tree_edge(run.run_dir());
    let empty_tree = failing_edge(
        "empty_tree",
        empty_tree(),
        leaf_state(vec![], vec![]),
        run.run_dir(),
    );
    let duplicate = failing_edge(
        "duplicate_id",
        duplicate_node_tree(),
        leaf_state(vec![("ready", BehaviorStatus::Success)], vec![]),
        run.run_dir(),
    );
    let missing_leaf = failing_edge(
        "missing_leaf",
        patrol_chase_attack_tree(),
        leaf_state(vec![], vec![]),
        run.run_dir(),
    );
    let cycle = failing_edge(
        "cycle",
        cycle_tree(),
        leaf_state(vec![("ready", BehaviorStatus::Success)], vec![]),
        run.run_dir(),
    );
    let running_condition = failing_edge(
        "running_condition",
        condition_tree(),
        leaf_state(vec![("ready", BehaviorStatus::Running)], vec![]),
        run.run_dir(),
    );

    let after = file_state(&run_path);
    let report = json!({
        "schema_version": 1,
        "issue": 1322,
        "source_of_truth": {
            "behavior_run_path": run_path,
            "behavior_run_sha256": readback.sha256,
            "behavior_run_fnv1a64": readback.fnv1a64,
            "tree_fnv1a64": tree_fnv1a64,
            "latest_manifest_path": run.latest_manifest_path(),
        },
        "happy_path": {
            "before": before,
            "after": after,
            "expected": {
                "root_statuses": ["Running", "Running", "Success"],
                "trace": status_trace(&ticks),
                "repeat_equal": true,
            },
            "actual": {
                "root_statuses": root_statuses(&disk_run),
                "trace": status_trace(&disk_run.ticks),
                "tick_hashes": tick_hashes(&disk_run),
                "run_fnv1a64": disk_run.fnv1a64,
                "repeat_equal": ticks == repeat_ticks,
            }
        },
        "edges": {
            "max_tree": max_tree,
            "empty_tree": empty_tree,
            "duplicate_id": duplicate,
            "missing_leaf": missing_leaf,
            "cycle": cycle,
            "running_condition": running_condition,
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(
        report_readback.json["happy_path"]["actual"]["root_statuses"],
        json!(["Running", "Running", "Success"])
    );
    assert_eq!(
        report_readback.json["edges"]["duplicate_id"]["error"]["code"],
        "LITD-AI-INVALID-CONFIG"
    );
    println!(
        "ISSUE1322_BT_FSV_REPORT path={} bytes={} sha256={} run_json={} run_sha256={} run_fnv1a64={} tree_fnv1a64={} roots={:?} max_trace={} empty_code={} duplicate_code={} missing_code={} cycle_code={} running_condition_code={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        readback.path.display(),
        readback.sha256,
        readback.fnv1a64,
        tree_fnv1a64,
        root_statuses(&disk_run),
        report_readback.json["edges"]["max_tree"]["after"]["trace_len"],
        report_readback.json["edges"]["empty_tree"]["error"]["code"],
        report_readback.json["edges"]["duplicate_id"]["error"]["code"],
        report_readback.json["edges"]["missing_leaf"]["error"]["code"],
        report_readback.json["edges"]["cycle"]["error"]["code"],
        report_readback.json["edges"]["running_condition"]["error"]["code"],
    );
}

fn max_tree_edge(root: &Path) -> Value {
    let path = root.join("edge-max-tree-behavior-run-aster");
    let before = file_state(&path);
    let tree = max_condition_tree(64);
    let mut conditions = Vec::new();
    for index in 0..64 {
        conditions.push((format!("ready_{index:02}"), BehaviorStatus::Success));
    }
    let state = leaf_state_owned(conditions, Vec::new());
    let tick = tick_behavior_tree(&tree, &state, 99).unwrap();
    let run = behavior_run(compute_behavior_tree_fnv1a64(&tree), vec![tick]);
    let readback = write_behavior_run_json(&path, &run).unwrap();
    let disk = load_behavior_run_json(&path).unwrap().run;
    assert_eq!(disk.ticks[0].trace.len(), 65);
    assert_eq!(disk.ticks[0].root_status, BehaviorStatus::Success);
    json!({
        "before": before,
        "after": {
            "exists": path.exists(),
            "bytes": readback.bytes,
            "sha256": readback.sha256,
            "fnv1a64": readback.fnv1a64,
            "trace_len": disk.ticks[0].trace.len(),
            "root_status": format!("{:?}", disk.ticks[0].root_status),
        },
        "expected": {"trace_len": 65, "root_status": "Success"}
    })
}

fn failing_edge(label: &str, tree: BehaviorTree, state: BehaviorLeafState, root: &Path) -> Value {
    let path = root.join(format!("edge-{label}-behavior-run-aster"));
    let before = file_state(&path);
    let error = attempt_tick_and_write(&tree, &state, &path).unwrap_err();
    let after = file_state(&path);
    assert!(!path.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

fn attempt_tick_and_write(
    tree: &BehaviorTree,
    state: &BehaviorLeafState,
    path: &Path,
) -> Result<(), EngineError> {
    let tick = tick_behavior_tree(tree, state, 7)?;
    let run = behavior_run(compute_behavior_tree_fnv1a64(tree), vec![tick]);
    write_behavior_run_json(path, &run).map(|_| ())
}

fn behavior_run(tree_fnv1a64: String, ticks: Vec<BehaviorTickReport>) -> BehaviorRunReport {
    let mut run = BehaviorRunReport {
        schema: BEHAVIOR_TREE_RUN_SCHEMA.to_string(),
        tree_fnv1a64,
        ticks,
        fnv1a64: String::new(),
    };
    run.fnv1a64 = compute_behavior_run_fnv1a64(&run);
    run
}

fn patrol_chase_attack_tree() -> BehaviorTree {
    BehaviorTree {
        schema: BEHAVIOR_TREE_SCHEMA.to_string(),
        root: "root".to_string(),
        nodes: vec![
            selector("root", &["attack_sequence", "chase_sequence", "patrol"]),
            sequence("attack_sequence", &["in_attack_range", "attack"]),
            condition("in_attack_range", "in_attack_range"),
            action("attack", "attack"),
            sequence("chase_sequence", &["sees_enemy", "chase"]),
            condition("sees_enemy", "sees_enemy"),
            action("chase", "chase"),
            action("patrol", "patrol"),
        ],
    }
}

fn empty_tree() -> BehaviorTree {
    BehaviorTree {
        schema: BEHAVIOR_TREE_SCHEMA.to_string(),
        root: "root".to_string(),
        nodes: Vec::new(),
    }
}

fn duplicate_node_tree() -> BehaviorTree {
    BehaviorTree {
        schema: BEHAVIOR_TREE_SCHEMA.to_string(),
        root: "root".to_string(),
        nodes: vec![condition("root", "ready"), condition("root", "ready")],
    }
}

fn cycle_tree() -> BehaviorTree {
    BehaviorTree {
        schema: BEHAVIOR_TREE_SCHEMA.to_string(),
        root: "root".to_string(),
        nodes: vec![sequence("root", &["child"]), sequence("child", &["root"])],
    }
}

fn condition_tree() -> BehaviorTree {
    BehaviorTree {
        schema: BEHAVIOR_TREE_SCHEMA.to_string(),
        root: "root".to_string(),
        nodes: vec![condition("root", "ready")],
    }
}

fn max_condition_tree(count: usize) -> BehaviorTree {
    let mut nodes = vec![sequence(
        "root",
        &(0..count)
            .map(|index| format!("ready_{index:02}"))
            .collect::<Vec<_>>(),
    )];
    for index in 0..count {
        let id = format!("ready_{index:02}");
        nodes.push(condition(&id, &id));
    }
    BehaviorTree {
        schema: BEHAVIOR_TREE_SCHEMA.to_string(),
        root: "root".to_string(),
        nodes,
    }
}

fn patrol_state() -> BehaviorLeafState {
    leaf_state(
        vec![
            ("in_attack_range", BehaviorStatus::Failure),
            ("sees_enemy", BehaviorStatus::Failure),
        ],
        vec![("patrol", BehaviorStatus::Running)],
    )
}

fn chase_state() -> BehaviorLeafState {
    leaf_state(
        vec![
            ("in_attack_range", BehaviorStatus::Failure),
            ("sees_enemy", BehaviorStatus::Success),
        ],
        vec![("chase", BehaviorStatus::Running)],
    )
}

fn attack_state() -> BehaviorLeafState {
    leaf_state(
        vec![("in_attack_range", BehaviorStatus::Success)],
        vec![("attack", BehaviorStatus::Success)],
    )
}

fn sequence(id: &str, children: &[impl AsRef<str>]) -> BehaviorNode {
    node(id, BehaviorNodeKind::Sequence, children, None)
}

fn selector(id: &str, children: &[impl AsRef<str>]) -> BehaviorNode {
    node(id, BehaviorNodeKind::Selector, children, None)
}

fn condition(id: &str, binding: &str) -> BehaviorNode {
    node(
        id,
        BehaviorNodeKind::Condition,
        &[] as &[&str],
        Some(binding),
    )
}

fn action(id: &str, binding: &str) -> BehaviorNode {
    node(id, BehaviorNodeKind::Action, &[] as &[&str], Some(binding))
}

fn node(
    id: &str,
    kind: BehaviorNodeKind,
    children: &[impl AsRef<str>],
    binding: Option<&str>,
) -> BehaviorNode {
    BehaviorNode {
        id: id.to_string(),
        kind,
        children: children
            .iter()
            .map(|child| child.as_ref().to_string())
            .collect(),
        binding: binding.map(ToString::to_string),
    }
}

fn leaf_state(
    conditions: Vec<(&str, BehaviorStatus)>,
    actions: Vec<(&str, BehaviorStatus)>,
) -> BehaviorLeafState {
    leaf_state_owned(
        conditions
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
        actions
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn leaf_state_owned(
    conditions: Vec<(String, BehaviorStatus)>,
    actions: Vec<(String, BehaviorStatus)>,
) -> BehaviorLeafState {
    BehaviorLeafState {
        conditions: BTreeMap::from_iter(conditions),
        actions: BTreeMap::from_iter(actions),
    }
}

fn status_trace(ticks: &[BehaviorTickReport]) -> Vec<String> {
    ticks
        .iter()
        .flat_map(|tick| {
            tick.trace
                .iter()
                .map(|entry| format!("{}:{}={:?}", tick.tick, entry.node_id, entry.status))
        })
        .collect()
}

fn root_statuses(run: &BehaviorRunReport) -> Vec<String> {
    run.ticks
        .iter()
        .map(|tick| format!("{:?}", tick.root_status))
        .collect()
}

fn tick_hashes(run: &BehaviorRunReport) -> Vec<String> {
    run.ticks.iter().map(|tick| tick.fnv1a64.clone()).collect()
}

fn file_state(path: &Path) -> Value {
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "bytes": fs::metadata(path).map(|meta| meta.len()).ok(),
    })
}

fn error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
    })
}

#[test]
fn issue1322_latest_fsv_manifest_points_at_real_report() {
    issue1322_behavior_tree_source_truth_fsv();
    let root = issue_root("issue1322_behavior_tree");
    let manifest_path = root.join("latest-manifest.json");
    let bytes = fs::read(&manifest_path).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let report_path = Path::new(manifest["report_path"].as_str().unwrap());
    let report_bytes = fs::read(report_path).unwrap();
    assert_eq!(sha256_hex(&report_bytes), manifest["report_sha256"]);
}
