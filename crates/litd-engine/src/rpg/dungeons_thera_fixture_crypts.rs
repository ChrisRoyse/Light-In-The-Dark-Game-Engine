// #1657 crypts & undead sites (parent #1598, world-graph nodes #1632, framework #987).
// Level bands and denizen rosters are factual from the CF area registry
// (https://wiki.carrionfields.net/game/areas) and the #1657 issue body; only
// name/lore is originalized per #984. Each is a Dungeon: entrance -> descending
// corridors -> inner chambers -> boss vault, with a locked reliquary and a hidden
// secret passage (#944). Undead here ignore visual stealth but hear (sound-only
// perception, #1600) amid crypt-echo acoustics (#1628).

use crate::rpg::{
    AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment, RoomKind,
    TravelKind,
};

use super::rpg_dungeons_thera_support::{
    DungeonSpecInput, GRAVEYARD_AREA_ID, MAUSOLEUM_AREA_ID, VORALIAN_CORRIDOR_AREA_ID,
    dungeon_spec, external, hidden_internal, internal, locked_internal, room,
};

// #1657 Graveyard (3-10 CF; the shared newbie undead ground next to Galadon, #1636).
// area_id reuses GRAVEYARD_AREA_ID so Galadon's `conn.galadon.to_graveyard` resolves.
pub(crate) fn graveyard_area_spec() -> AreaSpec {
    let a = GRAVEYARD_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "Graveyard",
        source_issue: 1657,
        extra_canon: &[1600, 1628, 1636],
        level: (1, 10),
        summary: "The consecrated ground just off Galadon's Holy Road: a walled graveyard whose \
            sunken tombs descend into crypts. Skeletons, graverobbers, zombies, ghouls, and a \
            banshee haunt it for the righteous; mourners, gravediggers, and ghosts for the wicked.",
        slug: "graveyard",
        silhouette: "walled hillside graveyard of leaning headstones over a descending crypt warren",
        extra_props: &["prop.crypt.headstone", "prop.crypt.iron_grave_gate"],
        notes: &[
            "Level band 1-10 and denizen roster are factual (CF area registry + #1657); \
                originalized name/lore only (#984).",
            "Undead ignore visual stealth but hear intruders (sound-only perception, #1600); \
                crypt-echo acoustics per #1628.",
        ],
        rooms: vec![
            room(
                "room.graveyard.gate",
                "Iron Grave Gate",
                RoomKind::Gate,
                "The rusted gate off the Holy Road where gravediggers keep their vigil.",
                RoomConcealment::GateChokepoint,
                &["npc.graveyard.gravedigger"],
            ),
            room(
                "room.graveyard.grounds",
                "Graveyard Grounds",
                RoomKind::Chamber,
                "Leaning headstones and disturbed earth; skeletons and graverobbers wander.",
                RoomConcealment::None,
                &["npc.graveyard.skeleton", "npc.graveyard.graverobber"],
            ),
            room(
                "room.graveyard.crypt_stair",
                "Crypt Stair",
                RoomKind::Chamber,
                "A stair spiralling down into the sunken tombs; echoes carry every footfall.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.graveyard.tombs",
                "Sunken Tombs",
                RoomKind::Chamber,
                "Flooded burial vaults where zombies and ghouls shamble in the dark.",
                RoomConcealment::HiddenLayer,
                &["npc.graveyard.zombie", "npc.graveyard.ghoul"],
            ),
            room(
                "room.graveyard.banshee_vault",
                "Banshee's Vault",
                RoomKind::Chamber,
                "The deepest tomb, where a wailing banshee guards the oldest dead.",
                RoomConcealment::None,
                &["npc.graveyard.banshee"],
            ),
            room(
                "room.graveyard.reliquary",
                "Hidden Reliquary",
                RoomKind::Landmark,
                "A sealed reliquary of grave-goods behind a mourner-warded door.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
        ],
        connections: vec![
            external(
                "conn.graveyard.to_galadon",
                "room.graveyard.gate",
                GALADON_AREA_ID,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            internal(
                "conn.graveyard.enter",
                a,
                "room.graveyard.gate",
                "room.graveyard.grounds",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            internal(
                "conn.graveyard.descend",
                a,
                "room.graveyard.grounds",
                "room.graveyard.crypt_stair",
                ConnectionDirection::Down,
                TravelKind::Descend,
            ),
            internal(
                "conn.graveyard.stair",
                a,
                "room.graveyard.crypt_stair",
                "room.graveyard.tombs",
                ConnectionDirection::Down,
                TravelKind::Stair,
            ),
            internal(
                "conn.graveyard.to_boss",
                a,
                "room.graveyard.tombs",
                "room.graveyard.banshee_vault",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            locked_internal(
                "conn.graveyard.reliquary_door",
                a,
                "room.graveyard.banshee_vault",
                "room.graveyard.reliquary",
                ConnectionDirection::In,
                TravelKind::Stair,
                "lock.graveyard.reliquary",
            ),
            hidden_internal(
                "conn.graveyard.secret",
                a,
                "room.graveyard.crypt_stair",
                "room.graveyard.reliquary",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
    })
}

// #1657 Mausoleum (25-50 CF; juju zombies 30-35 good ranking; west of Voralian, #1632).
pub(crate) fn mausoleum_area_spec() -> AreaSpec {
    let a = MAUSOLEUM_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Mausoleum",
        source_issue: 1657,
        extra_canon: &[1600, 1628],
        level: (25, 50),
        summary: "A vast tiered mausoleum west of the Voralian corridor whose galleries sink into \
            grave-cold vaults. Juju zombies (30-35 for the righteous) and worse things keep the dead.",
        slug: "mausoleum",
        silhouette: "tiered marble mausoleum sinking into black grave-galleries",
        extra_props: &["prop.crypt.sarcophagus", "prop.crypt.bone_reliquary"],
        notes: &[
            "Level band 25-50 and juju-zombie ranking (30-35 good) are factual (CF registry + #1657); \
                originalized name/lore only (#984).",
            "Sound-only undead perception (#1600) and crypt-echo acoustics (#1628) apply.",
        ],
        rooms: vec![
            room(
                "room.mausoleum.portico",
                "Marble Portico",
                RoomKind::Gate,
                "The pillared entrance where grave-cold air spills out of the tomb.",
                RoomConcealment::GateChokepoint,
                &["npc.mausoleum.warden"],
            ),
            room(
                "room.mausoleum.gallery",
                "Gallery of Urns",
                RoomKind::Chamber,
                "Rows of funerary urns line a long gallery descending into the earth.",
                RoomConcealment::None,
                &["npc.mausoleum.juju_zombie"],
            ),
            room(
                "room.mausoleum.stair",
                "Grave Stair",
                RoomKind::Chamber,
                "A broad stair sinks toward the lower vaults; every echo returns twofold.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.mausoleum.vaults",
                "Cold Vaults",
                RoomKind::Chamber,
                "Grave-cold vaults where juju zombies labor over the interred dead.",
                RoomConcealment::HiddenLayer,
                &["npc.mausoleum.juju_zombie", "npc.mausoleum.wraith"],
            ),
            room(
                "room.mausoleum.crypt_lord",
                "Crypt Lord's Sepulchre",
                RoomKind::Chamber,
                "The sepulchre of the mausoleum's undead lord, ringed with grave-lights.",
                RoomConcealment::None,
                &["npc.mausoleum.crypt_lord"],
            ),
            room(
                "room.mausoleum.treasury",
                "Sealed Treasury",
                RoomKind::Landmark,
                "A warded treasury of grave-hoarded wealth behind a bound door.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
        ],
        connections: vec![
            external(
                "conn.mausoleum.to_voralian",
                "room.mausoleum.portico",
                VORALIAN_CORRIDOR_AREA_ID,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            internal(
                "conn.mausoleum.enter",
                a,
                "room.mausoleum.portico",
                "room.mausoleum.gallery",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            internal(
                "conn.mausoleum.descend",
                a,
                "room.mausoleum.gallery",
                "room.mausoleum.stair",
                ConnectionDirection::Down,
                TravelKind::Descend,
            ),
            internal(
                "conn.mausoleum.stair_down",
                a,
                "room.mausoleum.stair",
                "room.mausoleum.vaults",
                ConnectionDirection::Down,
                TravelKind::Stair,
            ),
            internal(
                "conn.mausoleum.to_boss",
                a,
                "room.mausoleum.vaults",
                "room.mausoleum.crypt_lord",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            locked_internal(
                "conn.mausoleum.treasury_door",
                a,
                "room.mausoleum.crypt_lord",
                "room.mausoleum.treasury",
                ConnectionDirection::In,
                TravelKind::Stair,
                "lock.mausoleum.treasury",
            ),
            hidden_internal(
                "conn.mausoleum.secret",
                a,
                "room.mausoleum.stair",
                "room.mausoleum.treasury",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
    })
}

#[path = "dungeons_thera_fixture_crypts/deeper.rs"]
mod deeper;

pub(crate) use deeper::{
    eil_shaeria_area_spec, forgotten_crypts_area_spec, ruins_of_maethien_area_spec,
};
