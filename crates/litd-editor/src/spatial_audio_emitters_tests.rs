use std::{fs, path::Path};

use litd_engine::{audio::AudioBus, math::Vec3};

use crate::{
    asset_products::{AssetProduct, AssetProductKind},
    history::{EditorDocument, EditorObject, stable_fingerprint},
    spatial_audio_emitters::{
        AUDIO_PRODUCT_ID_PROPERTY, SOUND_EMITTER_KIND, SpatialAudioEmitterKind,
        SpatialAudioEmitterPlacementRequest, collect_spatial_audio_emitters,
        place_spatial_audio_emitter_in_document,
    },
};

#[test]
fn issue869_places_no_code_spatial_audio_emitter_with_readback() {
    let mut document = test_document("place");
    let before = stable_fingerprint(&document).unwrap();
    let report =
        place_spatial_audio_emitter_in_document(&mut document, &request("audio.crypt_gate"))
            .unwrap();
    let after = stable_fingerprint(&document).unwrap();

    assert_ne!(before, after);
    assert_eq!(report.before_fingerprint, before);
    assert_eq!(report.after_fingerprint, after);
    assert_eq!(report.readback.object_id, "audio.crypt_gate");
    assert_eq!(
        report.readback.product_id,
        "litd.asset.issue869.audio.chime"
    );
    assert_eq!(
        report.readback.kind,
        SpatialAudioEmitterKind::PositionalSound
    );
    assert_eq!(report.readback.bus, AudioBus::Sfx);
    assert_eq!(report.readback.position, [4.0, 0.0, -2.0]);
    assert_eq!(report.readback.direction, [0.0, 0.0, -1.0]);
    let object = document.objects.get("audio.crypt_gate").unwrap();
    assert_eq!(object.kind, SOUND_EMITTER_KIND);
    assert_eq!(
        object.properties.get(AUDIO_PRODUCT_ID_PROPERTY).unwrap(),
        "litd.asset.issue869.audio.chime"
    );
    let emitters = collect_spatial_audio_emitters(&document).unwrap();
    assert_eq!(emitters.len(), 1);
    println!(
        "ISSUE869_EDITOR_EMITTER_READBACK object={} product={} fingerprint={}",
        report.object_id, report.readback.product_id, report.after_fingerprint
    );
}

#[test]
fn issue869_rejects_invalid_audio_emitter_without_mutating_document() {
    let mut document = test_document("reject");
    let before = stable_fingerprint(&document).unwrap();
    let mut bad = request("audio.bad_cone");
    bad.cone_inner_degrees = 180.0;
    bad.cone_outer_degrees = 90.0;

    let error = place_spatial_audio_emitter_in_document(&mut document, &bad).unwrap_err();
    assert!(format!("{error:?}").contains("outer cone"));
    assert_eq!(stable_fingerprint(&document).unwrap(), before);
    assert!(!document.objects.contains_key("audio.bad_cone"));

    let mut bad_product = request("audio.bad_product");
    bad_product.product_id = "litd.asset.missing".to_string();
    let error = place_spatial_audio_emitter_in_document(&mut document, &bad_product).unwrap_err();
    assert!(format!("{error:?}").contains("missing audio product"));
    assert_eq!(stable_fingerprint(&document).unwrap(), before);
}

fn test_document(label: &str) -> EditorDocument {
    let root = crate::test_scratch::issue_scratch_dir("issue869_editor", label);
    fs::create_dir_all(root.join("assets/audio")).unwrap();
    let wav_path = root.join("assets/audio/chime.wav");
    write_wav(&wav_path).unwrap();
    let wav_path = fs::canonicalize(&wav_path).unwrap();
    let bytes = fs::read(&wav_path).unwrap();
    let mut document = EditorDocument::new(
        format!("calyx://world/issue869-{label}"),
        format!("rev-issue869-{label}"),
    )
    .unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "WorldRoot",
            None,
            litd_engine::math::Transform::from_translation(Vec3::ZERO),
        ))
        .unwrap();
    let product = AssetProduct {
        product_id: "litd.asset.issue869.audio.chime".to_string(),
        product_kind: AssetProductKind::Audio,
        source: "issue869-test".to_string(),
        asset_id: "chime".to_string(),
        title: "Issue 869 Chime".to_string(),
        license: "CC0-1.0".to_string(),
        license_url: "https://creativecommons.org/publicdomain/zero/1.0/".to_string(),
        source_url: "local-test".to_string(),
        source_relative_path: "assets/audio/chime.wav".to_string(),
        source_absolute_path: wav_path.display().to_string(),
        source_sha256: litd_hash::sha256_hex(&bytes),
        source_bytes: bytes.len() as u64,
        builder_id: "litd.asset.vault_product_builder".to_string(),
        builder_fingerprint: "asset-products-v1".to_string(),
        deterministic_sub_id: "issue869chime".to_string(),
        dependencies: Vec::new(),
    };
    product.validate().unwrap();
    document
        .asset_products
        .insert(product.product_id.clone(), product);
    document.validate().unwrap();
    document
}

fn request(object_id: &str) -> SpatialAudioEmitterPlacementRequest {
    SpatialAudioEmitterPlacementRequest {
        object_id: object_id.to_string(),
        parent_id: None,
        product_id: "litd.asset.issue869.audio.chime".to_string(),
        trigger_id: "trg_crypt_gate".to_string(),
        cue_id: "snd_crypt_gate".to_string(),
        kind: SpatialAudioEmitterKind::PositionalSound,
        bus: AudioBus::Sfx,
        position: Vec3::new(4.0, 0.0, -2.0),
        velocity: Vec3::new(0.0, 0.0, -4.0),
        direction: Vec3::new(0.0, 0.0, -1.0),
        cone_inner_degrees: 90.0,
        cone_outer_degrees: 180.0,
        min_distance_meters: 1.0,
        max_distance_meters: 28.0,
        spatialization_strength: 0.9,
        volume_db: -8.0,
        playback_rate: 1.0,
        occlusion_factor: 0.2,
    }
}

fn write_wav(path: &Path) -> hound::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for frame in 0..2_205 {
        let sample = ((frame as f32 / 44_100.0) * 440.0 * std::f32::consts::TAU).sin() * 0.2;
        writer.write_sample((sample * i16::MAX as f32) as i16)?;
    }
    writer.finalize()
}
