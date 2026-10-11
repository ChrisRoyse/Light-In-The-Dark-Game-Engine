use std::{fs, path::PathBuf};

use litd_engine::math::Transform;

use super::*;
use crate::{
    authoring::{
        AssayBalanceProfile, GroundedAbilityRequest, GroundingAnchor, LoweredAbilityDocument,
        generate_grounded_ability, verify_lowered_ability_toml,
    },
    history::EditorObject,
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
        create_editor_world_record,
    },
};

#[test]
fn admits_grounded_and_refuses_unsafe_and_ood_queries() {
    let index = build_world_memory_index(&[
        memory_input(
            "wa-poison",
            "Venom Vale",
            poison_ability("req-guard-poison", "wa-poison"),
        ),
        memory_input(
            "wa-frost",
            "Frost Vale",
            frost_ability("req-guard-frost", "wa-frost"),
        ),
    ])
    .unwrap();
    let ward = search_ward();
    let calibration = search_calibration(300, 50);

    let admitted = guarded_query_world_memory(
        &index,
        &guarded_query("issue1207-happy", "poison stacking ability", 3),
        &ward,
        Some(&calibration),
    )
    .unwrap();
    assert_eq!(admitted.status, GuardedWorldMemorySearchStatus::Admitted);
    assert_eq!(admitted.code, "LITD-SEARCH-ADMITTED");
    assert!(!admitted.hits.is_empty());
    assert_eq!(admitted.hits[0].source_kind, WorldMemorySourceKind::Ability);

    let unsafe_query = guarded_query_world_memory(
        &index,
        &guarded_query("issue1207-unsafe", "spaceship laser exploit", 3),
        &ward,
        Some(&calibration),
    )
    .unwrap();
    assert_eq!(unsafe_query.status, GuardedWorldMemorySearchStatus::Refused);
    assert_eq!(unsafe_query.code, "LITD-SEARCH-REFUSED-UNSAFE");
    assert!(unsafe_query.hits.is_empty());
    assert_eq!(unsafe_query.blocked_terms, vec!["laser", "spaceship"]);

    let ood = guarded_query_world_memory(
        &index,
        &guarded_query("issue1207-ood", "mortgage bakery account", 3),
        &ward,
        Some(&calibration),
    )
    .unwrap();
    assert_eq!(ood.status, GuardedWorldMemorySearchStatus::Refused);
    assert_eq!(ood.code, "LITD-SEARCH-REFUSED-OOD");
    assert!(ood.hits.is_empty());
    println!(
        "issue1207 guarded happy={} hits={} unsafe={} ood={}",
        admitted.code, admitted.hit_count, unsafe_query.code, ood.code
    );
}

#[test]
fn edges_fail_closed() {
    let index = build_world_memory_index(&[memory_input(
        "wa-poison",
        "Venom Vale",
        poison_ability("req-guard-edges", "wa-poison"),
    )])
    .unwrap();
    let ward = search_ward();

    let borderline = guarded_query_world_memory(
        &index,
        &guarded_query("issue1207-borderline", "antidote", 3),
        &ward,
        Some(&search_calibration(800, 50)),
    )
    .unwrap();
    assert_eq!(borderline.status, GuardedWorldMemorySearchStatus::Refused);
    assert_eq!(borderline.code, "LITD-SEARCH-REFUSED-BORDERLINE");
    assert!(borderline.top_score_milli.unwrap() <= 850);

    let mut empty = index.clone();
    empty.world_count = 0;
    empty.constellation_count = 0;
    empty.constellations.clear();
    let empty_index = guarded_query_world_memory(
        &empty,
        &guarded_query("issue1207-empty-index", "poison", 3),
        &ward,
        Some(&search_calibration(300, 50)),
    )
    .unwrap();
    assert_eq!(empty_index.status, GuardedWorldMemorySearchStatus::Refused);
    assert_eq!(empty_index.code, "LITD-SEARCH-EMPTY-INDEX");

    let missing_calibration = guarded_query_world_memory(
        &index,
        &guarded_query("issue1207-missing-calibration", "poison", 3),
        &ward,
        None,
    )
    .unwrap();
    assert_eq!(missing_calibration.code, "LITD-SEARCH-CALIBRATION-MISSING");
    assert!(missing_calibration.calibration_id.is_none());
    println!(
        "issue1207 edges borderline={} empty={} calibration={}",
        borderline.code, empty_index.code, missing_calibration.code
    );
}

#[test]
fn ledger_persists_to_aster_and_reads_back() {
    let index = build_world_memory_index(&[memory_input(
        "wa-poison",
        "Venom Vale",
        poison_ability("req-guard-ledger", "wa-poison"),
    )])
    .unwrap();
    let ward = search_ward();
    let calibration = search_calibration(300, 50);
    let ledger = build_guarded_world_memory_search_ledger(
        "issue1207-guarded-search-test",
        vec![
            guarded_query_world_memory(
                &index,
                &guarded_query("issue1207-ledger-happy", "poison stacking ability", 3),
                &ward,
                Some(&calibration),
            )
            .unwrap(),
            guarded_query_world_memory(
                &index,
                &guarded_query("issue1207-ledger-unsafe", "spaceship laser", 3),
                &ward,
                Some(&calibration),
            )
            .unwrap(),
            guarded_query_world_memory(
                &index,
                &guarded_query("issue1207-ledger-missing-calibration", "poison", 3),
                &ward,
                None,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let root = test_root("guarded-search-ledger");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("guarded-search-ledger.json");
    let aster_dir = root.join("aster");

    let writeback =
        write_guarded_world_memory_search_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let readback = read_guarded_world_memory_search_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_guarded_world_memory_search_ledger(&readback.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(readback.bytes, writeback.bytes);
    assert_eq!(parsed.verdict_count, 3);
    assert_eq!(
        parsed.verdicts[0].status,
        GuardedWorldMemorySearchStatus::Admitted
    );
    assert_eq!(parsed.verdicts[1].code, "LITD-SEARCH-REFUSED-UNSAFE");
    assert_eq!(parsed.verdicts[2].code, "LITD-SEARCH-CALIBRATION-MISSING");
    println!(
        "issue1207 aster verdicts={} graph_rows={} sha={}",
        parsed.verdict_count, readback.graph_rows, readback.sha256
    );
}

fn memory_input(
    world_id: &str,
    world_name: &str,
    ability: LoweredAbilityDocument,
) -> AuthoredWorldMemoryInput {
    let world = world_record(world_id, world_name);
    AuthoredWorldMemoryInput {
        scripts: vec![generated_world_script(&world).unwrap()],
        world,
        abilities: vec![ability],
        objects: vec![
            EditorObject::new("unit_venom_archer", "Unit", None, Transform::IDENTITY)
                .with_property("name", "Venom Archer")
                .with_property("role", "poison stacking ranged unit")
                .with_property("source_ref", "Aster://wa-poison/units/venom-archer"),
            EditorObject::new("item_antidote", "Item", None, Transform::IDENTITY)
                .with_property("name", "Antidote")
                .with_property("description", "cleanses poison stacks")
                .with_property("source_ref", "Aster://wa-poison/items/antidote"),
        ],
    }
}

fn poison_ability(request_id: &str, world_id: &str) -> LoweredAbilityDocument {
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: request_id.to_string(),
            prompt: "Create a Venom Circle poison stacking venom dose ability for Mire Reach toxin grove."
                .to_string(),
            seed: 1207,
            requested_by: "issue1207-test".to_string(),
        },
        &poison_corpus(world_id),
        &poison_assay(),
        &authoring_ward(),
    )
    .unwrap();
    let lowered = outcome.lowered.unwrap();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap()
}

fn frost_ability(request_id: &str, world_id: &str) -> LoweredAbilityDocument {
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: request_id.to_string(),
            prompt:
                "Create a Snow Court frost veil moon shard ability for Frost Reach glacier hollow."
                    .to_string(),
            seed: 2207,
            requested_by: "issue1207-test".to_string(),
        },
        &frost_corpus(world_id),
        &AssayBalanceProfile {
            allowed_damage_types: vec!["frost veil".to_string(), "moon shard".to_string()],
            ..poison_assay()
        },
        &authoring_ward(),
    )
    .unwrap();
    let lowered = outcome.lowered.unwrap();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap()
}

fn world_record(world_id: &str, name: &str) -> EditorWorldRecord {
    let properties = EditorWorldProperties {
        display_name: name.to_string(),
        description: format!("{name} guarded-search fixture"),
        width_tiles: 8,
        height_tiles: 8,
        playable_bounds: EditorPlayableBounds {
            min_tile_x: 0,
            min_tile_z: 0,
            max_tile_x_exclusive: 8,
            max_tile_z_exclusive: 8,
        },
        ..Default::default()
    };
    create_editor_world_record(EditorWorldCreationSpec {
        calyx_document_id: world_id.to_string(),
        revision_id: format!("{world_id}-rev"),
        properties,
    })
    .unwrap()
}

fn poison_corpus(world_id: &str) -> WorldCorpus {
    WorldCorpus {
        world_id: world_id.to_string(),
        revision_id: format!("{world_id}-rev"),
        region: "Mire Reach".to_string(),
        factions: vec!["Venom Circle".to_string()],
        lore_terms: vec![
            "poison stacking".to_string(),
            "venom dose".to_string(),
            "toxin grove".to_string(),
        ],
        anchors: vec![
            anchor("poison-region", world_id, "Mire Reach", "toxin grove"),
            anchor(
                "poison-faction",
                world_id,
                "Venom Circle",
                "poison stacking",
            ),
            anchor("poison-reagent", world_id, "venom dose", "poison stacking"),
        ],
    }
}

fn frost_corpus(world_id: &str) -> WorldCorpus {
    WorldCorpus {
        world_id: world_id.to_string(),
        revision_id: format!("{world_id}-rev"),
        region: "Frost Reach".to_string(),
        factions: vec!["Snow Court".to_string()],
        lore_terms: vec![
            "frost veil".to_string(),
            "moon shard".to_string(),
            "glacier hollow".to_string(),
        ],
        anchors: vec![
            anchor("frost-region", world_id, "Frost Reach", "glacier hollow"),
            anchor("frost-faction", world_id, "Snow Court", "frost veil"),
            anchor("frost-reagent", world_id, "moon shard", "frost veil"),
        ],
    }
}

fn poison_assay() -> AssayBalanceProfile {
    AssayBalanceProfile {
        assay_id: "assay-1207".to_string(),
        profile_id: "ability-tier-1-control".to_string(),
        min_damage: 18,
        max_damage: 54,
        min_cooldown_seconds: 6,
        max_cooldown_seconds: 16,
        max_mana_cost: 70,
        allowed_damage_types: vec!["poison stacking".to_string(), "venom dose".to_string()],
    }
}

fn authoring_ward() -> WardPolicy {
    WardPolicy {
        policy_id: "ward-authoring-issue1207".to_string(),
        min_source_anchors: 3,
        min_grounded_terms: 4,
        blocked_terms: vec!["spaceship".to_string(), "laser".to_string()],
    }
}

fn search_ward() -> WardPolicy {
    WardPolicy {
        policy_id: "ward-search-issue1207".to_string(),
        min_source_anchors: 1,
        min_grounded_terms: 1,
        blocked_terms: vec![
            "spaceship".to_string(),
            "laser".to_string(),
            "stock market".to_string(),
        ],
    }
}

fn search_calibration(tau_milli: u16, margin_milli: u16) -> WorldMemorySearchCalibration {
    WorldMemorySearchCalibration {
        calibration_id: format!("issue1207-search-calibration-{tau_milli}"),
        tau_milli,
        borderline_margin_milli: margin_milli,
    }
}

fn guarded_query(query_id: &str, query: &str, limit: usize) -> GuardedWorldMemoryQuery {
    GuardedWorldMemoryQuery {
        query_id: query_id.to_string(),
        audience: WorldMemorySearchAudience::Agent,
        query: query.to_string(),
        limit,
        requested_by: "issue1207-test".to_string(),
    }
}

fn anchor(id: &str, world_id: &str, first: &str, second: &str) -> GroundingAnchor {
    GroundingAnchor {
        anchor_id: id.to_string(),
        source_ref: format!("Aster://{world_id}/anchors/{id}"),
        text: format!("{first} anchors {second}."),
        terms: vec![first.to_string(), second.to_string()],
    }
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1207", label)
}
