use crate::visual_target_acceptance::{
    ISSUE1147_VISUAL_TARGET_SCHEMA, ISSUE1147_VISUAL_TARGET_SLUG,
    run_issue1147_visual_target_harness,
};

use std::{fs, path::Path};

#[test]
fn issue1147_harness_persists_visual_target_readback() {
    let readback = pollster::block_on(run_issue1147_visual_target_harness()).unwrap();
    let report = &readback.json;

    assert_eq!(report["schema"], ISSUE1147_VISUAL_TARGET_SCHEMA);
    assert_eq!(report["issue"], 1147);
    assert_eq!(
        report["readme_reference_readback"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    for reference in report["readme_reference_readback"].as_array().unwrap() {
        assert_eq!(reference["format"], "png");
    }
    assert_eq!(report["happy_path"]["settings"]["width"], 960);
    assert_eq!(report["happy_path"]["settings"]["height"], 540);
    assert_eq!(
        report["happy_path"]["cpu_reference"]["comparison"]["pass"],
        true
    );
    assert_eq!(
        report["happy_path"]["gpu_reference"]["comparison"]["pass"],
        true
    );
    assert_eq!(report["happy_path"]["gpu_reference"]["draw_calls"], 2);
    assert_eq!(
        report["edges"]["blank_visual_target"]["after"]["pass"],
        false
    );
    assert_eq!(
        report["edges"]["invalid_cpu_size"]["after"]["error_code"],
        "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["invalid_gpu_size"]["after"]["error_code"],
        "LITD-RENDER-REFERENCE-SCENE-INVALID-CONFIG"
    );
    assert_eq!(
        report["edges"]["missing_reference_image"]["after"]["error_code"],
        "LITD-REFLECT-SERIALIZATION-FAILED"
    );
    assert_eq!(
        readback.manifest["issue_slug"],
        ISSUE1147_VISUAL_TARGET_SLUG
    );
    assert_eq!(readback.manifest["report_sha256"], readback.sha256);

    println!(
        "ISSUE1147_FSV_REPORT_READBACK path={} bytes={} sha256={} cpu_bmp_sha256={} gpu_bmp_sha256={} gpu_draw_calls={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        report["happy_path"]["cpu_reference"]["bmp_readback"]["sha256"],
        report["happy_path"]["gpu_reference"]["bmp_readback"]["sha256"],
        report["happy_path"]["gpu_reference"]["draw_calls"],
    );
}

#[test]
fn issue1441_readme_png_assets_have_png_signature() {
    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    let readme_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/readme");
    let mut checked = 0usize;
    let mut total_bytes = 0u64;

    for entry in fs::read_dir(&readme_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|value| value.to_str()) != Some("png") {
            continue;
        }
        let bytes = fs::read(&path).unwrap();
        assert!(
            bytes.starts_with(&PNG_SIGNATURE),
            "{} has .png extension but does not start with PNG signature",
            path.display()
        );
        checked += 1;
        total_bytes += bytes.len() as u64;
    }

    assert_eq!(checked, 111);
    println!("ISSUE1441_README_PNG_SIGNATURES count={checked} total_bytes={total_bytes}");
}
