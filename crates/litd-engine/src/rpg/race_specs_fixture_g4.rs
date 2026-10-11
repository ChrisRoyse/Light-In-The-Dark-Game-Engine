use crate::rpg::{
    AttributeScores, GOBLIN_RACE_ID, MINOTAUR_RACE_ID, RPG_RACE_SPEC_SCHEMA, RaceAccessRule,
    RaceAttributeCaps, RaceClassAccess, RaceHometown, RaceInnateAbility, RaceResistanceProfile,
    RaceSize, RaceSpec, RaceVision, RaceVisualContract, STORM_GIANT_RACE_ID, race_attribute_caps,
};

// #1010 Goblin. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/goblin); only name/lore is originalized per #984.
pub(crate) fn goblin_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: GOBLIN_RACE_ID.to_string(),
        display_name: "Goblin".to_string(),
        player_facing_canon_name: "Goblin".to_string(),
        source_issue: 1010,
        canon_issues: vec![984, 985, 1010],
        mechanics_source_url: "https://wiki.carrionfields.net/race/goblin".to_string(),
        source_history_label: "CF Goblin factual mechanics source only".to_string(),
        culture_summary: "Reach goblins - the dark-stomping raider folk of the Orc-marches, comparable in height to gnomes, unable to rise above their war-bands.".to_string(),
        stat_caps: AttributeScores::new(18, 16, 17, 24, 24, 13),
        alignment: RaceAccessRule::Restricted { values: vec!["neutral".to_string(), "evil".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Small,
        resistances: vec![RaceResistanceProfile { resistance_id: "poison".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "disease".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "magic".to_string(), amount: 25 }],
        innate_abilities: Vec::new(),
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.reach_whelp_raider".to_string()] },
        experience_penalty_percent: 100,
        hometowns: vec![home("origin.goblin.orc_march", "Orc-March Burrow", "The dark burrows of the Orc-marches")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.goblin.orc_march".to_string(),
            skeleton_id: "skeleton.biped.small".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "small, wiry, hunched biped".to_string(),
            material_notes: "Vhael-original scavenged rags and crude iron; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Vulnerable: Magic' is encoded as the 'magic' affinity vulnerability via the #1631 RaceAffinity vocabulary.".to_string(), "CF has no Inherent Abilities or Skills section for goblins; innate_abilities left empty.".to_string(), "CF says goblins 'can see well in the dark' in prose but never uses the word infravision; not promoted to a mechanical field, infravision left false.".to_string(), "CF: all goblins are Raiders by trade; encoded as the sole allowed class. Additional CF constraints (cannot advance beyond rank 40, may join but not rule an Orc clan, hatred of dwarves and gnomes, barred from Galadon) are captured here as facts pending their own surfaces.".to_string(), "CF states 'Resistant' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF gives no explicit size tier; prose says shorter than humans, comparable to gnomes. Small is the nearest framework tier.".to_string()],
    }
}

pub(crate) fn goblin_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&goblin_race_spec())
}

// #1011 Minotaur. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/minotaur); only name/lore is originalized per #984.
pub(crate) fn minotaur_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: MINOTAUR_RACE_ID.to_string(),
        display_name: "Minotaur".to_string(),
        player_facing_canon_name: "Minotaur".to_string(),
        source_issue: 1011,
        canon_issues: vec![984, 985, 1011],
        mechanics_source_url: "https://wiki.carrionfields.net/race/minotaur".to_string(),
        source_history_label: "CF Minotaur factual mechanics source only".to_string(),
        culture_summary: "Reach minotaurs - rare, tall, horned beast-kin of the wild reaches, fearless in battle and bound to the warrior and shaman paths.".to_string(),
        stat_caps: AttributeScores::new(23, 16, 22, 19, 21, 11),
        alignment: RaceAccessRule::Restricted { values: vec!["evil".to_string(), "neutral".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Large,
        resistances: vec![RaceResistanceProfile { resistance_id: "physical".to_string(), amount: 25 }],
        vulnerabilities: Vec::new(),
        innate_abilities: vec![RaceInnateAbility { ability_id: "gore".to_string(), display_name: "Gore".to_string(), description: "Lowers the horns and gores a foe with a charging strike.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.scourge_priest".to_string()] },
        experience_penalty_percent: 450,
        hometowns: vec![home("origin.minotaur.wild_reach", "Wild-Reach Labyrinth", "The maze-holds of the wild reaches")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.minotaur.wild_reach".to_string(),
            skeleton_id: "skeleton.biped.large".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "large, tall, hairy horned humanoid".to_string(),
            material_notes: "Vhael-original harness, horn and hide ornament; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Resistant: Bash, Slash, Pierce' covers all three physical damage types together, so it maps exactly to the framework 'physical' channel.".to_string(), "CF states 'Resistant' qualitatively with no numeric magnitude; the physical channel uses the framework default magnitude of 25.".to_string(), "CF prose (not mechanical fields): minotaurs are immune to fear and can shrug off many physical attacks; captured here.".to_string(), "CF notes minotaurs are very rare and sometimes unavailable at character creation; captured here as a fact pending an availability surface.".to_string(), "CF gives no explicit size tier; prose says large beasts, tall and hairy. Large is the nearest framework tier.".to_string(), "CF lists no infravision for minotaurs; infravision left false.".to_string()],
    }
}

pub(crate) fn minotaur_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&minotaur_race_spec())
}

// #1012 Storm Giant. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/storm-giant); only name/lore is originalized per #984.
pub(crate) fn storm_giant_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: STORM_GIANT_RACE_ID.to_string(),
        display_name: "Storm Giant".to_string(),
        player_facing_canon_name: "Storm Giant".to_string(),
        source_issue: 1012,
        canon_issues: vec![984, 985, 1012],
        mechanics_source_url: "https://wiki.carrionfields.net/race/storm-giant".to_string(),
        source_history_label: "CF Storm Giant factual mechanics source only".to_string(),
        culture_summary: "Storm giants - the tallest and most civilized of the giant-kin, ocean- and lake-dwelling storm-callers of good heart.".to_string(),
        stat_caps: AttributeScores::new(24, 17, 17, 17, 21, 19),
        alignment: RaceAccessRule::Restricted { values: vec!["good".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Giant,
        resistances: vec![RaceResistanceProfile { resistance_id: "physical".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "lightning".to_string(), amount: 25 }],
        vulnerabilities: Vec::new(),
        innate_abilities: vec![RaceInnateAbility { ability_id: "call_lightning".to_string(), display_name: "Call Lightning".to_string(), description: "Calls down a bolt of storm lightning upon a foe.".to_string() }, RaceInnateAbility { ability_id: "waterbreathing".to_string(), display_name: "Waterbreathing".to_string(), description: "Breathes underwater without drowning.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.lampbearer".to_string(), "class.scourge_priest".to_string(), "class.lifewarden".to_string(), "class.thornwarden".to_string()] },
        experience_penalty_percent: 500,
        hometowns: vec![home("origin.storm_giant.tide_hold", "Tide Hold", "The deep ocean and lakeshore halls of the storm giants")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.storm_giant.tide_hold".to_string(),
            skeleton_id: "skeleton.biped.giant".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "towering good-natured giant, near thirteen feet".to_string(),
            material_notes: "Vhael-original sea-storm raiment, blue-grey tones and lightning motifs; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Resistant: Bash, Slash, Pierce, Lightning' - the Bash/Slash/Pierce trio maps to the framework 'physical' channel, and Lightning maps to the lightning channel.".to_string(), "CF states 'Resistant' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF grants Bash and Enhanced Damage at 100% (skill grants, not race-spec fields); captured here pending a skill-grant surface.".to_string(), "CF gives no explicit discrete size category; prose is nearly 13' tall, the tallest civilized giant. Giant is the framework tier.".to_string(), "CF lists no infravision for storm giants; infravision left false.".to_string(), "CF label anomaly captured verbatim: STR reads 'Titanic (24)' here while the other giants read 'Titanic (25)', and INT reads 'Average (17)'.".to_string()],
    }
}

pub(crate) fn storm_giant_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&storm_giant_race_spec())
}

fn home(origin_id: &str, display_name: &str, canon_place: &str) -> RaceHometown {
    RaceHometown {
        origin_id: origin_id.to_string(),
        display_name: display_name.to_string(),
        canon_place: canon_place.to_string(),
    }
}
