use litd_engine::math::Transform;

use super::*;
use crate::{
    authoring::{
        AssayBalanceProfile, GroundedAbilityRequest, GroundingAnchor, LoweredAbilityDocument,
        WardPolicy, WorldCorpus, generate_grounded_ability, verify_lowered_ability_toml,
    },
    history::{EditorObject, stable_fingerprint},
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
        create_editor_world_record,
    },
};

#[test]
fn flags_near_duplicate_abilities_and_leaves_distinct_content_unflagged() {
    let first = lowered_ability("req-a", 424242, "wa1-ashen-reach");
    let near = lowered_ability("req-b", 424243, "wa1-ashen-reach");
    let distinct = lowered_frost_ability();
    let records = vec![
        CatalogDedupRecord::from_lowered_ability(&first).unwrap(),
        CatalogDedupRecord::from_lowered_ability(&near).unwrap(),
        CatalogDedupRecord::from_lowered_ability(&distinct).unwrap(),
    ];

    let report = build_catalog_dedup_report(&records, &DedupPolicy::default()).unwrap();

    assert_eq!(report.record_count, 3);
    assert_eq!(report.candidate_count, 1);
    let candidate = &report.candidates[0];
    assert_eq!(candidate.decision, DedupDecisionKind::NearDuplicate);
    assert_eq!(candidate.kind, CatalogEntityKind::Ability);
    assert!(candidate.minimum_score_milli >= DedupPolicy::default().tau_milli);
    assert!(candidate.anchor_conflicts.is_empty());
    println!(
        "issue1232 happy before=3 ability_records after=candidates:{} decision:{:?} min_score:{} suggestion={}",
        report.candidate_count,
        candidate.decision,
        candidate.minimum_score_milli,
        candidate.merge_suggestion
    );
}

#[test]
fn exact_duplicate_is_separate_from_semantic_near_duplicate() {
    let ability = lowered_ability("req-exact", 424242, "wa1-ashen-reach");
    let left = CatalogDedupRecord::from_lowered_ability(&ability).unwrap();
    let mut right = left.clone();
    right.record_id = format!("{}#copy", right.record_id);
    right.anchor_ids.push("manual-copy-anchor".to_string());

    let report =
        build_catalog_dedup_report(&[left.clone(), right], &DedupPolicy::default()).unwrap();

    assert_eq!(report.candidate_count, 1);
    assert_eq!(
        report.candidates[0].decision,
        DedupDecisionKind::ExactDuplicate
    );
    assert!(report.candidates[0].exact_fingerprint);
    assert_eq!(
        report.candidates[0].slot_scores[&DedupSlotKind::Identity],
        1000
    );
    println!(
        "issue1232 edge=exact before=fingerprint:{} after=decision:{:?}",
        left.stable_fingerprint, report.candidates[0].decision
    );
}

#[test]
fn threshold_boundary_is_inclusive_for_every_required_slot() {
    let left = boundary_item("item.boundary.left", "alpha beta gamma");
    let right = boundary_item("item.boundary.right", "alpha beta delta");
    let policy = DedupPolicy {
        tau_milli: 667,
        ..DedupPolicy::default()
    };

    let report = build_catalog_dedup_report(&[left, right], &policy).unwrap();

    assert_eq!(report.candidate_count, 1);
    assert_eq!(
        report.candidates[0].decision,
        DedupDecisionKind::NearDuplicate
    );
    assert_eq!(report.candidates[0].minimum_score_milli, 667);
    println!(
        "issue1232 edge=threshold before=tau:667 after=min_score:{} decision:{:?}",
        report.candidates[0].minimum_score_milli, report.candidates[0].decision
    );
}

#[test]
fn cross_world_duplicate_is_flagged_but_not_auto_merged() {
    let first = lowered_ability("req-cross-a", 424242, "wa1-ashen-reach");
    let second = lowered_ability("req-cross-b", 424242, "wa2-ashen-reach");
    let records = vec![
        CatalogDedupRecord::from_lowered_ability(&first).unwrap(),
        CatalogDedupRecord::from_lowered_ability(&second).unwrap(),
    ];

    let report = build_catalog_dedup_report(&records, &DedupPolicy::default()).unwrap();

    assert_eq!(report.candidate_count, 1);
    let candidate = &report.candidates[0];
    assert_eq!(candidate.decision, DedupDecisionKind::AnchorConflict);
    assert!(
        candidate
            .anchor_conflicts
            .iter()
            .any(|item| item.starts_with("world_id:"))
    );
    assert!(candidate.merge_suggestion.contains("do not auto-merge"));
    println!(
        "issue1232 edge=cross_world before=worlds:{}|{} after=decision:{:?} conflicts={}",
        records[0].world_id,
        records[1].world_id,
        candidate.decision,
        candidate.anchor_conflicts.join("|")
    );
}

#[test]
fn units_items_and_worlds_use_real_editor_records() {
    let unit_a = unit_record("unit_knight_a", "Ashen Knight", "tank guard");
    let unit_b = unit_record("unit_knight_b", "Ashen Knight", "tank guard");
    let item_a = item_record("item_cinder_key_a", "Cinder Key", "opens ember gates");
    let item_b = item_record("item_cinder_key_b", "Cinder Key", "opens ember gates");
    let world_a = world_record("calyx://world/dedup-a", "Ashen Boundary");
    let world_b = world_record("calyx://world/dedup-b", "Ashen Boundary");
    let records = vec![
        CatalogDedupRecord::from_editor_object("wa1", &unit_a).unwrap(),
        CatalogDedupRecord::from_editor_object("wa1", &unit_b).unwrap(),
        CatalogDedupRecord::from_editor_object("wa1", &item_a).unwrap(),
        CatalogDedupRecord::from_editor_object("wa1", &item_b).unwrap(),
        CatalogDedupRecord::from_world_record(&world_a).unwrap(),
        CatalogDedupRecord::from_world_record(&world_b).unwrap(),
    ];

    let report = build_catalog_dedup_report(&records, &DedupPolicy::default()).unwrap();

    assert!(
        report
            .candidates
            .iter()
            .any(|c| c.kind == CatalogEntityKind::Unit)
    );
    assert!(
        report
            .candidates
            .iter()
            .any(|c| c.kind == CatalogEntityKind::Item)
    );
    assert!(
        report
            .candidates
            .iter()
            .any(|c| c.kind == CatalogEntityKind::World)
    );
}

#[test]
fn invalid_inputs_fail_loud_without_partial_report() {
    let error = build_catalog_dedup_report(&[], &DedupPolicy::default()).unwrap_err();
    assert!(error.to_string().contains("editor.authoring_dedup.records"));

    let ability = lowered_ability("req-invalid", 424242, "wa1-ashen-reach");
    let record = CatalogDedupRecord::from_lowered_ability(&ability).unwrap();
    let duplicate_error =
        build_catalog_dedup_report(&[record.clone(), record], &DedupPolicy::default()).unwrap_err();
    assert!(
        duplicate_error
            .to_string()
            .contains("editor.authoring_dedup.records.duplicate")
    );

    let bad_policy = DedupPolicy {
        required_slots: vec![DedupSlotKind::Identity, DedupSlotKind::Identity],
        tau_milli: 850,
    };
    let policy_error = build_catalog_dedup_report(
        &[CatalogDedupRecord::from_lowered_ability(&ability).unwrap()],
        &bad_policy,
    )
    .unwrap_err();
    assert!(
        policy_error
            .to_string()
            .contains("editor.authoring_dedup.policy.required_slots")
    );
}

fn lowered_ability(request_id: &str, seed: u64, world_id: &str) -> LoweredAbilityDocument {
    let corpus = corpus(world_id);
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: request_id.to_string(),
            prompt:
                "Create a Dawnward cinder bloom ward flame ability for Ashen Reach dusk forest."
                    .to_string(),
            seed,
            requested_by: "issue1232-test".to_string(),
        },
        &corpus,
        &assay(),
        &ward(),
    )
    .unwrap();
    let lowered = outcome.lowered.unwrap();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap()
}

fn lowered_frost_ability() -> LoweredAbilityDocument {
    let corpus = WorldCorpus {
        world_id: "wa1-frost-vein".to_string(),
        revision_id: "wa1-frost-rev".to_string(),
        region: "Frost Vein".to_string(),
        factions: vec!["Nightglass".to_string()],
        lore_terms: vec![
            "moon shard".to_string(),
            "frost veil".to_string(),
            "glacier hollow".to_string(),
        ],
        anchors: vec![
            anchor(
                "frost-region",
                "Aster://wa1/world/frost#region",
                "Frost Vein is a glacier hollow.",
                &["Frost Vein", "glacier hollow"],
            ),
            anchor(
                "frost-faction",
                "Aster://wa1/factions/nightglass",
                "Nightglass hides behind a frost veil.",
                &["Nightglass", "frost veil"],
            ),
            anchor(
                "frost-reagent",
                "Aster://wa1/reagents/moon-shard",
                "Moon shard bends cold light.",
                &["moon shard"],
            ),
        ],
    };
    let outcome = generate_grounded_ability(
        &GroundedAbilityRequest {
            request_id: "req-frost".to_string(),
            prompt:
                "Create a Nightglass moon shard frost veil ability for Frost Vein glacier hollow."
                    .to_string(),
            seed: 5150,
            requested_by: "issue1232-test".to_string(),
        },
        &corpus,
        &AssayBalanceProfile {
            allowed_damage_types: vec!["frost veil".to_string(), "moon shard".to_string()],
            ..assay()
        },
        &ward(),
    )
    .unwrap();
    let lowered = outcome.lowered.unwrap();
    verify_lowered_ability_toml(&lowered.canonical_toml, &lowered.fingerprint).unwrap()
}

fn boundary_item(id: &str, text: &str) -> CatalogDedupRecord {
    CatalogDedupRecord::from_slot_texts(
        id,
        CatalogEntityKind::Item,
        "wa-boundary",
        vec!["anchor-boundary".to_string()],
        vec!["Aster://boundary/item".to_string()],
        stable_fingerprint(&text).unwrap(),
        [
            (DedupSlotKind::Identity, text.to_string()),
            (DedupSlotKind::Mechanics, text.to_string()),
            (DedupSlotKind::Semantics, text.to_string()),
        ],
    )
    .unwrap()
}

fn unit_record(id: &str, name: &str, role: &str) -> EditorObject {
    EditorObject::new(id, "Unit", None, Transform::IDENTITY)
        .with_property("name", name)
        .with_property("unit_type", "unit.ashen_knight")
        .with_property("stats", "hp 600 armor 4 damage 27")
        .with_property("role", role)
        .with_property("source_ref", "Aster://wa1/units/ashen-knight")
        .with_property("anchor_id", "wa1-unit-ashen-knight")
}

fn item_record(id: &str, name: &str, description: &str) -> EditorObject {
    EditorObject::new(id, "Item", None, Transform::IDENTITY)
        .with_property("name", name)
        .with_property("item_type", "item.cinder_key")
        .with_property("stats", "charges 1 key ember")
        .with_property("description", description)
        .with_property("source_ref", "Aster://wa1/items/cinder-key")
        .with_property("anchor_id", "wa1-item-cinder-key")
}

fn world_record(id: &str, name: &str) -> crate::world_creation::EditorWorldRecord {
    let properties = EditorWorldProperties {
        display_name: name.to_string(),
        description: "Known near-duplicate dedup governance world".to_string(),
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
        calyx_document_id: id.to_string(),
        revision_id: format!("{id}#rev1"),
        properties,
    })
    .unwrap()
}

fn corpus(world_id: &str) -> WorldCorpus {
    WorldCorpus {
        world_id: world_id.to_string(),
        revision_id: format!("{world_id}-rev"),
        region: "Ashen Reach".to_string(),
        factions: vec!["Dawnward".to_string(), "Cinder Choir".to_string()],
        lore_terms: vec![
            "cinder bloom".to_string(),
            "ward flame".to_string(),
            "dusk forest".to_string(),
        ],
        anchors: vec![
            anchor(
                "region",
                &format!("Aster://{world_id}/world/ashen-reach#region"),
                "Ashen Reach is a dusk forest scarred by ember fog.",
                &["Ashen Reach", "dusk forest"],
            ),
            anchor(
                "faction",
                &format!("Aster://{world_id}/factions/dawnward"),
                "The Dawnward bind ward flame into defensive rites.",
                &["Dawnward", "ward flame"],
            ),
            anchor(
                "reagent",
                &format!("Aster://{world_id}/reagents/cinder-bloom"),
                "Cinder bloom stores heat without open flame.",
                &["cinder bloom"],
            ),
        ],
    }
}

fn assay() -> AssayBalanceProfile {
    AssayBalanceProfile {
        assay_id: "assay-724".to_string(),
        profile_id: "ability-tier-1-control".to_string(),
        min_damage: 24,
        max_damage: 68,
        min_cooldown_seconds: 8,
        max_cooldown_seconds: 18,
        max_mana_cost: 60,
        allowed_damage_types: vec!["ward flame".to_string(), "ember".to_string()],
    }
}

fn ward() -> WardPolicy {
    WardPolicy {
        policy_id: "ward-grounding-issue1232".to_string(),
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
