use litd_hash::sha256_hex;
use std::{fs, path::PathBuf};

use super::*;
use crate::asset_vault::{AssetVaultCatalog, AssetVaultRecord};

const REAL_PNG: &str = "extracted/kenney/game-kit/tiny-dungeon/Tiles/tile_0000.png";
const REAL_JPG: &str = "sources/quaternius/game-kit/junkfood/Blender/Textures/CupcakeTexture.jpg";
const REAL_NORMAL_PNG: &str = "extracted/quaternius/manual-game-kit/universalbasecharacters/Universal_Base_Characters_Standard/Universal_Base_Characters_Standard/Base_Characters/Textures/T_Eye_Normal.png";
const REAL_AUDIO: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";

#[test]
fn imports_real_png_and_jpg_to_basis_and_bc7() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let png_record = real_record(&catalog, REAL_PNG);
    let jpg_record = real_record(&catalog, REAL_JPG);
    let normal_record = real_record(&catalog, REAL_NORMAL_PNG);

    let png = import_texture_asset(&png_record, TextureRole::UiSprite).unwrap();
    let jpg = import_texture_asset(&jpg_record, TextureRole::BaseColor).unwrap();
    let normal = import_texture_asset(&normal_record, TextureRole::Normal).unwrap();

    assert_eq!(
        png.receipt.source_sha256,
        file_sha256(&png_record.absolute_path)
    );
    assert_eq!(
        jpg.receipt.source_sha256,
        file_sha256(&jpg_record.absolute_path)
    );
    assert_eq!(
        normal.receipt.source_sha256,
        file_sha256(&normal_record.absolute_path)
    );
    assert_eq!(png.receipt.source_format, "png");
    assert_eq!(jpg.receipt.source_format, "jpeg");
    assert_eq!(normal.receipt.source_format, "png");
    assert_eq!(png.receipt.color_space, TextureColorSpace::Srgb);
    assert_eq!(jpg.receipt.color_space, TextureColorSpace::Srgb);
    assert_eq!(normal.receipt.color_space, TextureColorSpace::Linear);
    assert!(png.receipt.width > 0 && png.receipt.height > 0);
    assert!(jpg.receipt.width > 0 && jpg.receipt.height > 0);
    assert!(normal.receipt.width > 0 && normal.receipt.height > 0);
    assert_eq!(
        png.receipt.decoded_rgba8_bytes,
        (png.receipt.width * png.receipt.height * 4) as usize
    );
    assert_eq!(
        jpg.receipt.decoded_rgba8_bytes,
        (jpg.receipt.width * jpg.receipt.height * 4) as usize
    );
    assert!(png.receipt.basis_bytes > 0);
    assert!(jpg.receipt.basis_bytes > 0);
    assert!(normal.receipt.basis_bytes > 0);
    assert_eq!(normal.receipt.basis_container_format, "UASTC4x4");
    assert_imported_mip_chain(&png);
    assert_imported_mip_chain(&jpg);
    assert_imported_mip_chain(&normal);
    assert_eq!(png.receipt.gpu_transcode_format, "BC7_RGBA");
    assert_eq!(jpg.receipt.gpu_transcode_format, "BC7_RGBA");
    assert_eq!(normal.receipt.gpu_transcode_format, "BC7_RGBA");
    println!(
        "issue1100 import happy png={} dims={}x{} source_bytes={} decoded_bytes={} basis={} bc7={} mips={} basis_sha={} bc7_sha={} jpg={} dims={}x{} source_bytes={} decoded_bytes={} basis={} bc7={} mips={} basis_sha={} bc7_sha={} normal={} dims={}x{} source_bytes={} decoded_bytes={} basis_format={} basis={} bc7={} mips={} basis_sha={} bc7_sha={}",
        png.receipt.source_relative_path,
        png.receipt.width,
        png.receipt.height,
        png.receipt.source_bytes,
        png.receipt.decoded_rgba8_bytes,
        png.receipt.basis_bytes,
        png.receipt.gpu_transcode_bytes,
        png.receipt.gpu_mip_levels.len(),
        png.receipt.basis_sha256,
        png.receipt.gpu_transcode_sha256,
        jpg.receipt.source_relative_path,
        jpg.receipt.width,
        jpg.receipt.height,
        jpg.receipt.source_bytes,
        jpg.receipt.decoded_rgba8_bytes,
        jpg.receipt.basis_bytes,
        jpg.receipt.gpu_transcode_bytes,
        jpg.receipt.gpu_mip_levels.len(),
        jpg.receipt.basis_sha256,
        jpg.receipt.gpu_transcode_sha256,
        normal.receipt.source_relative_path,
        normal.receipt.width,
        normal.receipt.height,
        normal.receipt.source_bytes,
        normal.receipt.decoded_rgba8_bytes,
        normal.receipt.basis_container_format,
        normal.receipt.basis_bytes,
        normal.receipt.gpu_transcode_bytes,
        normal.receipt.gpu_mip_levels.len(),
        normal.receipt.basis_sha256,
        normal.receipt.gpu_transcode_sha256
    );
}

#[test]
fn atlas_and_artifacts_are_written_and_read_back_from_disk() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let png =
        import_texture_asset(&real_record(&catalog, REAL_PNG), TextureRole::UiSprite).unwrap();
    let jpg =
        import_texture_asset(&real_record(&catalog, REAL_JPG), TextureRole::BaseColor).unwrap();
    let normal =
        import_texture_asset(&real_record(&catalog, REAL_NORMAL_PNG), TextureRole::Normal).unwrap();
    let textures = vec![png, jpg, normal];
    let atlas = build_texture_atlas(
        &textures,
        TextureAtlasSettings {
            tile_size: 64,
            columns: 3,
            max_size: 256,
        },
    )
    .unwrap();

    assert_eq!(atlas.receipt.texture_count, 3);
    assert_eq!(atlas.receipt.width, 192);
    assert_eq!(atlas.receipt.height, 64);
    assert_eq!(atlas.receipt.placements.len(), 3);
    assert_eq!(atlas.receipt.rgba8_bytes, 192 * 64 * 4);
    assert_eq!(atlas.receipt.rgba8_sha256, sha256_hex(&atlas.rgba8));

    let root = test_dir("issue1100_texture_import_artifacts");
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    let before_exists = root.exists();
    let artifacts = write_texture_import_artifacts(&root, &textures, &atlas).unwrap();
    let manifest_bytes = fs::read(&artifacts.manifest_path).unwrap();
    let atlas_bytes = fs::read(&artifacts.atlas_path).unwrap();
    let manifest = read_texture_import_manifest(&artifacts.manifest_path).unwrap();
    let expected_texture_files = textures.len()
        + textures
            .iter()
            .map(|texture| texture.receipt.gpu_mip_levels.len())
            .sum::<usize>();

    assert!(!before_exists);
    assert_eq!(manifest.textures.len(), 3);
    assert_eq!(manifest.atlas.rgba8_sha256, atlas.receipt.rgba8_sha256);
    assert_eq!(artifacts.manifest_sha256, sha256_hex(&manifest_bytes));
    assert_eq!(artifacts.atlas_sha256, sha256_hex(&atlas_bytes));
    assert_eq!(artifacts.texture_files.len(), expected_texture_files);
    assert_eq!(manifest.texture_files.len(), expected_texture_files);
    for file in &artifacts.texture_files {
        let bytes = fs::read(&file.path).unwrap();
        assert_eq!(file.bytes, bytes.len());
        assert_eq!(file.sha256, sha256_hex(&bytes));
    }
    println!(
        "issue1100 atlas artifacts before_exists={} after_manifest={} manifest_sha={} atlas={} atlas_bytes={} atlas_sha={} texture_files={}",
        before_exists,
        artifacts.manifest_path,
        artifacts.manifest_sha256,
        artifacts.atlas_path,
        artifacts.atlas_bytes,
        artifacts.atlas_sha256,
        artifacts.texture_files.len()
    );
}

#[test]
fn encodes_real_png_to_parseable_ktx2_bytes() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let png_record = real_record(&catalog, REAL_PNG);
    let png = import_texture_asset(&png_record, TextureRole::UiSprite).unwrap();
    let ktx2 = encode_rgba8_ktx2(
        &png.receipt.source_relative_path,
        &png.decoded_rgba8,
        png.receipt.width,
        png.receipt.height,
        "litd-editor texture_import_tests",
    )
    .unwrap();
    let reader = ktx2::Reader::new(ktx2.bytes.as_slice()).unwrap();
    let levels = reader.levels().collect::<Vec<_>>();

    assert_eq!(reader.header().pixel_width, png.receipt.width);
    assert_eq!(reader.header().pixel_height, png.receipt.height);
    assert_eq!(reader.header().format, Some(ktx2::Format::R8G8B8A8_SRGB));
    assert_eq!(levels.len(), 1);
    assert_eq!(levels[0].data, png.decoded_rgba8.as_slice());
    assert_eq!(ktx2.receipt.ktx2_sha256, sha256_hex(&ktx2.bytes));
    println!(
        "issue1297 encode real png={} dims={}x{} ktx2_bytes={} ktx2_sha={} dfd_offset={} dfd_len={} kvd_offset={} kvd_len={} level_offset={} level_bytes={}",
        png.receipt.source_relative_path,
        ktx2.receipt.width,
        ktx2.receipt.height,
        ktx2.receipt.ktx2_bytes,
        ktx2.receipt.ktx2_sha256,
        ktx2.receipt.dfd_byte_offset,
        ktx2.receipt.dfd_byte_length,
        ktx2.receipt.kvd_byte_offset,
        ktx2.receipt.kvd_byte_length,
        ktx2.receipt.level_byte_offset,
        ktx2.receipt.level_byte_length
    );
}

#[test]
fn invalid_texture_inputs_fail_loud_without_source_mutation() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let audio = real_record(&catalog, REAL_AUDIO);
    let png = real_record(&catalog, REAL_PNG);
    let before_audio = file_sha256(&audio.absolute_path);
    let before_png = file_sha256(&png.absolute_path);

    let audio_error = import_texture_asset(&audio, TextureRole::BaseColor)
        .unwrap_err()
        .to_string();
    assert!(audio_error.contains("editor.texture_import.source.extension"));

    let mut png_as_ktx2 = png.clone();
    png_as_ktx2.relative_path =
        "extracted/kenney/game-kit/tiny-dungeon/Tiles/tile_0000.ktx2".to_string();
    let ktx2_error = validate_ktx2_texture_record(&png_as_ktx2)
        .unwrap_err()
        .to_string();
    assert!(ktx2_error.contains("editor.texture_import.ktx2.parse"));

    let atlas_error = build_texture_atlas(&[], TextureAtlasSettings::default())
        .unwrap_err()
        .to_string();
    assert!(atlas_error.contains("editor.texture_import.atlas.inputs"));

    let after_audio = file_sha256(&audio.absolute_path);
    let after_png = file_sha256(&png.absolute_path);
    assert_eq!(before_audio, after_audio);
    assert_eq!(before_png, after_png);
    println!(
        "issue1100 edges audio_before={} audio_after={} audio_error={} png_before={} png_after={} ktx2_error={} atlas_error={}",
        before_audio, after_audio, audio_error, before_png, after_png, ktx2_error, atlas_error
    );
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

fn test_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("fsv")
        .join(name)
}

fn file_sha256(path: &std::path::Path) -> String {
    sha256_hex(&fs::read(path).unwrap())
}

fn assert_imported_mip_chain(texture: &ProcessedTextureAsset) {
    assert!(!texture.gpu_mip_levels.is_empty());
    assert_eq!(
        texture.receipt.mipmaps_generated,
        texture.gpu_mip_levels.len() > 1
    );
    assert_eq!(
        texture.receipt.gpu_mip_levels.len(),
        texture.gpu_mip_levels.len()
    );
    let mut aggregate = Vec::new();
    for (index, mip) in texture.gpu_mip_levels.iter().enumerate() {
        assert_eq!(mip.receipt.level_index, index as u32);
        assert_eq!(texture.receipt.gpu_mip_levels[index], mip.receipt);
        assert_eq!(mip.receipt.gpu_transcode_format, "BC7_RGBA");
        assert_eq!(
            mip.receipt.bytes,
            expected_bc7_bytes(mip.receipt.width, mip.receipt.height).unwrap()
        );
        assert_eq!(mip.receipt.bytes, mip.bytes.len());
        assert_eq!(mip.receipt.sha256, sha256_hex(&mip.bytes));
        aggregate.extend_from_slice(&mip.bytes);
    }
    let base = &texture.gpu_mip_levels[0].receipt;
    assert_eq!(base.width, texture.receipt.width);
    assert_eq!(base.height, texture.receipt.height);
    assert_eq!(texture.receipt.gpu_transcode_bytes, aggregate.len());
    assert_eq!(texture.receipt.gpu_transcode_sha256, sha256_hex(&aggregate));
    assert_eq!(texture.gpu_transcode_bytes, aggregate);
}
