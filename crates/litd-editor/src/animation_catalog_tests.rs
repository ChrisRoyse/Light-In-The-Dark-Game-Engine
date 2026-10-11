use super::*;

use crate::asset_vault::{AssetVaultCatalog, AssetVaultRecord};

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const LEELA_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Leela.gltf";
const GATE_GLB: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";

#[test]
fn search_finds_real_sword_attack_clip_by_description_frames_and_rig() {
    let stan = real_record(STAN_GLTF);
    let index = build_animation_clip_index_from_records(&[stan]).unwrap();
    let rig = index.rigs.first().unwrap().clone();
    let result = search_animation_clips(
        &index,
        AnimationClipQuery {
            description: "sword attack".to_string(),
            min_frame_count: Some(19),
            max_frame_count: Some(19),
            rig: Some(AnimationRigFilter {
                compatibility_key: rig.compatibility_key.clone(),
                exact_rig_id: Some(rig.rig_id.clone()),
                allow_retarget: false,
            }),
        },
    )
    .unwrap();

    assert!(result.match_count >= 1);
    let hit = &result.matches[0];
    assert_eq!(hit.clip.name, "SwordSlash");
    assert_eq!(hit.clip.frame_count, 19);
    assert_eq!(hit.clip.duration_ms, 750);
    assert_eq!(hit.clip.rig.rig_kind, AnimationRigKind::SkinnedBiped);
    assert_eq!(hit.clip.rig.joint_count, 43);
    assert!(!hit.retargetable);
    assert!(hit.matched_terms.contains(&"sword".to_string()));
    assert!(hit.matched_terms.contains(&"attack".to_string()));
}

#[test]
fn no_match_query_returns_empty_without_fallback_clip() {
    let index = build_animation_clip_index_from_records(&[real_record(STAN_GLTF)]).unwrap();
    let result = search_animation_clips(
        &index,
        AnimationClipQuery {
            description: "swimming crawl".to_string(),
            min_frame_count: None,
            max_frame_count: None,
            rig: None,
        },
    )
    .unwrap();

    assert_eq!(result.match_count, 0);
    assert!(result.matches.is_empty());
}

#[test]
fn rig_mismatch_is_excluded_unless_retargeting_is_allowed() {
    let stan_index = build_animation_clip_index_from_records(&[real_record(STAN_GLTF)]).unwrap();
    let leela_index = build_animation_clip_index_from_records(&[real_record(LEELA_GLTF)]).unwrap();
    let leela_rig = leela_index.rigs.first().unwrap();

    let result = search_animation_clips(
        &stan_index,
        AnimationClipQuery {
            description: "sword attack".to_string(),
            min_frame_count: Some(19),
            max_frame_count: Some(19),
            rig: Some(AnimationRigFilter {
                compatibility_key: leela_rig.compatibility_key.clone(),
                exact_rig_id: Some(leela_rig.rig_id.clone()),
                allow_retarget: false,
            }),
        },
    )
    .unwrap();
    assert_eq!(result.match_count, 0);
}

#[test]
fn retargetable_clip_is_flagged_for_same_biped_family() {
    let stan_index = build_animation_clip_index_from_records(&[real_record(STAN_GLTF)]).unwrap();
    let leela_index = build_animation_clip_index_from_records(&[real_record(LEELA_GLTF)]).unwrap();
    let leela_rig = leela_index.rigs.first().unwrap();

    let result = search_animation_clips(
        &stan_index,
        AnimationClipQuery {
            description: "sword attack".to_string(),
            min_frame_count: Some(19),
            max_frame_count: Some(19),
            rig: Some(AnimationRigFilter {
                compatibility_key: leela_rig.compatibility_key.clone(),
                exact_rig_id: Some(leela_rig.rig_id.clone()),
                allow_retarget: true,
            }),
        },
    )
    .unwrap();
    let hit = result.matches.first().unwrap();

    assert_eq!(hit.clip.name, "SwordSlash");
    assert!(hit.retargetable);
    assert_eq!(hit.clip.rig.compatibility_key, "biped");
    assert_ne!(hit.clip.rig.rig_id, leela_rig.rig_id);
}

#[test]
fn unskinned_animation_indexes_but_does_not_match_biped_filter() {
    let index = build_animation_clip_index_from_records(&[real_record(GATE_GLB)]).unwrap();
    assert_eq!(index.rig_count, 1);
    assert_eq!(
        index.rigs[0].rig_kind,
        AnimationRigKind::UnskinnedNodeAnimation
    );
    assert_eq!(index.clips[0].name, "open");

    let result = search_animation_clips(
        &index,
        AnimationClipQuery {
            description: "open".to_string(),
            min_frame_count: None,
            max_frame_count: None,
            rig: Some(AnimationRigFilter {
                compatibility_key: "biped".to_string(),
                exact_rig_id: None,
                allow_retarget: true,
            }),
        },
    )
    .unwrap();
    assert_eq!(result.match_count, 0);
}

#[test]
fn invalid_query_and_empty_records_fail_loud() {
    match build_animation_clip_index_from_records(&[]) {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
            assert_eq!(field, "editor.animation_catalog.records");
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }

    let index = build_animation_clip_index_from_records(&[real_record(STAN_GLTF)]).unwrap();
    match search_animation_clips(
        &index,
        AnimationClipQuery {
            description: String::new(),
            min_frame_count: Some(20),
            max_frame_count: Some(19),
            rig: None,
        },
    ) {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
            assert_eq!(field, "editor.animation_catalog.query.frame_count");
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn real_record(relative_path: &str) -> AssetVaultRecord {
    AssetVaultCatalog::load_default_cached()
        .unwrap()
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
