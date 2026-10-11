use crate::two_d_pipeline::{ISSUE1154_2D_PIPELINE_SCHEMA, run_issue1154_2d_pipeline_harness};

#[test]
fn issue1154_harness_persists_2d_pipeline_readback() {
    let readback = run_issue1154_2d_pipeline_harness().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&readback.bytes).unwrap();

    assert_eq!(report["schema"], ISSUE1154_2D_PIPELINE_SCHEMA);
    assert_eq!(report["issue"], 1154);
    assert_eq!(report["happy_path"]["plan"]["sprite_count"], 4);
    assert_eq!(report["happy_path"]["plan"]["tile_columns"], 18);
    assert_eq!(report["happy_path"]["plan"]["tile_rows"], 10);
    assert_eq!(report["happy_path"]["plan"]["tile_count"], 180);
    assert_eq!(report["happy_path"]["plan"]["light_count"], 2);
    assert_eq!(report["happy_path"]["plan"]["draw_calls"], 5);
    assert_eq!(
        report["happy_path"]["visual_floor"]["comparison"]["pass"],
        true
    );
    assert_eq!(
        report["edges"]["invalid_output_size"]["after"]["error_code"],
        "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["invalid_sprite_frame"]["after"]["error_code"],
        "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["tile_count_mismatch"]["after"]["error_code"],
        "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["missing_light"]["after"]["error_code"],
        "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["over_draw_budget"]["after"]["error_code"],
        "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
    );
    assert!(readback.sha256.len() == 64);
    println!(
        "ISSUE1154_FSV_REPORT_READBACK path={} bytes={} sha256={} bmp_sha256={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        report["happy_path"]["visual_floor"]["bmp_readback"]["sha256"]
            .as_str()
            .unwrap()
    );
}
