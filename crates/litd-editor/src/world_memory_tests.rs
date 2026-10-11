use litd_engine::math::Transform;

use super::*;
use crate::{
    authoring::{GroundingAnchor, LoweredAbilityDocument},
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
        create_editor_world_record,
    },
};

#[test]
fn ingests_worlds_queries_poison_ability_and_reuses_into_new_world() {
    let poison_world = memory_input(
        "wa-poison",
        "Venom Vale",
        poison_ability("req-poison", "wa-poison"),
    );
    let frost_world = memory_input(
        "wa-frost",
        "Frost Vale",
        frost_ability("req-frost", "wa-frost"),
    );
    let index = build_world_memory_index(&[poison_world, frost_world]).unwrap();

    let hits = query_world_memory(&index, "poison stacking ability", 3).unwrap();

    assert_eq!(index.world_count, 2);
    assert!(index.constellation_count >= 8);
    assert_eq!(hits[0].source_kind, WorldMemorySourceKind::Ability);
    assert!(hits[0].title.contains("Poison"));
    assert!(!hits[0].provenance_refs.is_empty());

    let target = poison_corpus("wa-target");
    let reuse = reuse_ability_from_memory(
        &index,
        &WorldMemoryReuseRequest {
            source_constellation_id: hits[0].constellation_id.clone(),
            request_id: "req-reuse-poison".to_string(),
            seed: 999,
            requested_by: "issue1231-test".to_string(),
        },
        &target,
        &poison_assay(),
        &ward(),
    )
    .unwrap();
    let lowered =
        verify_lowered_ability_toml(&reuse.canonical_toml, &reuse.lowered_fingerprint).unwrap();

    assert_eq!(reuse.target_world_id, "wa-target");
    assert_eq!(lowered.ability.world_id, "wa-target");
    assert!(
        lowered
            .ability
            .summary
            .to_ascii_lowercase()
            .contains("poison")
    );
    println!(
        "issue1231 happy before=worlds:2 query=poison_stacking after=top:{} score:{} reuse_world:{} fingerprint:{}",
        hits[0].source_id, hits[0].score, reuse.target_world_id, reuse.lowered_fingerprint
    );
}

#[test]
fn conflicting_cross_world_specs_are_flagged_as_anchor_conflicts() {
    let first = memory_input(
        "wa-conflict-a",
        "Venom A",
        poison_ability("req-conflict-a", "wa-conflict-a"),
    );
    let second = memory_input(
        "wa-conflict-b",
        "Venom B",
        poison_ability("req-conflict-b", "wa-conflict-b"),
    );

    let index = build_world_memory_index(&[first, second]).unwrap();

    assert_eq!(index.conflicts.len(), 1);
    assert_eq!(index.conflicts[0].conflict_kind, "anchor-conflict");
    println!(
        "issue1231 edge=anchor_conflict before=2_poison_worlds after=conflicts:{} reason={}",
        index.conflicts.len(),
        index.conflicts[0].reason
    );
}

#[test]
fn empty_catalog_and_ood_query_fail_closed() {
    let empty_error = build_world_memory_index(&[]).unwrap_err();
    assert!(
        empty_error
            .to_string()
            .contains("editor.world_memory.worlds")
    );

    let index = build_world_memory_index(&[memory_input(
        "wa-poison",
        "Venom Vale",
        poison_ability("req-poison-ood", "wa-poison"),
    )])
    .unwrap();
    let ood_error = query_world_memory(&index, "spaceship stockmarket laser", 3).unwrap_err();
    assert!(
        ood_error
            .to_string()
            .contains("editor.world_memory.query.ood")
    );
    println!(
        "issue1231 edge=fail_closed empty={} ood={}",
        empty_error, ood_error
    );
}

#[test]
fn missing_script_is_rejected_without_synthetic_code_lens() {
    let mut input = memory_input(
        "wa-noscript",
        "No Script",
        poison_ability("req-noscript", "wa-noscript"),
    );
    input.scripts.clear();

    let error = build_world_memory_index(&[input]).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("editor.world_memory.world.scripts")
    );
}

fn memory_input(
    world_id: &str,
    world_name: &str,
    ability: LoweredAbilityDocument,
) -> AuthoredWorldMemoryInput {
    let world = world_record(world_id, world_name);
    let scripts = vec![generated_world_script(&world).unwrap()];
    AuthoredWorldMemoryInput {
        world,
        abilities: vec![ability],
        objects: vec![
            unit_object(
                "unit_venom_archer",
                "Venom Archer",
                "poison stacking ranged unit",
            ),
            item_object("item_antidote", "Antidote", "cleanses poison stacks"),
        ],
        scripts,
    }
}

fn poison_ability(request_id: &str, world_id: &str) -> LoweredAbilityDocument {
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: request_id.to_string(),
            prompt: "Create a Venom Circle poison stacking venom dose ability for Mire Reach toxin grove."
                .to_string(),
            seed: 1231,
            requested_by: "issue1231-test".to_string(),
        },
        &poison_corpus(world_id),
        &poison_assay(),
        &ward(),
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
            seed: 2231,
            requested_by: "issue1231-test".to_string(),
        },
        &frost_corpus(world_id),
        &AssayBalanceProfile {
            allowed_damage_types: vec!["frost veil".to_string(), "moon shard".to_string()],
            ..poison_assay()
        },
        &ward(),
    )
    .unwrap();
    let lowered = outcome.lowered.unwrap();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap()
}

fn world_record(world_id: &str, name: &str) -> EditorWorldRecord {
    let properties = EditorWorldProperties {
        display_name: name.to_string(),
        description: format!("{name} authored-world memory fixture"),
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

fn unit_object(id: &str, name: &str, role: &str) -> EditorObject {
    EditorObject::new(id, "Unit", None, Transform::IDENTITY)
        .with_property("name", name)
        .with_property("unit_type", "unit.venom_archer")
        .with_property("stats", "hp 420 damage 18 poison_stacks 3")
        .with_property("role", role)
        .with_property("source_ref", "Aster://wa-poison/units/venom-archer")
}

fn item_object(id: &str, name: &str, description: &str) -> EditorObject {
    EditorObject::new(id, "Item", None, Transform::IDENTITY)
        .with_property("name", name)
        .with_property("item_type", "item.antidote")
        .with_property("description", description)
        .with_property("source_ref", "Aster://wa-poison/items/antidote")
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
            anchor(
                "poison-region",
                &format!("Aster://{world_id}/world/mire#region"),
                "Mire Reach is a toxin grove.",
                &["Mire Reach", "toxin grove"],
            ),
            anchor(
                "poison-faction",
                &format!("Aster://{world_id}/factions/venom-circle"),
                "Venom Circle tracks poison stacking rites.",
                &["Venom Circle", "poison stacking"],
            ),
            anchor(
                "poison-reagent",
                &format!("Aster://{world_id}/reagents/venom-dose"),
                "Venom dose adds poison stacks.",
                &["venom dose"],
            ),
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
            anchor(
                "frost-region",
                &format!("Aster://{world_id}/world/frost#region"),
                "Frost Reach is a glacier hollow.",
                &["Frost Reach", "glacier hollow"],
            ),
            anchor(
                "frost-faction",
                &format!("Aster://{world_id}/factions/snow-court"),
                "Snow Court shapes a frost veil.",
                &["Snow Court", "frost veil"],
            ),
            anchor(
                "frost-reagent",
                &format!("Aster://{world_id}/reagents/moon-shard"),
                "Moon shard bends cold light.",
                &["moon shard"],
            ),
        ],
    }
}

fn poison_assay() -> AssayBalanceProfile {
    AssayBalanceProfile {
        assay_id: "assay-1231".to_string(),
        profile_id: "ability-tier-1-control".to_string(),
        min_damage: 18,
        max_damage: 54,
        min_cooldown_seconds: 6,
        max_cooldown_seconds: 16,
        max_mana_cost: 70,
        allowed_damage_types: vec!["poison stacking".to_string(), "venom dose".to_string()],
    }
}

fn ward() -> WardPolicy {
    WardPolicy {
        policy_id: "ward-grounding-issue1231".to_string(),
        min_source_anchors: 3,
        min_grounded_terms: 4,
        blocked_terms: vec![
            "spaceship".to_string(),
            "laser".to_string(),
            "stock market".to_string(),
        ],
    }
}

fn anchor(id: &str, source: &str, text: &str, terms: &[&str]) -> GroundingAnchor {
    GroundingAnchor {
        anchor_id: id.to_string(),
        source_ref: source.to_string(),
        text: text.to_string(),
        terms: terms.iter().map(|term| term.to_string()).collect(),
    }
}
