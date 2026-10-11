use std::{fs, path::Path};

use sha2::{Digest, Sha256};

use super::*;

#[test]
fn default_visual_preset_reads_real_readme_reference_images() {
    let preset = default_editor_world_visual_preset();
    let root = workspace_root();
    println!(
        "issue1164 references before root={} list={}",
        root.display(),
        preset.reference_image_list()
    );

    preset.validate_reference_files(root).unwrap();

    for relative in preset.reference_images {
        let path = root.join(relative);
        let bytes = fs::read(&path).unwrap();
        let sha = Sha256::digest(&bytes);
        assert!(bytes.len() > 400_000);
        println!(
            "issue1164 references after path={} bytes={} sha256={:X}",
            path.display(),
            bytes.len(),
            sha
        );
    }
}

#[test]
fn default_visual_preset_validates_real_reference_scene_material_contract() {
    let preset = default_editor_world_visual_preset();
    let scene = build_reference_scene();
    let metadata = scene.metadata();
    println!(
        "issue1164 material_contract before source={} materials={} images={} base_bytes={} normal_bytes={} mr_bytes={}",
        metadata.source_id,
        metadata.material_count,
        metadata.image_count,
        metadata.base_color_texture_bytes,
        metadata.normal_texture_bytes,
        metadata.metallic_roughness_texture_bytes
    );

    preset.validate_scene_metadata(&metadata).unwrap();

    println!(
        "issue1164 material_contract after preset={} camera={} materials={} images={}",
        preset.id, preset.camera_profile_id, metadata.material_count, metadata.image_count
    );
}

#[test]
fn visual_preset_without_reference_images_fails_loud() {
    let before = default_editor_world_visual_preset();
    let invalid = EditorWorldVisualPreset {
        reference_images: &[],
        ..before
    };
    println!(
        "issue1164 empty_references before preset={} reference_count={}",
        invalid.id,
        invalid.reference_images.len()
    );

    let error = invalid
        .validate()
        .expect_err("empty reference list must fail")
        .to_string();

    assert!(error.contains("visual_preset.reference_images"));
    println!("issue1164 empty_references after_error={error}");
}

#[test]
fn visual_preset_missing_reference_file_fails_loud() {
    let preset = default_editor_world_visual_preset();
    // #1535: TestTempDir auto-cleans on Drop; the dir is never created, so it stays absent.
    let _temp_root = crate::test_temp::TestTempDir::new("missing-reference-root");
    let root = _temp_root.path().to_path_buf();
    println!(
        "issue1164 missing_reference before root={} exists={}",
        root.display(),
        root.exists()
    );

    let error = preset
        .validate_reference_files(&root)
        .expect_err("missing physical README references must fail")
        .to_string();

    assert!(error.contains("visual_preset.reference_images"));
    assert!(error.contains("failed to read"));
    println!(
        "issue1164 missing_reference after root={} exists={} error={}",
        root.display(),
        root.exists(),
        error
    );
}

#[test]
fn visual_preset_rejects_nonblank_scene_without_pbr_texture_readback() {
    let preset = default_editor_world_visual_preset();
    let mut metadata = build_reference_scene().metadata();
    metadata.normal_texture_bytes = 0;
    println!(
        "issue1164 missing_normal before source={} normal_path={} normal_bytes={}",
        metadata.source_id, metadata.normal_texture_source_path, metadata.normal_texture_bytes
    );

    let error = preset
        .validate_scene_metadata(&metadata)
        .expect_err("missing normal texture bytes must fail")
        .to_string();

    assert!(error.contains("visual_preset.normal_texture"));
    println!("issue1164 missing_normal after_error={error}");
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-engine must live under workspace crates")
}
