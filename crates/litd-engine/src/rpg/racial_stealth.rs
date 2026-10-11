//! The authored 20-race stealth profile table (#1603) plus its readback and
//! referential-integrity validation.
//!
//! Innates are CF-factual, transcribed from the wiki race pages cited per profile
//! (the same URLs the authored `RaceSpec`s carry). Movement-noise classes follow
//! the issue #1603 size->noise mapping where CF gives no explicit noise value.
//! Names/lore are originalized per #984; mechanics stay factual.

use super::race_specs_types::{
    ASHEN_HUMAN_RACE_ID, ASHKIN_DEEP_RACE_ID, ASHKIN_MIXED_RACE_ID, CLOUD_GIANT_RACE_ID,
    DEEP_KIN_RACE_ID, DWARF_RACE_ID, ELF_RACE_ID, FELISAR_RACE_ID, FIRE_GIANT_RACE_ID,
    FROST_GIANT_RACE_ID, GNOME_RACE_ID, GOBLIN_RACE_ID, HALF_ELF_RACE_ID, MINOTAUR_RACE_ID,
    ONYX_KIN_RACE_ID, ORC_RACE_ID, RaceSize, SAUR_RACE_ID, STORM_GIANT_RACE_ID, SYLPHKIN_RACE_ID,
    WOOD_ELF_RACE_ID,
};
use super::racial_stealth_types::{
    MovementNoiseClass, RACIAL_STEALTH_TABLE_ID, RPG_RACIAL_STEALTH_SCHEMA, RacialStealthProfile,
    RacialStealthProfileTable, RacialStealthTrait, StealthTerrainScope, StealthTraitRole,
};

const CF: &str = "https://wiki.carrionfields.net/race";

/// The full authored racial stealth profile table.
pub fn racial_stealth_profile_table() -> RacialStealthProfileTable {
    RacialStealthProfileTable {
        schema: RPG_RACIAL_STEALTH_SCHEMA.to_string(),
        table_id: RACIAL_STEALTH_TABLE_ID.to_string(),
        profiles: vec![
            // Human: no innate stealth; baseline noise.
            plain(
                ASHEN_HUMAN_RACE_ID,
                "Ashfolk",
                "human",
                RaceSize::Medium,
                MovementNoiseClass::Normal,
                &["No innate stealth or detection; learns everything through class training."],
            ),
            // Elf: 'almost universally so light of step that their passage goes
            // unnoticed' => innate PERMANENT sneak. Footstep radius 0.
            profile(
                ELF_RACE_ID,
                "Silverstride",
                "elf",
                RaceSize::Medium,
                MovementNoiseClass::Silent,
                vec![silent_move(
                    "trait.elf_light_step",
                    "ability.sneak",
                    StealthTerrainScope::Always,
                    "Light of step everywhere: passage goes unnoticed without practice (permanent sneak).",
                )],
                &[
                    "Footstep audio radius 0; exit-direction cues and object-interaction sounds suppressed.",
                ],
            ),
            // Half-Elf: 'move quietly in natural terrain' => natural quiet movement
            // (helpfile 483, no practice needed).
            profile(
                HALF_ELF_RACE_ID,
                "Duskborn",
                "half-elf",
                RaceSize::Medium,
                MovementNoiseClass::Normal,
                vec![silent_move(
                    "trait.half_elf_natural_quiet",
                    "ability.quiet_movement",
                    StealthTerrainScope::NaturalTerrain,
                    "Moves quietly in natural terrain with no need to practice quiet movement.",
                )],
                &["Footstep radius halved on natural-terrain tags; normal in cities/indoors."],
            ),
            // Half-Drow: same natural quiet-movement line as the half-elf.
            profile(
                ASHKIN_MIXED_RACE_ID,
                "Gloamblood",
                "half-drow",
                RaceSize::Medium,
                MovementNoiseClass::Normal,
                vec![silent_move(
                    "trait.half_drow_natural_quiet",
                    "ability.quiet_movement",
                    StealthTerrainScope::NaturalTerrain,
                    "Moves quietly in natural terrain with no need to practice quiet movement.",
                )],
                &["Footstep radius halved on natural-terrain tags; normal in cities/indoors."],
            ),
            // Dark-Elf: 'can walk unnoticed' => perma-sneak; PLUS innate faerie fire.
            profile(
                ASHKIN_DEEP_RACE_ID,
                "Nightkin",
                "dark-elf",
                RaceSize::Medium,
                MovementNoiseClass::Silent,
                vec![
                    silent_move(
                        "trait.dark_elf_unnoticed",
                        "ability.sneak",
                        StealthTerrainScope::Always,
                        "Walks unnoticed everywhere without practice (permanent sneak).",
                    ),
                    reveal(
                        "trait.dark_elf_faerie_fire",
                        "ability.faerie_fire",
                        "Innate faerie fire: the counter-tool lives in the stealthiest race.",
                    ),
                ],
                &[
                    "Footstep audio radius 0; carries the concealment counter it is hardest to catch with.",
                ],
            ),
            // Wood-Elf: quiet in the wilds, less so than other elves; wilderness-only.
            profile(
                WOOD_ELF_RACE_ID,
                "Thornstep",
                "wood-elf",
                RaceSize::Medium,
                MovementNoiseClass::Normal,
                vec![silent_move(
                    "trait.wood_elf_wild_quiet",
                    "ability.quiet_movement",
                    StealthTerrainScope::Wilderness,
                    "Walks quietly in the wilds, though not so quietly as high elves; wilderness only.",
                )],
                &[
                    "Nature-skill affinity: trains camouflage/entangle faster (not an innate grant, see #1623).",
                ],
            ),
            // Dwarf: no innate stealth.
            plain(
                DWARF_RACE_ID,
                "Deepforged",
                "dwarf",
                RaceSize::Medium,
                MovementNoiseClass::Normal,
                &["No innate stealth or detection; sturdy frame, baseline movement noise."],
            ),
            // Duergar: racially trained detect hidden (helpfile 489: strong
            // underground/city, weaker outdoors).
            profile(
                ONYX_KIN_RACE_ID,
                "Onyxkin",
                "duergar",
                RaceSize::Medium,
                MovementNoiseClass::Normal,
                vec![detect(
                    "trait.duergar_detect_hidden",
                    "ability.detect_hidden",
                    StealthTerrainScope::UndergroundAndCity,
                    "Racial detect hidden: sharp underground and in cities, weaker in open wilds.",
                )],
                &["Grouped with thieves by the detect-hidden helpfile; terrain-profiled sense."],
            ),
            // Gnome: no innate stealth; small frame => quiet.
            plain(
                GNOME_RACE_ID,
                "Cogkin",
                "gnome",
                RaceSize::Small,
                MovementNoiseClass::Quiet,
                &["No innate stealth or detection; small frame keeps footfalls quiet."],
            ),
            // Svirfneblin: passive DETECT INVIS always on.
            profile(
                DEEP_KIN_RACE_ID,
                "Gloamgnome",
                "svirfneblin",
                RaceSize::Small,
                MovementNoiseClass::Quiet,
                vec![detect(
                    "trait.svirfneblin_detect_invis",
                    "ability.detect_invisibility",
                    StealthTerrainScope::Always,
                    "Passive detect invisibility is always on; sees the invisible without casting.",
                )],
                &["Small frame keeps footfalls quiet; the always-on sense never lapses."],
            ),
            // Orc: no innate stealth; class-conditional hide terrains (orc berserker).
            plain(
                ORC_RACE_ID,
                "Ironmaw",
                "orc",
                RaceSize::Medium,
                MovementNoiseClass::Normal,
                &[
                    "No innate stealth; orc berserkers unlock special hide terrains via class (#1623).",
                ],
            ),
            // Goblin: no innate stealth; class-conditional hide terrains (goblin raider).
            plain(
                GOBLIN_RACE_ID,
                "Skulk",
                "goblin",
                RaceSize::Small,
                MovementNoiseClass::Quiet,
                &[
                    "No innate stealth; goblin raiders unlock special hide terrains via class (#1623).",
                ],
            ),
            // Minotaur: large frame; heavy noise; creep penalty.
            plain(
                MINOTAUR_RACE_ID,
                "Hornbound",
                "minotaur",
                RaceSize::Large,
                MovementNoiseClass::Heavy,
                &["Large frame broadcasts heavy footfalls; size penalizes creep."],
            ),
            // Giants: oversized noise radius; size penalizes creep.
            plain(
                STORM_GIANT_RACE_ID,
                "Stormcrest",
                "storm-giant",
                RaceSize::Giant,
                MovementNoiseClass::Thunderous,
                &["Oversized noise radius; size heavily penalizes creep."],
            ),
            plain(
                FIRE_GIANT_RACE_ID,
                "Embercrest",
                "fire-giant",
                RaceSize::Giant,
                MovementNoiseClass::Thunderous,
                &["Oversized noise radius; size heavily penalizes creep."],
            ),
            plain(
                FROST_GIANT_RACE_ID,
                "Rimecrest",
                "frost-giant",
                RaceSize::Giant,
                MovementNoiseClass::Thunderous,
                &["Oversized noise radius; size heavily penalizes creep."],
            ),
            plain(
                CLOUD_GIANT_RACE_ID,
                "Skycrest",
                "cloud-giant",
                RaceSize::Giant,
                MovementNoiseClass::Thunderous,
                &["Oversized noise radius; size heavily penalizes creep."],
            ),
            // Arial (sylphkin): no innate stealth called out by CF; baseline noise.
            plain(
                SYLPHKIN_RACE_ID,
                "Windwing",
                "arial",
                RaceSize::Medium,
                MovementNoiseClass::Normal,
                &["No innate stealth or detection transcribed; baseline movement noise."],
            ),
            // Felar: non-retractable claws => CANNOT innately sneak; claw-click on
            // hard surfaces until class training teaches quiet movement.
            plain(
                FELISAR_RACE_ID,
                "Clawkin",
                "felar",
                RaceSize::Medium,
                MovementNoiseClass::Normal,
                &[
                    "Non-retractable claws: cannot innately sneak; claws click on hard surfaces until trained.",
                    "Must learn to walk unnoticed like other races through class training.",
                ],
            ),
            // Saurian: large frame; heavy noise; creep penalty.
            plain(
                SAUR_RACE_ID,
                "Scalebound",
                "saurian",
                RaceSize::Large,
                MovementNoiseClass::Heavy,
                &["Large frame broadcasts heavy footfalls; size penalizes creep."],
            ),
        ],
    }
}

// --- Builders ---------------------------------------------------------------

fn profile(
    race_id: &str,
    display_name: &str,
    cf_slug: &str,
    size: RaceSize,
    noise: MovementNoiseClass,
    innate_traits: Vec<RacialStealthTrait>,
    notes: &[&str],
) -> RacialStealthProfile {
    RacialStealthProfile {
        race_id: race_id.to_string(),
        display_name: display_name.to_string(),
        source_issue: 1603,
        mechanics_source_url: format!("{CF}/{cf_slug}"),
        size,
        movement_noise: noise,
        footstep_radius_scale_percent: noise.footstep_radius_scale_percent(),
        creep_size_penalty: matches!(size, RaceSize::Large | RaceSize::Giant),
        innate_traits,
        notes: notes.iter().map(|note| (*note).to_string()).collect(),
    }
}

fn plain(
    race_id: &str,
    display_name: &str,
    cf_slug: &str,
    size: RaceSize,
    noise: MovementNoiseClass,
    notes: &[&str],
) -> RacialStealthProfile {
    profile(
        race_id,
        display_name,
        cf_slug,
        size,
        noise,
        Vec::new(),
        notes,
    )
}

fn innate_trait(
    trait_id: &str,
    role: StealthTraitRole,
    ability_id: &str,
    scope: StealthTerrainScope,
    description: &str,
) -> RacialStealthTrait {
    RacialStealthTrait {
        trait_id: trait_id.to_string(),
        role,
        ability_id: ability_id.to_string(),
        scope,
        requires_training: false,
        description: description.to_string(),
    }
}

fn silent_move(
    trait_id: &str,
    ability_id: &str,
    scope: StealthTerrainScope,
    description: &str,
) -> RacialStealthTrait {
    innate_trait(
        trait_id,
        StealthTraitRole::SilentMove,
        ability_id,
        scope,
        description,
    )
}

fn detect(
    trait_id: &str,
    ability_id: &str,
    scope: StealthTerrainScope,
    description: &str,
) -> RacialStealthTrait {
    innate_trait(
        trait_id,
        StealthTraitRole::Detect,
        ability_id,
        scope,
        description,
    )
}

fn reveal(trait_id: &str, ability_id: &str, description: &str) -> RacialStealthTrait {
    innate_trait(
        trait_id,
        StealthTraitRole::Reveal,
        ability_id,
        StealthTerrainScope::Always,
        description,
    )
}
