use crate::rpg::{
    ASHKIN_DEEP_RACE_ID, ASHKIN_MIXED_RACE_ID, AttributeScores, HALF_ELF_RACE_ID,
    RPG_RACE_SPEC_SCHEMA, RaceAccessRule, RaceAttributeCaps, RaceClassAccess, RaceHometown,
    RaceInnateAbility, RaceResistanceProfile, RaceSize, RaceSpec, RaceVision, RaceVisualContract,
    race_attribute_caps,
};

// #1001 Half-Elf. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/half-elf); only name/lore is originalized per #984.
pub(crate) fn half_elf_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: HALF_ELF_RACE_ID.to_string(),
        display_name: "Half-Elf".to_string(),
        player_facing_canon_name: "Half-Elf".to_string(),
        source_issue: 1001,
        canon_issues: vec![984, 985, 1001],
        mechanics_source_url: "https://wiki.carrionfields.net/race/half-elf".to_string(),
        source_history_label: "CF Half-Elf factual mechanics source only".to_string(),
        culture_summary: "Bridge-Kin half-elves - mixed-heritage trade and bridge families moving between Galdmere's courts, the forest paths, and wandering caravans.".to_string(),
        stat_caps: AttributeScores::new(19, 21, 21, 21, 19, 20),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: Vec::new(),
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "iron".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "infravision".to_string(), display_name: "Infravision".to_string(), description: "Sees in the dark by heat and shadow without a light source.".to_string() }, RaceInnateAbility { ability_id: "move_quietly".to_string(), display_name: "Quiet Passage".to_string(), description: "Moves quietly through natural terrain, hard to hear in the wilds.".to_string() }],
        vision: RaceVision { infravision: true, infravision_tiles: 15 },
        class_access: RaceClassAccess::AllExcept { class_ids: vec!["class.reach_ravager".to_string()] },
        experience_penalty_percent: 0,
        hometowns: vec![home("origin.half_elf.bridge_house", "Bridge House", "Vhael bridge families between courts and roads")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.half_elf.bridge_kin".to_string(),
            skeleton_id: "skeleton.biped.slender".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "medium biped, slender mixed-heritage proportions".to_string(),
            material_notes: "Vhael-original bridge-family raiment and road-worn travel gear; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Vulnerable: Iron' (half-strength; can hold and sacrifice but not wear) is encoded as the 'iron' affinity vulnerability via the #1631 RaceAffinity vocabulary.".to_string(), "CF gives no explicit size tier; prose is 5'6\" to 6'4\", slender. Medium is the nearest framework tier.".to_string(), "CF lists infravision explicitly but no numeric range; infravision_tiles set to the framework default of 15.".to_string()],
    }
}

pub(crate) fn half_elf_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&half_elf_race_spec())
}

// #1002 Ashkin (Mixed-Line). Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/half-drow); only name/lore is originalized per #984.
pub(crate) fn ashkin_mixed_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: ASHKIN_MIXED_RACE_ID.to_string(),
        display_name: "Ashkin (Mixed-Line)".to_string(),
        player_facing_canon_name: "Ashkin (Mixed-Line)".to_string(),
        source_issue: 1002,
        canon_issues: vec![984, 985, 1002],
        mechanics_source_url: "https://wiki.carrionfields.net/race/half-drow".to_string(),
        source_history_label: "CF Half-Drow factual mechanics source only".to_string(),
        culture_summary: "Mixed-line Ashkin - human-blooded grey elves whose diluted Sundering-mark softens the Hollow's touch; distrusted but not cursed, half-belonging to marked and walker alike.".to_string(),
        stat_caps: AttributeScores::new(19, 21, 21, 21, 19, 20),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: Vec::new(),
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "mithril".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "infravision".to_string(), display_name: "Infravision".to_string(), description: "Sees in the dark by heat and shadow without a light source.".to_string() }, RaceInnateAbility { ability_id: "move_quietly".to_string(), display_name: "Quiet Passage".to_string(), description: "Moves quietly through natural terrain, hard to hear in the wilds.".to_string() }],
        vision: RaceVision { infravision: true, infravision_tiles: 15 },
        class_access: RaceClassAccess::AllExcept { class_ids: vec!["class.reach_ravager".to_string()] },
        experience_penalty_percent: 0,
        hometowns: vec![home("origin.ashkin.thinweave_edge", "Thin-Weave Edge", "The thin-Weave borderlands where mixed-line Ashkin walk")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.ashkin.mixed_line".to_string(),
            skeleton_id: "skeleton.biped.slender".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "medium biped, slender grey-marked proportions".to_string(),
            material_notes: "Vhael-original grey-marked raiment and muted travel greys; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Vulnerable: Mithril' (half-strength) is encoded as the 'mithril' affinity vulnerability via the #1631 RaceAffinity vocabulary.".to_string(), "CF gives no explicit size tier; prose is 5'6\" to 6'4\", slender. Medium is the nearest framework tier.".to_string(), "CF lists infravision explicitly but no numeric range; infravision_tiles set to the framework default of 15.".to_string(), "Name/lore originalized per #984: player-facing category is the canon-original Ashkin (mixed-line); CF Half-Drow is the mechanics source only. Mechanics unchanged.".to_string()],
    }
}

pub(crate) fn ashkin_mixed_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&ashkin_mixed_race_spec())
}

// #1003 Ashkin (Deep-Line). Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/dark-elf); only name/lore is originalized per #984.
pub(crate) fn ashkin_deep_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: ASHKIN_DEEP_RACE_ID.to_string(),
        display_name: "Ashkin (Deep-Line)".to_string(),
        player_facing_canon_name: "Ashkin (Deep-Line)".to_string(),
        source_issue: 1003,
        canon_issues: vec![984, 985, 1003],
        mechanics_source_url: "https://wiki.carrionfields.net/race/dark-elf".to_string(),
        source_history_label: "CF Dark-Elf factual mechanics source only".to_string(),
        culture_summary: "Deep-line Ashkin - the undiluted grey-marked line of the Sundering, gifted and dangerous near thin Weave, their power bound to the shadowed path they walk.".to_string(),
        stat_caps: AttributeScores::new(18, 24, 21, 24, 16, 18),
        alignment: RaceAccessRule::Restricted { values: vec!["evil".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: Vec::new(),
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "mithril".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "faerie_fire".to_string(), display_name: "Faerie Fire".to_string(), description: "Calls a clinging magical light that outlines a foe, marking them for easier strikes.".to_string() }, RaceInnateAbility { ability_id: "infravision".to_string(), display_name: "Infravision".to_string(), description: "Sees in the dark by heat and shadow without a light source.".to_string() }],
        vision: RaceVision { infravision: true, infravision_tiles: 15 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.galdmere_shade".to_string(), "class.sinewright".to_string(), "class.currentbraider".to_string(), "class.gravebinder".to_string(), "class.circlebinder".to_string(), "class.hushblade".to_string(), "class.blackoath_knight".to_string(), "class.lifewarden".to_string(), "class.scourge_priest".to_string(), "class.thornwarden".to_string()] },
        experience_penalty_percent: 400,
        hometowns: vec![home("origin.ashkin.deep_seat", "Deep Seat", "The deep thin-Weave holdfast of the deep-line Ashkin")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.ashkin.deep_line".to_string(),
            skeleton_id: "skeleton.biped.slender".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "medium biped, slight deep-marked proportions".to_string(),
            material_notes: "Vhael-original deep-marked raiment and shadow-worn greys; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Vulnerable: Mithril' is encoded as the 'mithril' affinity vulnerability via the #1631 RaceAffinity vocabulary.".to_string(), "CF: deep-line must stay evil to cast their spells; encoded as the Evil-only alignment restriction.".to_string(), "CF also states they can 'walk unnoticed'; captured as flavor alongside the infravision innate.".to_string(), "CF gives no explicit size tier; prose is slight build, average 5'6\". Medium is the nearest framework tier.".to_string(), "CF lists infravision explicitly but no numeric range; infravision_tiles set to the framework default of 15.".to_string(), "Name/lore originalized per #984: player-facing category is the canon-original Ashkin (deep-line); CF Dark-Elf is the mechanics source only. Mechanics unchanged.".to_string()],
    }
}

pub(crate) fn ashkin_deep_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&ashkin_deep_race_spec())
}

fn home(origin_id: &str, display_name: &str, canon_place: &str) -> RaceHometown {
    RaceHometown {
        origin_id: origin_id.to_string(),
        display_name: display_name.to_string(),
        canon_place: canon_place.to_string(),
    }
}
