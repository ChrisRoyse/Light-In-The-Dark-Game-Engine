use super::{
    InfernalRelicRuntimeCatalog, InfernalRelicRuntimeSpec, RelicQualification,
    relic_power_qualifies, relic_world_finite_issue,
};

fn brandblade() -> InfernalRelicRuntimeSpec {
    InfernalRelicRuntimeSpec {
        item_id: "item.infernal.hellforged.brandblade".into(),
        slot: "weapon".into(),
        rarity: "legendary".into(),
        source_vault_id: "vault.infernal.ember_wastes".into(),
        guarded_by_ruler_ref: "agent.infernal.ember_wastes.cinder_baron".into(),
        world_finite_count: 1,
        power_rating_milli: 980,
    }
}

#[test]
fn qualifies_at_floor_and_withholds_below() {
    let relic = brandblade();
    assert_eq!(
        relic_power_qualifies(&relic, 980),
        RelicQualification::Qualifies
    );
    assert_eq!(
        relic_power_qualifies(&relic, 979),
        RelicQualification::Withholds {
            required_power_milli: 980,
            actual_power_milli: 979,
        }
    );
}

#[test]
fn boundary_move_changes_gate() {
    let mut relic = brandblade();
    assert_eq!(
        relic_power_qualifies(&relic, 980),
        RelicQualification::Qualifies
    );
    relic.power_rating_milli = 995;
    assert_eq!(
        relic_power_qualifies(&relic, 980),
        RelicQualification::Withholds {
            required_power_milli: 995,
            actual_power_milli: 980,
        }
    );
}

#[test]
fn world_finite_issue_cap_is_fail_loud() {
    let relic = brandblade();
    relic_world_finite_issue(&relic, 0).expect("first issue ok");
    let err = relic_world_finite_issue(&relic, 1).expect_err("cap");
    let message = format!("{err}");
    assert!(message.contains("world-finite") || message.contains("cap"));
}

#[test]
fn empty_catalog_cannot_be_constructed_from_unit_test_without_frozen_table() {
    // from_frozen empty path is covered by FSV; unit-level sanity on catalog helpers:
    let catalog = InfernalRelicRuntimeCatalog {
        items: vec![brandblade()],
    };
    assert_eq!(catalog.len(), 1);
    assert!(!catalog.is_empty());
    assert!(
        catalog
            .item("item.infernal.hellforged.brandblade")
            .is_some()
    );
    assert!(catalog.item("item.missing").is_none());
}
