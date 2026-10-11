use crate::reference_scene_quality::{
    ISSUE1442_REFERENCE_QUALITY_SCHEMA, ISSUE1442_REFERENCE_QUALITY_SLUG,
    run_issue1442_reference_quality_harness,
};

#[test]
fn issue1442_harness_persists_resolution_stable_gpu_readback() {
    let readback = pollster::block_on(run_issue1442_reference_quality_harness()).unwrap();
    let report = &readback.json;

    assert_eq!(report["schema"], ISSUE1442_REFERENCE_QUALITY_SCHEMA);
    assert_eq!(report["issue"], 1442);
    assert_eq!(
        report["happy_path"]["gpu_960x540"]["after"]["comparison"]["pass"],
        true
    );
    assert_eq!(
        report["happy_path"]["gpu_1920x1080"]["after"]["comparison"]["pass"],
        true
    );
    assert_eq!(report["happy_path"]["edge_stability"]["pass"], true);
    assert_eq!(
        report["edges"]["flat_reference_image_fails_edge_band"]["after"]["edge_band"]["pass"],
        false
    );
    assert_eq!(
        readback.manifest["issue_slug"],
        ISSUE1442_REFERENCE_QUALITY_SLUG
    );
    assert_eq!(readback.manifest["report_sha256"], readback.sha256);

    println!(
        "ISSUE1442_FSV_REPORT_READBACK path={} bytes={} sha256={} gpu_960_bmp_sha256={} gpu_1080_bmp_sha256={} edge_delta={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        report["happy_path"]["gpu_960x540"]["after"]["bmp_readback"]["sha256"],
        report["happy_path"]["gpu_1920x1080"]["after"]["bmp_readback"]["sha256"],
        report["happy_path"]["edge_stability"]["delta_abs"],
    );
}
