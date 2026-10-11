use std::{fs, path::PathBuf};

use super::*;

// #1543: these were five HARDCODED machine-specific absolute paths into the checkout root —
// checkout-specific knowledge scattered through the tree. They now go through the one
// authoritative RUNTIME resolver, so they work from the primary checkout, from any linked git
// worktree, and from any clone on any machine.
const MOUNTAINSIDE_GLTF: &str = "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf";
const CASTLE_WALL_GLB: &str = "extracted/kenney/game-kit/castle-kit/Models/GLB_format/wall.glb";

fn vault_path(relative: &str) -> PathBuf {
    // #1825: the resolver returns a worktree-portable ABSOLUTE path but does not check
    // existence (callers may create files there). These fixtures read a real base asset, so a
    // missing file otherwise surfaced as a bare `os error 3`. Fail loud naming the fixture and
    // the provisioning step instead.
    let path = litd_engine::asset_vault_root::local_asset_vault_path(relative).unwrap();
    assert!(
        path.exists(),
        "{}",
        crate::asset_vault_fixture::missing_fixture(relative)
    );
    path
}

#[test]
fn validates_real_polyhaven_gltf_and_dependencies() {
    let path = vault_path(MOUNTAINSIDE_GLTF);
    let report = validate_gltf_source(
        &path,
        "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf",
    )
    .unwrap();

    assert_eq!(report.mesh_count, 1);
    assert_eq!(report.primitive_count, 1);
    assert_eq!(report.vertex_count, 80_238);
    assert_eq!(report.index_count, 460_416);
    assert_eq!(report.triangle_count, 153_472);
    assert_eq!(report.degenerate_triangle_count, 0);
    assert_eq!(report.buffer_count, 1);
    assert_eq!(report.image_count, 3);
    assert_eq!(report.dependencies.len(), 4);
    assert!(
        report
            .dependencies
            .contains(&"sources/polyhaven/models/mountainside/2k/mountainside.bin".to_string())
    );
    println!(
        "gltf_happy meshes={} primitives={} vertices={} indices={} triangles={} degenerates={} deps={}",
        report.mesh_count,
        report.primitive_count,
        report.vertex_count,
        report.index_count,
        report.triangle_count,
        report.degenerate_triangle_count,
        report.dependencies.join("|")
    );
}

#[test]
fn real_degenerate_glb_fails_loud_with_source_context() {
    let path = vault_path(CASTLE_WALL_GLB);
    println!(
        "edge=real_degenerate_glb before_exists={} path={}",
        path.exists(),
        path.display()
    );

    let error = validate_gltf_source(
        &path,
        "extracted/kenney/game-kit/castle-kit/Models/GLB_format/wall.glb",
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("editor.asset_product.gltf.geometry.degenerate_triangles"));
    assert!(error.contains("wall.glb"));
    assert!(error.contains("mesh 0 primitive 0"));
    assert!(error.contains("clean or replace the source asset before import"));
    println!(
        "edge=real_degenerate_glb after_exists={} error={}",
        path.exists(),
        error
    );
}

#[test]
fn missing_external_resource_fails_loud() {
    let (_temp_root, path) = temp_path("missing-resource");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::copy(vault_path(MOUNTAINSIDE_GLTF), &path).unwrap();
    println!("edge=missing_resource before_exists={}", path.exists());

    let error = validate_gltf_source(
        &path,
        "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf",
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("editor.asset_product.gltf.dependency.exists"));
    println!(
        "edge=missing_resource after_exists={} error={}",
        path.exists(),
        error
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn invalid_gltf_json_fails_loud() {
    let (_temp_root, path) = temp_path("invalid-json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{not-json}\n").unwrap();
    println!("edge=invalid_json before_exists={}", path.exists());

    let error = validate_gltf_source(&path, "sources/test/invalid-json.gltf")
        .unwrap_err()
        .to_string();

    assert!(error.contains("editor.asset_product.gltf.parse"));
    println!(
        "edge=invalid_json after_exists={} bytes={} error={}",
        path.exists(),
        fs::metadata(&path).unwrap().len(),
        error
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn escaping_dependency_uri_fails_before_import() {
    let (_temp_root, path) = temp_path("escaping-dependency");
    let mut text = fs::read_to_string(vault_path(MOUNTAINSIDE_GLTF)).unwrap();
    text = text.replacen(
        "\"uri\": \"mountainside.bin\"",
        "\"uri\": \"../mountainside.bin\"",
        1,
    );
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    println!("edge=escaping_dependency before_exists={}", path.exists());

    let error = validate_gltf_source(
        &path,
        "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf",
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("editor.asset_product.gltf.dependency.uri"));
    println!(
        "edge=escaping_dependency after_exists={} error={}",
        path.exists(),
        error
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn required_extension_fails_loud() {
    let (_temp_root, path) = temp_path("required-extension");
    let mut text = fs::read_to_string(vault_path(MOUNTAINSIDE_GLTF)).unwrap();
    text = text.replacen(
        "\"asset\": {",
        "\"extensionsRequired\": [\"KHR_draco_mesh_compression\"],\n  \"asset\": {",
        1,
    );
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    println!("edge=required_extension before_exists={}", path.exists());

    let error = validate_gltf_source(
        &path,
        "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf",
    )
    .unwrap_err()
    .to_string();

    assert!(
        error.contains("editor.asset_product.gltf.extensions_required")
            || error.contains("editor.asset_product.gltf.parse")
            || error.contains("editor.asset_product.gltf.import")
    );
    println!(
        "edge=required_extension after_exists={} error={}",
        path.exists(),
        error
    );
    fs::remove_file(path).unwrap();
}

// #1535: own a `TestTempDir` guard for the parent dir so it is removed on Drop. The old raw
// `env::temp_dir().join("litd-editor-gltf-validation-tests")` parent leaked forever (callers
// remove the .gltf file but never the dir). Callers MUST bind the returned guard.
fn temp_path(name: &str) -> (litd_engine::test_temp::TestTempDir, PathBuf) {
    let root = litd_engine::test_temp::TestTempDir::new(&format!("editor-gltf-validation-{name}"));
    let path = root.path().join(format!("{name}.gltf"));
    (root, path)
}
