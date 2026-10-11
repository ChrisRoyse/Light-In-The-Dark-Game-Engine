use super::{
    WitheringPhaseRuntimeSpec, WitheringRaidPathAdmission, WitheringRaidPathRuntime,
    WitheringRaidRuntimeSpec, admit_withering_raid_from_path,
};

fn path(min_party: i64, min_level: i64) -> WitheringRaidPathRuntime {
    WitheringRaidPathRuntime {
        raid: WitheringRaidRuntimeSpec {
            raid_id: "raid.withering.black_reliquary".into(),
            minimum_party_size: min_party,
            minimum_character_level: min_level,
            pressure_tier_milli: 900,
            final_guard_agent_id: "agent.infernal.wailing_tombs.choirmaster".into(),
            target_item_of_power_id: "item.hollow_pact.rift_shard".into(),
            requires_faction_leader_order: true,
        },
        phases: vec![WitheringPhaseRuntimeSpec {
            phase_id: "phase.withering.cipher_trail".into(),
            raid_id: "raid.withering.black_reliquary".into(),
            kind: "cipher".into(),
            pressure_tier_milli: 900,
        }],
    }
}

#[test]
fn admits_at_floors_and_denies_below() {
    let runtime = path(8, 47);
    assert_eq!(
        admit_withering_raid_from_path(&runtime, 8, 47),
        WitheringRaidPathAdmission::Admitted
    );
    assert!(matches!(
        admit_withering_raid_from_path(&runtime, 7, 47),
        WitheringRaidPathAdmission::Denied { .. }
    ));
    assert!(matches!(
        admit_withering_raid_from_path(&runtime, 8, 46),
        WitheringRaidPathAdmission::Denied { .. }
    ));
}

#[test]
fn boundary_move_denies_previous_party() {
    let runtime = path(8, 47);
    assert_eq!(
        admit_withering_raid_from_path(&runtime, 8, 47),
        WitheringRaidPathAdmission::Admitted
    );
    let raised = path(10, 47);
    assert!(matches!(
        admit_withering_raid_from_path(&raised, 8, 47),
        WitheringRaidPathAdmission::Denied { .. }
    ));
}
