use super::*;

const FOOTSTEP_OGG: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";
const DUNGEON_TEXTURE: &str = "sources/kaykit/game-kit/KayKit-Dungeon-Remastered-1.0/addons/kaykit_dungeon_remastered/Assets/texture/dungeon_texture.png";

#[test]
fn content_addressed_audio_handle_loads_real_vault_bytes() {
    let store = ContentAddressedAssetStore::load_default_cached().unwrap();
    let record = real_record(store.catalog(), FOOTSTEP_OGG);
    let handle = store.handle_for_record::<AudioAsset>(&record).unwrap();
    let receipt = store.load(handle.clone()).unwrap();

    assert_eq!(handle.asset_kind_label(), "audio");
    assert_eq!(receipt.handle.relative_path(), FOOTSTEP_OGG);
    assert_eq!(receipt.verified_sha256, record.sha256);
    assert_eq!(receipt.bytes_read, record.bytes_on_disk);
    assert_eq!(receipt.bytes.len() as u64, record.bytes_on_disk);
    assert!(!receipt.loaded_on_background_thread);
}

#[test]
fn content_addressed_texture_handle_loads_on_background_thread() {
    let store = ContentAddressedAssetStore::load_default_cached().unwrap();
    let record = real_record(store.catalog(), DUNGEON_TEXTURE);
    let handle = store.handle_for_record::<TextureAsset>(&record).unwrap();
    let job = store.load_background(handle);
    let receipt = job.join().unwrap();

    assert_eq!(receipt.handle.asset_kind_label(), "texture");
    assert_eq!(receipt.verified_sha256, record.sha256);
    assert_eq!(receipt.bytes_read, record.bytes_on_disk);
    assert!(receipt.loaded_on_background_thread);
}

#[test]
fn wrong_type_tampered_sha_and_missing_record_fail_loud() {
    let store = ContentAddressedAssetStore::load_default_cached().unwrap();
    let audio = real_record(store.catalog(), FOOTSTEP_OGG);
    let wrong_type = store
        .handle_for_record::<TextureAsset>(&audio)
        .unwrap_err()
        .to_string();
    assert!(wrong_type.contains("editor.asset_store.handle.kind"));

    let tampered = AssetHandle::<AudioAsset>::new(
        audio.relative_path.clone(),
        "0".repeat(64),
        audio.bytes_on_disk,
    )
    .unwrap();
    let tampered_error = store.load(tampered).unwrap_err().to_string();
    assert!(tampered_error.contains("editor.asset_store.handle.sha256"));

    let missing =
        AssetHandle::<MeshAsset>::new("missing/never-there.glb", "a".repeat(64), 1).unwrap();
    let missing_error = store.load(missing).unwrap_err().to_string();
    assert!(missing_error.contains("editor.asset_store.handle.relative_path"));
}

fn real_record(catalog: &AssetVaultCatalog, relative_path: &str) -> AssetVaultRecord {
    catalog
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "{}",
                crate::asset_vault_fixture::missing_fixture(relative_path)
            )
        })
}
