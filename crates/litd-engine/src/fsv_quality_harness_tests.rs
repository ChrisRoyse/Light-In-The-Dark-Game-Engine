use crate::fsv_quality_harness::{
    ISSUE1149_FSV_SCHEMA, ISSUE1149_FSV_SLUG, run_issue1149_fsv_quality_harness,
};

#[test]
fn issue1149_harness_persists_render_replay_and_calyx_readback() {
    let readback = pollster::block_on(run_issue1149_fsv_quality_harness()).unwrap();
    let report = &readback.json;

    assert_eq!(report["schema"], ISSUE1149_FSV_SCHEMA);
    assert_eq!(report["issue"], 1149);
    assert_eq!(
        report["happy_path"]["render_stat_readback"]["after"]["draw_calls"],
        2
    );
    assert_eq!(
        report["happy_path"]["render_stat_readback"]["after"]["draw_budget_ok"],
        true
    );
    assert_eq!(
        report["happy_path"]["determinism_replay"]["after"]["hashes_match"],
        true
    );
    assert_eq!(
        report["happy_path"]["determinism_replay"]["after"]["orders_applied"],
        3
    );
    assert_eq!(
        report["happy_path"]["calyx_round_trip"]["after"]["payload"]["render_draw_budget_ok"],
        true
    );
    assert_eq!(
        report["edges"]["invalid_render_size"]["after"]["error_code"],
        "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["replay_hash_mismatch"]["after"]["error_code"],
        "LITD-REFLECT-FINGERPRINT-MISMATCH"
    );
    assert_eq!(
        report["edges"]["calyx_invalid_record"]["after"]["error_kind"],
        "InvalidRecord"
    );
    assert_eq!(
        report["edges"]["calyx_missing_record"]["after"]["error_kind"],
        "MissingRecord"
    );
    assert_eq!(readback.manifest["issue_slug"], ISSUE1149_FSV_SLUG);
    assert_eq!(readback.manifest["report_sha256"], readback.sha256);

    println!(
        "ISSUE1149_FSV_REPORT_READBACK path={} bytes={} sha256={} render_bmp_sha256={} replay_final_hash={} calyx_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        report["happy_path"]["render_stat_readback"]["after"]["bmp_readback"]["sha256"],
        report["happy_path"]["determinism_replay"]["after"]["first_final_hash"],
        report["happy_path"]["calyx_round_trip"]["after"]["sha256"],
    );
}
