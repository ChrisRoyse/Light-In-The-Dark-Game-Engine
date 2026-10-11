use calyx_core::{Anchor, AnchorKind, AnchorValue};
use serde_json::{Value, json};

use crate::{
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_lowering::{
        ENGINE_LOWERING_HOT_PATH_POLICY, ENGINE_LOWERING_SIM_ACCESS_MODE, EngineLoweringError,
        EngineLoweringValve,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    determinism_boundary::{HotPathPhase, enter_hot_path},
    math::{Fixed, FixedVec3},
    sim::{FixedTickSim, SimConfig},
    sim_order::SimOrderSourceKind,
};

#[test]
fn lowering_valve_freezes_calyx_content_for_sim_consumers() {
    let (store, vault_dir) = content_store("freeze");
    let record = content(
        "unit-warden",
        json!({
            "unit_id": "unit-warden",
            "hit_points_fp": 120 * Fixed::ONE_RAW,
            "move_speed_fp": Fixed::ONE_RAW * 3,
            "spawn": {
                "x_fp": 0,
                "y_fp": 0,
                "z_fp": -2 * Fixed::ONE_RAW
            },
            "can_attack": true,
            "tags": ["frontline", "guard"]
        }),
    );
    let readback = store.put_content(&record).unwrap();

    let package = EngineLoweringValve::lower_content_for_sim(&readback).unwrap();
    let verified = EngineLoweringValve::verify_lowered_package(&package).unwrap();

    assert_eq!(package.source_cx_id, readback.cx_id);
    assert_eq!(package.source_fingerprint, readback.input_sha256);
    assert_eq!(package.hot_path_policy, ENGINE_LOWERING_HOT_PATH_POLICY);
    assert_eq!(package.sim_access_mode, ENGINE_LOWERING_SIM_ACCESS_MODE);
    assert!(!package.calyx_live_query_allowed_in_hot_path);
    assert_eq!(
        package.sim_order_source().kind,
        SimOrderSourceKind::LoweredAgent
    );
    assert_eq!(verified.field_count, package.fields.len());
    assert_eq!(verified.package_fingerprint, package.package_fingerprint);

    let paths = package
        .fields
        .iter()
        .map(|field| field.path.clone())
        .collect::<Vec<_>>();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
    assert!(paths.contains(&"/hit_points_fp".to_string()));
    assert!(paths.contains(&"/spawn/z_fp".to_string()));
    assert!(paths.contains(&"/tags/0".to_string()));

    println!(
        "issue1118_lowering_valve fields={} fingerprint={} source_cx={} vault={}",
        verified.field_count,
        verified.package_fingerprint,
        verified.source_cx_id,
        vault_dir.display()
    );
}

#[test]
fn lowering_valve_does_not_mutate_existing_sim_hash() {
    let (store, _vault_dir) = content_store("sim-hash");
    let record = content(
        "unit-hash-probe",
        json!({
            "unit_id": "unit-hash-probe",
            "hit_points_fp": 25 * Fixed::ONE_RAW,
            "spawn": {
                "x_fp": Fixed::ONE_RAW,
                "y_fp": 0,
                "z_fp": 0
            }
        }),
    );
    let readback = store.put_content(&record).unwrap();
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(60).unwrap(), 77).unwrap();
    sim.spawn_body(FixedVec3::ZERO, FixedVec3::ZERO, FixedVec3::ZERO)
        .unwrap();
    let before = sim.state_hash();

    let package = EngineLoweringValve::lower_content_for_sim(&readback).unwrap();

    assert_eq!(sim.state_hash(), before);
    assert_eq!(
        package.sim_order_source().kind,
        SimOrderSourceKind::LoweredAgent
    );
}

#[test]
fn lowering_valve_rejects_float_payloads_and_policy_mismatch() {
    let (store, _vault_dir) = content_store("invalid");
    let float_record = content(
        "unit-float",
        json!({
            "unit_id": "unit-float",
            "move_speed_fp": 1.5
        }),
    );
    let float_readback = store.put_content(&float_record).unwrap();
    match EngineLoweringValve::lower_content_for_sim(&float_readback) {
        Err(EngineLoweringError::InvalidInput { field, reason }) => {
            assert_eq!(field, "/move_speed_fp");
            assert!(reason.contains("floating point"));
        }
        other => panic!("expected float rejection, got {other:?}"),
    }

    let valid_record = content(
        "unit-policy",
        json!({
            "unit_id": "unit-policy",
            "hit_points_fp": 10 * Fixed::ONE_RAW
        }),
    );
    let mut policy_readback = store.put_content(&valid_record).unwrap();
    policy_readback.lowering.hot_path_policy = "live_calyx_query".to_string();
    match EngineLoweringValve::lower_content_for_sim(&policy_readback) {
        Err(EngineLoweringError::InvalidInput { field, reason }) => {
            assert_eq!(field, "lowering.hot_path_policy");
            assert!(reason.contains(ENGINE_LOWERING_HOT_PATH_POLICY));
        }
        other => panic!("expected policy rejection, got {other:?}"),
    }
}

#[test]
fn lowering_valve_rejects_hot_path_invocation() {
    let (store, _vault_dir) = content_store("hot-path");
    let record = content(
        "unit-hot-path",
        json!({
            "unit_id": "unit-hot-path",
            "hit_points_fp": 8 * Fixed::ONE_RAW
        }),
    );
    let readback = store.put_content(&record).unwrap();

    let _guard = enter_hot_path(HotPathPhase::SimTick);
    match EngineLoweringValve::lower_content_for_sim(&readback) {
        Err(EngineLoweringError::HotPath { phase }) => {
            assert_eq!(phase, HotPathPhase::SimTick);
        }
        other => panic!("expected hot path rejection, got {other:?}"),
    }
}

fn content_store(label: &str) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = unique_vault_dir(label);
    let db = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(db, label),
        vault_dir,
    )
}

fn content(id: &str, payload: Value) -> EngineContentRecord {
    EngineContentRecord {
        kind: EngineContentKind::Unit,
        id: id.to_string(),
        display_name: id.replace('-', " "),
        payload,
        source: EngineContentSource {
            path: format!("synthetic://issue1118/{id}"),
            author: "issue1118-test".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1118:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1118_lowering_valve".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1118-{label}"))
}
