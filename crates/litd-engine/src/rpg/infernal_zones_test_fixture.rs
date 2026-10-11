use litd_engine::rpg::{
    ASHEN_REACH_INFERNAL_ZONES_POLICY_ID, InfernalCircleId, InfernalHazardKind,
    InfernalRepopProfile, InfernalResetProfile, InfernalZoneExit, InfernalZoneExitKind,
    InfernalZoneNode, InfernalZonePolicy, RPG_INFERNAL_ZONES_SCHEMA,
};

pub(crate) fn ashen_reach_infernal_zone_policy() -> InfernalZonePolicy {
    InfernalZonePolicy {
        schema: RPG_INFERNAL_ZONES_SCHEMA.to_string(),
        server_policy_id: ASHEN_REACH_INFERNAL_ZONES_POLICY_ID.to_string(),
        source_issue: 1067,
        built_in_map_required: false,
        entry_circle: InfernalCircleId::Threshold,
        terminal_circle: InfernalCircleId::ObsidianThrone,
        zones: vec![
            zone(ZoneArgs {
                circle_id: InfernalCircleId::Threshold,
                zone_id: "zone.infernal.threshold.ashgate",
                display_name: "Threshold of Cinders",
                depth: 1,
                hazard_profile_id: "hazard.infernal.threshold.ash_heat",
                hazard_kind: InfernalHazardKind::Heat,
                mob_population_ids: &[
                    "mob.infernal.ash_imp",
                    "mob.infernal.cinder_cultist",
                    "mob.infernal.gatebound_wretch",
                ],
                ruler_ref: "lord.infernal.threshold.ashgate_warden",
                reset_profile: reset("reset.infernal.threshold.ashfall", 45),
                repop_profile: repop("repop.infernal.threshold.gateguard", 18, 12, 1, true),
                exits: vec![exit(
                    "exit.infernal.threshold.to_ember_wastes",
                    InfernalCircleId::EmberWastes,
                    Some("gate.infernal.cindermark_clearance"),
                )],
                withering_reagent_source: false,
                relic_vault_source: false,
            }),
            zone(ZoneArgs {
                circle_id: InfernalCircleId::EmberWastes,
                zone_id: "zone.infernal.ember_wastes.lava_fields",
                display_name: "Ember Wastes",
                depth: 2,
                hazard_profile_id: "hazard.infernal.ember_wastes.lava_surge",
                hazard_kind: InfernalHazardKind::Heat,
                mob_population_ids: &[
                    "mob.infernal.ember_imp",
                    "mob.infernal.lava_behemoth",
                    "mob.infernal.cinder_hound",
                ],
                ruler_ref: "lord.infernal.ember_wastes.cinder_baron",
                reset_profile: reset("reset.infernal.ember_wastes.lava_shift", 50),
                repop_profile: repop("repop.infernal.ember_wastes.hunters", 24, 10, 2, true),
                exits: vec![exit(
                    "exit.infernal.ember_wastes.to_iron_bastion",
                    InfernalCircleId::IronBastion,
                    Some("gate.infernal.ember_chain"),
                )],
                withering_reagent_source: false,
                relic_vault_source: false,
            }),
            zone(ZoneArgs {
                circle_id: InfernalCircleId::IronBastion,
                zone_id: "zone.infernal.iron_bastion.warworks",
                display_name: "Iron Bastion",
                depth: 3,
                hazard_profile_id: "hazard.infernal.iron_bastion.furnace_works",
                hazard_kind: InfernalHazardKind::Heat,
                mob_population_ids: &[
                    "mob.infernal.iron_legionary",
                    "mob.infernal.hell_knight",
                    "mob.infernal.chain_artillerist",
                ],
                ruler_ref: "lord.infernal.iron_bastion.iron_praetor",
                reset_profile: reset("reset.infernal.iron_bastion.siege_cycle", 55),
                repop_profile: repop("repop.infernal.iron_bastion.legions", 30, 9, 3, true),
                exits: vec![exit(
                    "exit.infernal.iron_bastion.to_hungering_dark",
                    InfernalCircleId::HungeringDark,
                    Some("gate.infernal.bastion_understair"),
                )],
                withering_reagent_source: false,
                relic_vault_source: false,
            }),
            zone(ZoneArgs {
                circle_id: InfernalCircleId::HungeringDark,
                zone_id: "zone.infernal.hungering_dark.maw_caves",
                display_name: "Hungering Dark",
                depth: 4,
                hazard_profile_id: "hazard.infernal.hungering_dark.light_starvation",
                hazard_kind: InfernalHazardKind::DarkPressure,
                mob_population_ids: &[
                    "mob.infernal.eyeless_stalker",
                    "mob.infernal.hollow_maw",
                    "mob.infernal.dark_pressure_acolyte",
                ],
                ruler_ref: "lord.infernal.hungering_dark.the_maw",
                reset_profile: reset("reset.infernal.hungering_dark.torch_death", 60),
                repop_profile: repop("repop.infernal.hungering_dark.stalkers", 32, 8, 4, true),
                exits: vec![exit(
                    "exit.infernal.hungering_dark.to_wailing_tombs",
                    InfernalCircleId::WailingTombs,
                    Some("gate.infernal.bone_chorus"),
                )],
                withering_reagent_source: false,
                relic_vault_source: false,
            }),
            zone(ZoneArgs {
                circle_id: InfernalCircleId::WailingTombs,
                zone_id: "zone.infernal.wailing_tombs.bone_choir",
                display_name: "Wailing Tombs",
                depth: 5,
                hazard_profile_id: "hazard.infernal.wailing_tombs.soul_refrain",
                hazard_kind: InfernalHazardKind::SoulDrain,
                mob_population_ids: &[
                    "mob.infernal.wailing_wraith",
                    "mob.infernal.bone_choir",
                    "mob.infernal.gravebound_alchemist",
                ],
                ruler_ref: "lord.infernal.wailing_tombs.choirmaster",
                reset_profile: reset("reset.infernal.wailing_tombs.dirge_cycle", 70),
                repop_profile: repop("repop.infernal.wailing_tombs.dead", 36, 8, 5, true),
                exits: vec![exit(
                    "exit.infernal.wailing_tombs.to_molten_throne_road",
                    InfernalCircleId::MoltenThroneRoad,
                    Some("gate.infernal.dirge_sigil"),
                )],
                withering_reagent_source: true,
                relic_vault_source: false,
            }),
            zone(ZoneArgs {
                circle_id: InfernalCircleId::MoltenThroneRoad,
                zone_id: "zone.infernal.molten_throne_road.relic_vault",
                display_name: "Molten Throne-road",
                depth: 6,
                hazard_profile_id: "hazard.infernal.molten_throne_road.crown_lava",
                hazard_kind: InfernalHazardKind::Heat,
                mob_population_ids: &[
                    "mob.infernal.throne_road_jailer",
                    "mob.infernal.chained_sentinel",
                    "mob.infernal.relic_vault_keeper",
                ],
                ruler_ref: "lord.infernal.molten_throne_road.throne_warden",
                reset_profile: reset("reset.infernal.molten_throne_road.vault_lock", 80),
                repop_profile: repop("repop.infernal.molten_throne_road.vanguard", 40, 7, 6, true),
                exits: vec![exit(
                    "exit.infernal.molten_throne_road.to_obsidian_throne",
                    InfernalCircleId::ObsidianThrone,
                    Some("gate.infernal.obsidian_crown"),
                )],
                withering_reagent_source: false,
                relic_vault_source: true,
            }),
            zone(ZoneArgs {
                circle_id: InfernalCircleId::ObsidianThrone,
                zone_id: "zone.infernal.obsidian_throne.crown_chamber",
                display_name: "Obsidian Throne",
                depth: 7,
                hazard_profile_id: "hazard.infernal.obsidian_throne.name_burn",
                hazard_kind: InfernalHazardKind::SoulDrain,
                mob_population_ids: &[
                    "mob.infernal.archdevil_guard",
                    "mob.infernal.obsidian_seraph",
                    "mob.infernal.name_eater",
                ],
                ruler_ref: "lord.infernal.obsidian_throne.maldrithax",
                reset_profile: reset("reset.infernal.obsidian_throne.audience", 120),
                repop_profile: repop("repop.infernal.obsidian_throne.guard", 28, 15, 7, false),
                exits: Vec::new(),
                withering_reagent_source: false,
                relic_vault_source: false,
            }),
        ],
    }
}

struct ZoneArgs<'a> {
    circle_id: InfernalCircleId,
    zone_id: &'a str,
    display_name: &'a str,
    depth: u8,
    hazard_profile_id: &'a str,
    hazard_kind: InfernalHazardKind,
    mob_population_ids: &'a [&'a str],
    ruler_ref: &'a str,
    reset_profile: InfernalResetProfile,
    repop_profile: InfernalRepopProfile,
    exits: Vec<InfernalZoneExit>,
    withering_reagent_source: bool,
    relic_vault_source: bool,
}

fn zone(args: ZoneArgs<'_>) -> InfernalZoneNode {
    let ZoneArgs {
        circle_id,
        zone_id,
        display_name,
        depth,
        hazard_profile_id,
        hazard_kind,
        mob_population_ids,
        ruler_ref,
        reset_profile,
        repop_profile,
        exits,
        withering_reagent_source,
        relic_vault_source,
    } = args;
    InfernalZoneNode {
        circle_id,
        zone_id: zone_id.to_string(),
        display_name: display_name.to_string(),
        depth,
        hazard_profile_id: hazard_profile_id.to_string(),
        hazard_kind,
        mob_population_ids: mob_population_ids.iter().map(|id| id.to_string()).collect(),
        ruler_ref: ruler_ref.to_string(),
        reset_profile,
        repop_profile,
        exits,
        withering_reagent_source,
        relic_vault_source,
    }
}

fn reset(reset_id: &str, interval_minutes: u16) -> InfernalResetProfile {
    InfernalResetProfile {
        reset_id: reset_id.to_string(),
        interval_minutes,
        clears_unlooted_corpses: true,
        resets_hazard_emitters: true,
    }
}

fn repop(
    repop_id: &str,
    baseline_mob_budget: u16,
    repop_interval_minutes: u16,
    max_pressure_tier: u8,
    ruler_respawns: bool,
) -> InfernalRepopProfile {
    InfernalRepopProfile {
        repop_id: repop_id.to_string(),
        baseline_mob_budget,
        repop_interval_minutes,
        max_pressure_tier,
        ruler_respawns,
    }
}

fn exit(
    exit_id: &str,
    to_circle_id: InfernalCircleId,
    locked_by_ref: Option<&str>,
) -> InfernalZoneExit {
    InfernalZoneExit {
        exit_id: exit_id.to_string(),
        kind: InfernalZoneExitKind::Descend,
        to_circle_id,
        locked_by_ref: locked_by_ref.map(str::to_string),
    }
}
