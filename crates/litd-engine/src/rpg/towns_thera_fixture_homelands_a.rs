// #1647 racial homelands, group A: Grinning Skull Village, Goblin Village,
// Goblin Outpost, Ysigrath. CF-factual topology/rosters/level-bands; lore
// originalized (#984).
use litd_engine::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::towns_thera_support::{
    GOBLIN_OUTPOST_AREA_ID, GOBLIN_VILLAGE_AREA_ID, GRINNING_SKULL_VILLAGE_AREA_ID,
    TownExpectation, YSIGRATH_AREA_ID, access_any, access_restricted, area_spec, expect, external,
    hidden_internal, internal, recall, repop, resets, room, visual,
};

pub fn push_specs(specs: &mut Vec<(AreaSpec, TownExpectation)>) {
    specs.push((
        grinning_skull_village_area_spec(),
        expect(
            GRINNING_SKULL_VILLAGE_AREA_ID,
            1647,
            AreaKind::Town,
            true,
            true,
            5,
        ),
    ));
    specs.push((
        goblin_village_area_spec(),
        expect(GOBLIN_VILLAGE_AREA_ID, 1647, AreaKind::Town, true, true, 5),
    ));
    specs.push((
        goblin_outpost_area_spec(),
        expect(
            GOBLIN_OUTPOST_AREA_ID,
            1647,
            AreaKind::Town,
            false,
            false,
            4,
        ),
    ));
    specs.push((
        ysigrath_area_spec(),
        expect(YSIGRATH_AREA_ID, 1647, AreaKind::Wilderness, true, true, 5),
    ));
}

/// Grinning Skull Village: seat of the orc clan (ties races #1000-#1019 and the
/// Grinning Skulls cabal); orc-berserker home-hide terrain (L1).
pub fn grinning_skull_village_area_spec() -> AreaSpec {
    let aid = GRINNING_SKULL_VILLAGE_AREA_ID;
    area_spec(
        aid,
        "Grinning Skull Village",
        AreaKind::Town,
        1647,
        &[985, 1601],
        "grinning-skull-village",
        (1, 20),
        "The stake-and-skull seat of the orc clan, home-hide ground for orc berserkers \
         and the mustering point for the clan's warfare over items of power.",
        vec![
            room(
                "room.grinning_skull_village.war_gate",
                "War Gate",
                RoomKind::Gate,
                "A gate of lashed stakes crowned with grinning skulls.",
                RoomConcealment::GateChokepoint,
                &["npc.grinning_skull_village.war_sentry"],
            ),
            room(
                "room.grinning_skull_village.clan_grounds",
                "Clan Grounds",
                RoomKind::Intersection,
                "Trampled home-hide ground where the clan gathers.",
                RoomConcealment::CrowdCover,
                &["npc.grinning_skull_village.orc_warrior"],
            ),
            room(
                "room.grinning_skull_village.warlord_hut",
                "Warlord Hut",
                RoomKind::Guildhall,
                "The warlord's hut where berserkers take their oaths.",
                RoomConcealment::None,
                &["npc.grinning_skull_village.warlord"],
            ),
            room(
                "room.grinning_skull_village.raid_camp",
                "Raid Camp",
                RoomKind::Wilderness,
                "A hide-terrain camp on the raiding trail.",
                RoomConcealment::AlleyShadow,
                &["npc.grinning_skull_village.raider"],
            ),
            room(
                "room.grinning_skull_village.war_totem",
                "War Totem",
                RoomKind::Temple,
                "The blood-totem that serves as the clan recall point.",
                RoomConcealment::None,
                &["npc.grinning_skull_village.shaman"],
            ),
        ],
        vec![
            internal(
                "conn.grinning_skull_village.gate_to_grounds",
                "room.grinning_skull_village.war_gate",
                aid,
                "room.grinning_skull_village.clan_grounds",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.grinning_skull_village.grounds_to_hut",
                "room.grinning_skull_village.clan_grounds",
                aid,
                "room.grinning_skull_village.warlord_hut",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.grinning_skull_village.grounds_to_totem",
                "room.grinning_skull_village.clan_grounds",
                aid,
                "room.grinning_skull_village.war_totem",
                ConnectionDirection::East,
                TravelKind::Path,
            ),
            hidden_internal(
                "conn.grinning_skull_village.raid_trail",
                "room.grinning_skull_village.clan_grounds",
                aid,
                "room.grinning_skull_village.raid_camp",
                ConnectionDirection::West,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.grinning_skull_village.to_galadon",
                "room.grinning_skull_village.war_gate",
                GALADON_AREA_ID,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.grinning_skull_village.cycle", 25),
        repop("repop.grinning_skull_village.pop", 24, 12, 3),
        recall(
            true,
            Some("room.grinning_skull_village.war_totem"),
            false,
            access_restricted(&["race.orc"]),
        ),
        visual(
            "tileset.town.grinning_skull_village.stake_hide",
            "model.town.grinning_skull_village.war_camp",
            &[
                "prop.town.skull_stake_gate",
                "prop.town.warlord_hut",
                "prop.town.blood_totem",
            ],
            "stake-and-skull orc war-village of hide huts around a blood totem",
            "Original orc war-village props and totem dressing; no copied iconography.",
        ),
        &[
            "Orc hometown (access race.orc); orc-berserker home-hide terrain (#1601/#1647).",
            "Clan-warfare hooks over items of power; road runs south toward Galadon (#1632).",
        ],
    )
}

/// Goblin Village: goblin homeland south of Galadon (2-10); resolves Galadon's
/// concealed forward-stub edge. Goblin-raider hide terrains.
pub fn goblin_village_area_spec() -> AreaSpec {
    let aid = GOBLIN_VILLAGE_AREA_ID;
    area_spec(
        aid,
        "Goblin Village",
        AreaKind::Town,
        1647,
        &[985],
        "goblin-village",
        (2, 10),
        "A reeking warren of the goblin raiders south of Galadon past the Rocky Cliffs, \
         its hide-terrain burrows the homeland of the goblin folk.",
        vec![
            room(
                "room.goblin_village.thorn_gate",
                "Thorn Gate",
                RoomKind::Gate,
                "A gap in the thorn wall guarded by goblin pickets.",
                RoomConcealment::GateChokepoint,
                &["npc.goblin_village.picket"],
            ),
            room(
                "room.goblin_village.warren",
                "The Warren",
                RoomKind::Intersection,
                "A muddy warren of burrows and cook-fires (hide terrain).",
                RoomConcealment::CrowdCover,
                &["npc.goblin_village.goblin"],
            ),
            room(
                "room.goblin_village.chief_hut",
                "Chief's Hut",
                RoomKind::Guildhall,
                "The goblin chief's stinking hut of trophies.",
                RoomConcealment::None,
                &["npc.goblin_village.chief"],
            ),
            room(
                "room.goblin_village.raider_hide",
                "Raider Hide",
                RoomKind::Wilderness,
                "A concealed hide on the raiding trail north.",
                RoomConcealment::AlleyShadow,
                &["npc.goblin_village.raider"],
            ),
            room(
                "room.goblin_village.spirit_stone",
                "Spirit Stone",
                RoomKind::Temple,
                "A daubed standing stone that serves as the recall point.",
                RoomConcealment::None,
                &["npc.goblin_village.witch_doctor"],
            ),
        ],
        vec![
            internal(
                "conn.goblin_village.gate_to_warren",
                "room.goblin_village.thorn_gate",
                aid,
                "room.goblin_village.warren",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.goblin_village.warren_to_hut",
                "room.goblin_village.warren",
                aid,
                "room.goblin_village.chief_hut",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            internal(
                "conn.goblin_village.warren_to_stone",
                "room.goblin_village.warren",
                aid,
                "room.goblin_village.spirit_stone",
                ConnectionDirection::East,
                TravelKind::Path,
            ),
            hidden_internal(
                "conn.goblin_village.hide_trail",
                "room.goblin_village.warren",
                aid,
                "room.goblin_village.raider_hide",
                ConnectionDirection::West,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.goblin_village.to_galadon",
                "room.goblin_village.thorn_gate",
                GALADON_AREA_ID,
                ConnectionDirection::Northwest,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
        ],
        resets("reset.goblin_village.cycle", 20),
        repop("repop.goblin_village.pop", 26, 12, 2),
        recall(
            true,
            Some("room.goblin_village.spirit_stone"),
            false,
            access_restricted(&["race.goblin"]),
        ),
        visual(
            "tileset.town.goblin_village.thorn_warren",
            "model.town.goblin_village.warren",
            &[
                "prop.town.thorn_gate",
                "prop.town.burrow_mound",
                "prop.town.spirit_stone",
            ],
            "thorn-walled goblin warren of muddy burrows around a daubed spirit stone",
            "Original goblin-warren props and spirit-stone dressing; no copied iconography.",
        ),
        &[
            "Reuses area.thera.galadon.goblin_village to resolve Galadon's concealed forward-stub edge (#1636).",
            "Goblin hometown (access race.goblin); raider hide terrains (#1601/#1647).",
        ],
    )
}

/// Goblin Outpost: a goblin-raider camp on the far northern road.
pub fn goblin_outpost_area_spec() -> AreaSpec {
    let aid = GOBLIN_OUTPOST_AREA_ID;
    area_spec(
        aid,
        "Goblin Outpost",
        AreaKind::Town,
        1647,
        &[985],
        "goblin-outpost",
        (2, 12),
        "A staked raider outpost on the far northern road, a forward hide from which \
         goblin bands harry the trade to Udgaard.",
        vec![
            room(
                "room.goblin_outpost.road_stakes",
                "Road Stakes",
                RoomKind::Gate,
                "A line of sharpened stakes across the northern road.",
                RoomConcealment::GateChokepoint,
                &["npc.goblin_outpost.picket"],
            ),
            room(
                "room.goblin_outpost.watch_fire",
                "Watch Fire",
                RoomKind::Intersection,
                "A smoking watch-fire ringed by lean-tos.",
                RoomConcealment::CrowdCover,
                &["npc.goblin_outpost.raider"],
            ),
            room(
                "room.goblin_outpost.raider_lean",
                "Raider Lean-to",
                RoomKind::Landmark,
                "A hide lean-to stacked with stolen road-goods.",
                RoomConcealment::AlleyShadow,
                &["npc.goblin_outpost.bandit"],
            ),
            room(
                "room.goblin_outpost.spoil_pit",
                "Spoil Pit",
                RoomKind::Shop,
                "A pit-market of plundered oddments.",
                RoomConcealment::None,
                &["npc.goblin_outpost.spoil_keeper"],
            ),
        ],
        vec![
            internal(
                "conn.goblin_outpost.stakes_to_fire",
                "room.goblin_outpost.road_stakes",
                aid,
                "room.goblin_outpost.watch_fire",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.goblin_outpost.fire_to_lean",
                "room.goblin_outpost.watch_fire",
                aid,
                "room.goblin_outpost.raider_lean",
                ConnectionDirection::East,
                TravelKind::Path,
            ),
            internal(
                "conn.goblin_outpost.fire_to_pit",
                "room.goblin_outpost.watch_fire",
                aid,
                "room.goblin_outpost.spoil_pit",
                ConnectionDirection::West,
                TravelKind::Path,
            ),
            external(
                "conn.goblin_outpost.to_galadon",
                "room.goblin_outpost.road_stakes",
                GALADON_AREA_ID,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.goblin_outpost.to_village",
                "room.goblin_outpost.road_stakes",
                GOBLIN_VILLAGE_AREA_ID,
                ConnectionDirection::South,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
        ],
        resets("reset.goblin_outpost.cycle", 20),
        repop("repop.goblin_outpost.pop", 18, 12, 2),
        recall(false, None, false, access_any()),
        visual(
            "tileset.town.goblin_outpost.stake_camp",
            "model.town.goblin_outpost.road_camp",
            &[
                "prop.town.road_stakes",
                "prop.town.watch_fire",
                "prop.town.hide_leanto",
            ],
            "staked goblin road-camp of lean-tos around a smoking watch-fire",
            "Original raider road-camp props and lean-to dressing; no copied iconography.",
        ),
        &[
            "Far-north road raider camp; kin trail links to the Goblin Village (#1632).",
            "Goblin-raider hide terrains (#1601/#1647).",
        ],
    )
}

#[path = "towns_thera_fixture_homelands_a_ysigrath.rs"]
mod split;
pub use split::ysigrath_area_spec;
