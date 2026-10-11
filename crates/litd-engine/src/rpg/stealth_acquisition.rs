//! The authored stealth acquisition matrix (#1623) and its entry builders.
//!
//! Levels are CF-factual, transcribed from the helpfiles cited in issue #1623.
//! Every `ability_id` is checked against the #1604 stealth catalog and every
//! `grantor_id` against the authored class/race spec rosters (validation lives in
//! `stealth_acquisition_validate`). The five abilities the matrix references that
//! the #1604 catalog originally omitted -- gentle walk, heightened awareness, dark
//! vision, mass invis, vanish -- are now authored in the catalog (#1732), so their
//! rows are real validated cells rather than omissions.

use std::collections::BTreeSet;

use super::stealth_acquisition_types::{
    AcquisitionKind, GrantorKind, RPG_STEALTH_ACQUISITION_SCHEMA, STEALTH_ACQUISITION_TABLE_ID,
    StealthAcquisitionEntry, StealthAcquisitionMatrix,
};

// Class spec ids (#1020-#1036), originalized per #984. These are the exact id
// strings the authored ClassSpec fixtures use; the matrix references them so its
// grantor ids resolve against real specs.
pub(super) const THIEF: &str = "class.galdmere_shade";
pub(super) const ASSASSIN: &str = "class.hushblade";
const BARD: &str = "class.wandersong";
pub(super) const RANGER: &str = "class.thornwarden";
pub(super) const DRUID: &str = "class.wildbinder";
pub(super) const TRANSMUTER: &str = "class.sinewright";
pub(super) const INVOKER: &str = "class.currentbraider";
const SHAPESHIFTER: &str = "class.shapeshifter";
const WARRIOR: &str = "class.ashen_warrior";
const BERSERKER: &str = "class.reach_ravager";
const RAIDER: &str = "class.reach_whelp_raider";
const CONJURER: &str = "class.circlebinder";
const NECROMANCER: &str = "class.gravebinder";
const SHAMAN: &str = "class.scourge_priest";
const HEALER: &str = "class.lifewarden";
const PALADIN: &str = "class.lampbearer";
const ANTI_PALADIN: &str = "class.blackoath_knight";

const ELF: &str = "race.elf";
pub(super) const DARK_ELF: &str = "race.ashkin_deep";
const HALF_ELF: &str = "race.half_elf";
const HALF_DROW: &str = "race.ashkin_mixed";
const DUERGAR: &str = "race.onyx_kin";
const SVIRFNEBLIN: &str = "race.deep_kin";

/// The 17 authored class spec ids the matrix validates grantor class ids against.
pub fn known_class_ids() -> BTreeSet<String> {
    [
        WARRIOR,
        BERSERKER,
        RAIDER,
        THIEF,
        ASSASSIN,
        BARD,
        RANGER,
        DRUID,
        SHAMAN,
        HEALER,
        PALADIN,
        ANTI_PALADIN,
        CONJURER,
        INVOKER,
        NECROMANCER,
        TRANSMUTER,
        SHAPESHIFTER,
    ]
    .iter()
    .map(|id| (*id).to_string())
    .collect()
}

/// The full authored stealth acquisition matrix.
pub fn stealth_acquisition_matrix() -> StealthAcquisitionMatrix {
    StealthAcquisitionMatrix {
        schema: RPG_STEALTH_ACQUISITION_SCHEMA.to_string(),
        table_id: STEALTH_ACQUISITION_TABLE_ID.to_string(),
        entries: vec![
            // --- Sneak (helpfile 1169) ---
            trained("ability.sneak", THIEF, 4, "helpfile 1169 (thief L4)"),
            trained("ability.sneak", ASSASSIN, 4, "helpfile 1169 (assassin L4)"),
            trained("ability.sneak", BARD, 30, "helpfile 1169 (bard L30)"),
            innate("ability.sneak", ELF, "elf light-of-step (perma-sneak)"),
            innate(
                "ability.sneak",
                DARK_ELF,
                "dark-elf walks unnoticed (perma-sneak)",
            ),
            // --- Hide (helpfile 503) ---
            hide_terrain(
                "ability.hide",
                THIEF,
                1,
                "civilized",
                "helpfile 503 (thief L1)",
            ),
            hide_terrain(
                "ability.hide",
                ASSASSIN,
                4,
                "civilized+woodland",
                "helpfile 503 (assassin L4)",
            ),
            // --- Thief concealment path (point costs from #1623) ---
            thief_path(
                "ability.improved_hide",
                27,
                4,
                "helpfile 1421 (thief L27, 4pts)",
            ),
            thief_path("ability.covert", 16, 2, "helpfile 2185 (thief L16, 2pts)"),
            thief_path(
                "ability.shadow_disappear",
                37,
                2,
                "helpfile 1425 (thief L37, 2pts)",
            ),
            // --- Quiet movement (helpfile 483) ---
            trained(
                "ability.quiet_movement",
                RANGER,
                5,
                "helpfile 483 (ranger L5)",
            ),
            trained(
                "ability.quiet_movement",
                DRUID,
                13,
                "helpfile 483 (druid L13)",
            ),
            innate(
                "ability.quiet_movement",
                HALF_ELF,
                "half-elf natural quiet movement",
            ),
            innate(
                "ability.quiet_movement",
                HALF_DROW,
                "half-drow natural quiet movement",
            ),
            // --- Camouflage / creep / ambush / waylay / track (helpfiles 467/470/1591/478) ---
            trained(
                "ability.camouflage",
                RANGER,
                4,
                "helpfile 467 (ranger L4; wood-elf affinity)",
            ),
            trained("ability.creep", RANGER, 19, "helpfile 470 (ranger L19)"),
            trained("ability.ambush", RANGER, 12, "helpfile 467 (ranger L12)"),
            trained("ability.waylay", RANGER, 32, "helpfile 1591 (ranger L32)"),
            trained("ability.track", RANGER, 30, "helpfile 478 (ranger L30)"),
            // --- Detection suite (helpfiles 489/467/384) ---
            trained(
                "ability.detect_hidden",
                THIEF,
                10,
                "helpfile 489 (thief L10)",
            ),
            trained(
                "ability.detect_hidden",
                ASSASSIN,
                10,
                "helpfile 489 (assassin L10)",
            ),
            innate(
                "ability.detect_hidden",
                DUERGAR,
                "duergar racial detect hidden",
            ),
            trained(
                "ability.acute_vision",
                RANGER,
                7,
                "helpfile 467 (ranger L7)",
            ),
            cast_on_others(
                "ability.detect_invisibility",
                TRANSMUTER,
                8,
                "helpfile 384 (transmuter, castable on others)",
            ),
            trained(
                "ability.detect_invisibility",
                DRUID,
                5,
                "helpfile 384 (druid L5)",
            ),
            innate(
                "ability.detect_invisibility",
                SVIRFNEBLIN,
                "svirfneblin passive detect invisibility",
            ),
            // --- Arcane concealment (helpfiles 433/719/391/438) ---
            trained(
                "ability.invisibility",
                TRANSMUTER,
                4,
                "helpfile 433 (transmuter L4)",
            ),
            path_only(
                "ability.improved_invisibility",
                INVOKER,
                18,
                "path.air",
                "helpfile 719 (invoker air path)",
            ),
            cast_on_others(
                "ability.pass_door",
                TRANSMUTER,
                24,
                "helpfile 438 (transmuter L24, castable on others)",
            ),
            trained("ability.pass_door", DRUID, 26, "helpfile 438 (druid L26)"),
            trained(
                "ability.control_translucence",
                TRANSMUTER,
                36,
                "control translucence (transmuter L36)",
            ),
            trained(
                "ability.control_translucence",
                SHAPESHIFTER,
                36,
                "control translucence (shapeshifter L36)",
            ),
            trained(
                "ability.duo_dimension",
                TRANSMUTER,
                36,
                "helpfile 391 (transmuter L36)",
            ),
            // --- Reveal / lockout (helpfiles 400/401) ---
            trained(
                "ability.faerie_fog",
                TRANSMUTER,
                13,
                "helpfile 401 (transmuter L13)",
            ),
            trained("ability.faerie_fog", DRUID, 19, "helpfile 401 (druid L19)"),
            innate(
                "ability.faerie_fire",
                DARK_ELF,
                "helpfile 400 (dark-elf innate)",
            ),
            // --- Druid wild concealment (helpfiles 610/621) ---
            trained("ability.treeform", DRUID, 20, "helpfile 610 (druid L20)"),
            trained(
                "ability.forest_haven",
                DRUID,
                18,
                "helpfile 621 (druid L18)",
            ),
            // --- Assassin verbs (helpfiles 744/469) ---
            trained("ability.stalk", ASSASSIN, 35, "helpfile 744 (assassin L35)"),
            trained(
                "ability.assassinate",
                ASSASSIN,
                35,
                "helpfile 469 (assassin L35)",
            ),
            // --- Thief knockout opener (helpfile 482) ---
            trained("ability.blackjack", THIEF, 17, "helpfile 482 (thief L17)"),
            // --- #1732 catalog-gap abilities now authored in the #1604 catalog ---
            thief_path(
                "ability.gentle_walk",
                13,
                3,
                "helpfile 1569 (thief path L13, 3pts)",
            ),
            trained(
                "ability.heightened_awareness",
                ASSASSIN,
                7,
                "helpfile 730 (assassin L7)",
            ),
            trained(
                "ability.dark_vision",
                ASSASSIN,
                5,
                "helpfile 622 (assassin L5)",
            ),
            trained("ability.dark_vision", RANGER, 7, "helpfile 622 (ranger L7)"),
            trained(
                "ability.mass_invis",
                TRANSMUTER,
                11,
                "helpfile 433 (transmuter L11)",
            ),
            trained(
                "ability.vanish",
                ASSASSIN,
                16,
                "helpfile 521 (assassin L16)",
            ),
        ],
    }
}

// --- Entry builders ---------------------------------------------------------

fn entry(
    ability_id: &str,
    grantor_kind: GrantorKind,
    grantor_id: &str,
    level: u16,
    acquisition: AcquisitionKind,
    source_note: &str,
) -> StealthAcquisitionEntry {
    StealthAcquisitionEntry {
        ability_id: ability_id.to_string(),
        grantor_kind,
        grantor_id: grantor_id.to_string(),
        level,
        acquisition,
        path_id: None,
        point_cost: None,
        terrain_profile: None,
        provisional: false,
        source_note: source_note.to_string(),
    }
}

fn trained(ability_id: &str, class_id: &str, level: u16, note: &str) -> StealthAcquisitionEntry {
    entry(
        ability_id,
        GrantorKind::Class,
        class_id,
        level,
        AcquisitionKind::Trained,
        note,
    )
}

fn innate(ability_id: &str, race_id: &str, note: &str) -> StealthAcquisitionEntry {
    entry(
        ability_id,
        GrantorKind::Race,
        race_id,
        1,
        AcquisitionKind::Innate,
        note,
    )
}

fn cast_on_others(
    ability_id: &str,
    class_id: &str,
    level: u16,
    note: &str,
) -> StealthAcquisitionEntry {
    entry(
        ability_id,
        GrantorKind::Class,
        class_id,
        level,
        AcquisitionKind::CastableOnOthers,
        note,
    )
}

fn path_only(
    ability_id: &str,
    class_id: &str,
    level: u16,
    path_id: &str,
    note: &str,
) -> StealthAcquisitionEntry {
    let mut e = entry(
        ability_id,
        GrantorKind::Class,
        class_id,
        level,
        AcquisitionKind::PathOnly,
        note,
    );
    e.path_id = Some(path_id.to_string());
    e
}

fn thief_path(ability_id: &str, level: u16, points: u16, note: &str) -> StealthAcquisitionEntry {
    let mut e = trained(ability_id, THIEF, level, note);
    e.point_cost = Some(points);
    e.path_id = Some("path.concealment".to_string());
    e
}

fn hide_terrain(
    ability_id: &str,
    class_id: &str,
    level: u16,
    terrain: &str,
    note: &str,
) -> StealthAcquisitionEntry {
    let mut e = trained(ability_id, class_id, level, note);
    e.terrain_profile = Some(terrain.to_string());
    e
}
