use std::fs;

use super::*;

use crate::asset_vault::{AssetVaultCatalog, AssetVaultRecord};

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const LEELA_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Leela.gltf";
const KAYKIT_GLB: &str = "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/gltf/floor_dirt_large_rocky.gltf.glb";
const KAYKIT_TEXTURE: &str = "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/texture/dungeon_texture.png";
const FOOTSTEP_OGG: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";
const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";
const MOUNTAINSIDE_GLTF: &str = "sources/polyhaven/models/mountainside/2k/mountainside_2k.gltf";

#[test]
fn ingests_real_multimodal_constellations_and_finds_like_mesh() {
    let records = real_records(&[
        STAN_GLTF,
        LEELA_GLTF,
        KAYKIT_GLB,
        KAYKIT_TEXTURE,
        FOOTSTEP_OGG,
    ]);
    let index = build_asset_constellation_index(&records).unwrap();

    assert_eq!(index.constellation_count, 5);
    assert_eq!(index.modality_counts[&AssetModality::Text], 5);
    assert_eq!(index.modality_counts[&AssetModality::MeshStructured], 3);
    assert_eq!(index.modality_counts[&AssetModality::Image], 4);
    assert_eq!(index.modality_counts[&AssetModality::DocumentImage], 1);
    assert_eq!(index.modality_counts[&AssetModality::Audio], 1);

    let kaykit = constellation(&index, KAYKIT_GLB);
    assert_populated(kaykit, AssetModality::Text);
    assert_populated(kaykit, AssetModality::Image);
    assert_populated(kaykit, AssetModality::MeshStructured);
    assert!(kaykit.tags.iter().any(|tag| tag == "source:kaykit"));

    let texture = constellation(&index, KAYKIT_TEXTURE);
    assert_populated(texture, AssetModality::Image);
    assert_populated(texture, AssetModality::DocumentImage);

    let audio = constellation(&index, FOOTSTEP_OGG);
    assert_populated(audio, AssetModality::Audio);
    assert!(audio.roles.iter().any(|role| role == "audio"));

    let hits = find_assets_like_this(&index, STAN_GLTF, 3).unwrap();
    assert_eq!(hits[0].relative_path, LEELA_GLTF);
    assert!(hits[0].score > hits[1].score);
    println!(
        "issue1233 happy constellations={} text={} image={} doc_image={} audio={} mesh={} top_neighbor={} score={}",
        index.constellation_count,
        index.modality_counts[&AssetModality::Text],
        index.modality_counts[&AssetModality::Image],
        index.modality_counts[&AssetModality::DocumentImage],
        index.modality_counts[&AssetModality::Audio],
        index.modality_counts[&AssetModality::MeshStructured],
        hits[0].relative_path,
        hits[0].score
    );
}

#[test]
fn audio_only_asset_populates_audio_and_text_without_fake_visual_slots() {
    let records = real_records(&[FOOTSTEP_OGG]);
    let index = build_asset_constellation_index(&records).unwrap();
    let audio = constellation(&index, FOOTSTEP_OGG);

    assert_populated(audio, AssetModality::Text);
    assert_populated(audio, AssetModality::Audio);
    assert_missing(audio, AssetModality::Image);
    assert_missing(audio, AssetModality::DocumentImage);
    assert_missing(audio, AssetModality::MeshStructured);
    println!(
        "issue1233 edge=audio_only text={:?} audio={:?} image={:?}",
        audio.slot(AssetModality::Text).unwrap().state,
        audio.slot(AssetModality::Audio).unwrap().state,
        audio.slot(AssetModality::Image).unwrap().state
    );
}

#[test]
fn corrupt_mesh_is_rejected_before_constellation_success() {
    let mut record = real_record(GATE_GLB);
    // #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1233-corrupt-mesh");
    let dir = _temp_root.path().to_path_buf();
    fs::create_dir_all(&dir).unwrap();
    let corrupt_path = dir.join("gate.glb");
    fs::write(&corrupt_path, vec![b'{'; record.bytes_on_disk as usize]).unwrap();
    println!(
        "issue1233 edge=corrupt_mesh before_path={} before_bytes={}",
        corrupt_path.display(),
        fs::metadata(&corrupt_path).unwrap().len()
    );
    record.absolute_path = corrupt_path;

    let error = build_asset_constellation_index(&[record]).unwrap_err();

    assert!(error.to_string().contains("gltf"));
    println!("issue1233 edge=corrupt_mesh after_error={error}");
}

#[test]
fn missing_thumbnail_is_recorded_without_synthetic_image_slot() {
    let records = real_records(&[MOUNTAINSIDE_GLTF]);
    let index = build_asset_constellation_index(&records).unwrap();
    let mountainside = constellation(&index, MOUNTAINSIDE_GLTF);

    assert_missing(mountainside, AssetModality::Image);
    assert_populated(mountainside, AssetModality::MeshStructured);
    assert_populated(mountainside, AssetModality::Text);
    println!(
        "issue1233 edge=missing_thumbnail image_state={:?} image_reason={} mesh_state={:?}",
        mountainside.slot(AssetModality::Image).unwrap().state,
        mountainside.slot(AssetModality::Image).unwrap().evidence["reason"],
        mountainside
            .slot(AssetModality::MeshStructured)
            .unwrap()
            .state
    );
}

#[test]
fn invalid_queries_fail_loud() {
    let error = build_asset_constellation_index(&[]).unwrap_err();
    assert!(error.to_string().contains("records"));

    let records = real_records(&[STAN_GLTF, LEELA_GLTF]);
    let index = build_asset_constellation_index(&records).unwrap();
    let zero = find_assets_like_this(&index, STAN_GLTF, 0).unwrap_err();
    let missing = find_assets_like_this(&index, "missing.gltf", 1).unwrap_err();
    assert!(zero.to_string().contains("query.limit"));
    assert!(missing.to_string().contains("query.source"));
    println!("issue1233 edge=invalid empty={error} zero={zero} missing={missing}");
}

fn assert_populated(asset: &AssetMultimodalConstellation, modality: AssetModality) {
    assert_eq!(
        asset.slot(modality).unwrap().state,
        AssetSlotState::Populated
    );
}

fn assert_missing(asset: &AssetMultimodalConstellation, modality: AssetModality) {
    assert_eq!(asset.slot(modality).unwrap().state, AssetSlotState::Missing);
}

fn constellation<'a>(
    index: &'a AssetConstellationIndex,
    path: &str,
) -> &'a AssetMultimodalConstellation {
    index
        .constellations
        .iter()
        .find(|asset| asset.relative_path == path)
        .unwrap_or_else(|| panic!("missing constellation {path}"))
}

fn real_records(paths: &[&str]) -> Vec<AssetVaultRecord> {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    paths
        .iter()
        .map(|path| real_record_from(&catalog, path))
        .collect()
}

fn real_record(path: &str) -> AssetVaultRecord {
    real_record_from(&AssetVaultCatalog::load_default_cached().unwrap(), path)
}

fn real_record_from(catalog: &AssetVaultCatalog, path: &str) -> AssetVaultRecord {
    catalog
        .records()
        .iter()
        .find(|record| record.relative_path == path)
        .cloned()
        .unwrap_or_else(|| panic!("{}", crate::asset_vault_fixture::missing_fixture(path)))
}
