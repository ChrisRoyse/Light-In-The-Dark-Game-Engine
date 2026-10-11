use crate::rpg::{
    ASHEN_HUMAN_RACE_ID, AttributeScores, ELF_RACE_ID, RPG_RACE_SPEC_SCHEMA, RaceAccessRule,
    RaceAttributeCaps, RaceClassAccess, RaceHometown, RaceInnateAbility, RaceResistanceProfile,
    RaceSize, RaceSpec, RaceVision, RaceVisualContract, race_attribute_caps,
};

pub(crate) fn human_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: ASHEN_HUMAN_RACE_ID.to_string(),
        display_name: "Human".to_string(),
        player_facing_canon_name: "Human".to_string(),
        source_issue: 999,
        canon_issues: vec![985, 999, 1055, 1059],
        mechanics_source_url: "https://wiki.carrionfields.net/race/human".to_string(),
        source_history_label: "CF Human factual mechanics source only".to_string(),
        culture_summary:
            "City-born houses of Galdmere, road families, old noble remnants, and frontier settlers."
                .to_string(),
        stat_caps: AttributeScores::new(20, 20, 20, 20, 20, 20),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: Vec::new(),
        vulnerabilities: Vec::new(),
        innate_abilities: Vec::new(),
        vision: RaceVision {
            infravision: false,
            infravision_tiles: 0,
        },
        class_access: RaceClassAccess::All,
        experience_penalty_percent: 0,
        hometowns: vec![
            home("origin.galdmere.human_house", "Galdmere House", "Galdmere"),
            home("origin.vhael.road_family", "Road Family", "Galdmere roads"),
            home(
                "origin.ashen_reach.frontier",
                "Reach Frontier",
                "The Ashen Reach",
            ),
        ],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.human.vhael_adventurer".to_string(),
            skeleton_id: "skeleton.biped.medium".to_string(),
            required_animations: vec![
                "idle".to_string(),
                "walk".to_string(),
                "run".to_string(),
                "attack_1h".to_string(),
                "attack_2h".to_string(),
                "cast".to_string(),
                "interact".to_string(),
                "death".to_string(),
            ],
            silhouette: "medium biped, versatile adventurer proportions".to_string(),
            material_notes: "Vhael-original clothing, armor, skin, hair, and road-worn variants; no copied race iconography.".to_string(),
        },
        implementation_notes: vec![
            "Humans may be any class.".to_string(),
            "Humans have no source-listed resistance, vulnerability, innate, or infravision block."
                .to_string(),
            "Player-facing local culture is Vhael-original per #1059.".to_string(),
        ],
    }
}

pub(crate) fn human_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&human_race_spec())
}

// #1000 Elf. Every mechanic is captured verbatim from the Carrion Fields wiki
// (https://wiki.carrionfields.net/race/elf), per the "mechanics are factual" rule.
// Only the name/lore layer (culture_summary, hometown, visual) is originalized per #984.
pub(crate) fn elf_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: ELF_RACE_ID.to_string(),
        display_name: "Elf".to_string(),
        player_facing_canon_name: "Elf".to_string(),
        source_issue: 1000,
        canon_issues: vec![984, 985, 1000],
        mechanics_source_url: "https://wiki.carrionfields.net/race/elf".to_string(),
        source_history_label: "CF Elf (High Elf) factual mechanics source only".to_string(),
        culture_summary:
            "The Name-Keepers, eldest of the Light-devoted elven lines, holding the old memory of \
             Vhael from before the Sundering."
                .to_string(),
        // CF Max Stats: Strength Strong(18), Intelligence Genius(25), Wisdom Sagacious(21),
        // Dexterity Quick(23), Constitution Average(16), Charisma Disarming(21).
        stat_caps: AttributeScores::new(18, 25, 21, 23, 16, 21),
        // CF: "all High Elves must begin as Good-Aligned."
        alignment: RaceAccessRule::Restricted {
            values: vec!["good".to_string()],
        },
        // CF states no ethos (order/chaos) restriction for the Elf.
        ethos: RaceAccessRule::Any,
        // CF has no explicit size tier; described as taller than humans (avg 6'3") and slender.
        // Medium is the nearest framework tier (see implementation_notes).
        size: RaceSize::Medium,
        // CF "Resistant: Curse" and "Vulnerable: Iron" are encoded as the 'curse' and 'iron'
        // affinities via the #1631 RaceAffinity vocabulary, which decouples race affinity
        // channels from the nine combat damage channels (DEFAULT_RESISTANCE_IDS).
        resistances: vec![RaceResistanceProfile {
            resistance_id: "curse".to_string(),
            amount: 25,
        }],
        vulnerabilities: vec![RaceResistanceProfile {
            resistance_id: "iron".to_string(),
            amount: 25,
        }],
        innate_abilities: vec![RaceInnateAbility {
            // CF Inherent Abilities: "Swiftstrike".
            ability_id: "swiftstrike".to_string(),
            display_name: "Swiftstrike".to_string(),
            description:
                "Inherent elven quickness in the strike; the elf's light-footed passage often goes \
                 unnoticed by those who do not lay eyes upon it."
                    .to_string(),
        }],
        // CF: no infravision entry; High Elves are surface-dwellers "born to live under the sun".
        vision: RaceVision {
            infravision: false,
            infravision_tiles: 0,
        },
        // CF Classes: "any profession, save ... Druid, Anti-Paladin, or Necromancer.
        // In addition ... they cannot train as Assassins or Shamans."
        class_access: RaceClassAccess::AllExcept {
            class_ids: vec![
                "class.wildbinder".to_string(),
                "class.blackoath_knight".to_string(),
                "class.gravebinder".to_string(),
                "class.hushblade".to_string(),
                "class.scourge_priest".to_string(),
            ],
        },
        // CF Experience Penalty: 400.
        experience_penalty_percent: 400,
        // CF names no explicit hometown/recall field; flavor names the Kingdom of House Darsylon.
        // Originalized per #984 to the Name-Keeper high-elven seat.
        hometowns: vec![home(
            "origin.elf.name_keeper_seat",
            "Name-Keeper Seat",
            "The high-elven seat of the Name-Keepers",
        )],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.elf.name_keeper".to_string(),
            skeleton_id: "skeleton.biped.slender".to_string(),
            required_animations: vec![
                "idle".to_string(),
                "walk".to_string(),
                "run".to_string(),
                "attack_1h".to_string(),
                "attack_2h".to_string(),
                "cast".to_string(),
                "interact".to_string(),
                "death".to_string(),
            ],
            silhouette: "tall, slender biped; graceful high-elven proportions".to_string(),
            material_notes:
                "Vhael-original Name-Keeper raiment, carved name-tokens, and muted forest greys; \
                 no copied race iconography."
                    .to_string(),
        },
        implementation_notes: vec![
            "CF stat caps captured verbatim: STR Strong(18), INT Genius(25), WIS Sagacious(21), \
             DEX Quick(23), CON Average(16), CHA Disarming(21)."
                .to_string(),
            "CF 'Resistant: Curse' and 'Vulnerable: Iron' are encoded as the 'curse' and 'iron' \
             affinities via the #1631 RaceAffinity vocabulary, a race-affinity channel set \
             decoupled from the nine combat damage channels (DEFAULT_RESISTANCE_IDS)."
                .to_string(),
            "CF has no explicit Size field; descriptive only (taller than humans, avg 6'3\", \
             slender, least hardy). Medium is the nearest framework tier."
                .to_string(),
            "CF has no infravision entry; High Elves are surface-dwellers. Infravision left false."
                .to_string(),
            "CF class rule is a denylist (any profession EXCEPT Druid, Anti-Paladin, Necromancer, \
             Assassin, Shaman); encoded as RaceClassAccess::AllExcept."
                .to_string(),
            "Name/lore originalized per #984: player-facing category stays 'Elf'; the Vhael line is \
             the Light-devoted Name-Keepers. Mechanics unchanged."
                .to_string(),
        ],
    }
}

pub(crate) fn elf_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&elf_race_spec())
}

fn home(origin_id: &str, display_name: &str, canon_place: &str) -> RaceHometown {
    RaceHometown {
        origin_id: origin_id.to_string(),
        display_name: display_name.to_string(),
        canon_place: canon_place.to_string(),
    }
}
