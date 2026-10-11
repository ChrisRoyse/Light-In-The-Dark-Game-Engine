use crate::rpg::{
    AttributeScores, FELISAR_RACE_ID, RPG_RACE_SPEC_SCHEMA, RaceAccessRule, RaceAttributeCaps,
    RaceClassAccess, RaceHometown, RaceInnateAbility, RaceResistanceProfile, RaceSize, RaceSpec,
    RaceVision, RaceVisualContract, SAUR_RACE_ID, SYLPHKIN_RACE_ID, race_attribute_caps,
};

// #1016 Sylphkin. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/arial); only name/lore is originalized per #984.
pub(crate) fn sylphkin_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: SYLPHKIN_RACE_ID.to_string(),
        display_name: "Sylphkin".to_string(),
        player_facing_canon_name: "Sylphkin".to_string(),
        source_issue: 1016,
        canon_issues: vec![984, 985, 1016],
        mechanics_source_url: "https://wiki.carrionfields.net/race/arial".to_string(),
        source_history_label: "CF Arial factual mechanics source only".to_string(),
        culture_summary: "Sylphkin - the winged, air-blooded high-dwelling people of Vhael's mountain heights, swift and lightning-marked.".to_string(),
        stat_caps: AttributeScores::new(19, 22, 20, 25, 18, 18),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: Vec::new(),
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "lightning".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "drowning".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "wingsweep".to_string(), display_name: "Wingsweep".to_string(), description: "Sweeps broad wings to buffet and knock back nearby foes.".to_string() }, RaceInnateAbility { ability_id: "flight".to_string(), display_name: "Flight".to_string(), description: "Takes to the air and flies under its own power.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.galdmere_shade".to_string(), "class.hushblade".to_string(), "class.wandersong".to_string(), "class.sinewright".to_string(), "class.currentbraider".to_string(), "class.circlebinder".to_string(), "class.gravebinder".to_string(), "class.shapeshifter".to_string(), "class.blackoath_knight".to_string(), "class.scourge_priest".to_string(), "class.lifewarden".to_string(), "class.wildbinder".to_string(), "class.thornwarden".to_string()] },
        experience_penalty_percent: 300,
        hometowns: vec![home("origin.sylphkin.high_aerie", "High Aerie", "The high mountain aeries of the Sylphkin")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.sylphkin.high_aerie".to_string(),
            skeleton_id: "skeleton.biped.winged".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "tall, slender, winged biped, six to seven feet".to_string(),
            material_notes: "Vhael-original aerie raiment, feathered wings and sky tones; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Vulnerable: Drowning' (Hydrophobia; difficulty diving) is encoded as the 'drowning' affinity vulnerability via the #1631 RaceAffinity vocabulary; the Lightning vulnerability uses the combat channel.".to_string(), "CF states 'Vulnerable' qualitatively with no numeric magnitude; the lightning channel uses the framework default magnitude of 25.".to_string(), "CF lists no Resistant entry for this race; resistances left empty.".to_string(), "CF bars this race from Paladin (redirected to Shaman or Warrior); the allowed-class list is encoded as a Restricted allowlist that omits Paladin.".to_string(), "CF gives no explicit size tier; prose is about 6 to 7 feet, slender. Medium is the nearest framework tier.".to_string(), "CF lists no infravision for this race; infravision left false.".to_string(), "Name/lore originalized per #984: player-facing category is the canon-original Sylphkin; CF Arial is the mechanics source only. Mechanics unchanged.".to_string()],
    }
}

pub(crate) fn sylphkin_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&sylphkin_race_spec())
}

// #1017 Felisar. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/felar); only name/lore is originalized per #984.
pub(crate) fn felisar_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: FELISAR_RACE_ID.to_string(),
        display_name: "Felisar".to_string(),
        player_facing_canon_name: "Felisar".to_string(),
        source_issue: 1017,
        canon_issues: vec![984, 985, 1017],
        mechanics_source_url: "https://wiki.carrionfields.net/race/felar".to_string(),
        source_history_label: "CF Felar factual mechanics source only".to_string(),
        culture_summary: "Felisar - the muscular, claw-fanged cat-kin street-fighters of Vhael, pack-bonded and fire-shy.".to_string(),
        stat_caps: AttributeScores::new(17, 17, 20, 23, 23, 17),
        alignment: RaceAccessRule::Any,
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: vec![RaceResistanceProfile { resistance_id: "cold".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "fire".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "drowning".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "feral_growl".to_string(), display_name: "Feral Growl".to_string(), description: "Looses a feral growl that cows and unsettles enemies.".to_string() }, RaceInnateAbility { ability_id: "pack_sense".to_string(), display_name: "Pack Sense".to_string(), description: "Senses the presence and state of pack-kin at a distance.".to_string() }],
        vision: RaceVision { infravision: true, infravision_tiles: 15 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.galdmere_shade".to_string(), "class.thornwarden".to_string(), "class.hushblade".to_string(), "class.blackoath_knight".to_string(), "class.shapeshifter".to_string()] },
        experience_penalty_percent: 250,
        hometowns: vec![home("origin.felisar.pack_warren", "Pack Warren", "The pack-warrens of the Felisar")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.felisar.pack_warren".to_string(),
            skeleton_id: "skeleton.biped.feline".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "short, very muscular feline humanoid, about five feet".to_string(),
            material_notes: "Vhael-original street-fighter wraps, claw and fang ornament, tawny fur tones; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Vulnerable: Drowning' (Hydrophobia; difficulty diving) is encoded as the 'drowning' affinity vulnerability via the #1631 RaceAffinity vocabulary; the Fire vulnerability uses the combat channel.".to_string(), "CF states 'Resistant' / 'Vulnerable' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF grants Hand to Hand at 100% (a skill grant, not a race-spec field); captured here pending a skill-grant surface.".to_string(), "CF combat traits (prose): non-retractable claws for extra damage, may only dual-wield claws as a second weapon and cannot use the cross skill, free claw attacks while wielding two-handed weapons, famed street fighting; captured here.".to_string(), "CF gives no explicit size tier; prose is about 5' with a very muscular build. Medium is the nearest framework tier.".to_string(), "CF lists infravision explicitly but no numeric range; infravision_tiles set to the framework default of 15.".to_string(), "Name/lore originalized per #984: player-facing category is the canon-original Felisar; CF Felar is the mechanics source only. Mechanics unchanged.".to_string()],
    }
}

pub(crate) fn felisar_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&felisar_race_spec())
}

// #1018 Saur. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/saurian); only name/lore is originalized per #984.
pub(crate) fn saur_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: SAUR_RACE_ID.to_string(),
        display_name: "Saur".to_string(),
        player_facing_canon_name: "Saur".to_string(),
        source_issue: 1018,
        canon_issues: vec![984, 985, 1018],
        mechanics_source_url: "https://wiki.carrionfields.net/race/saurian".to_string(),
        source_history_label: "CF Saurian factual mechanics source only".to_string(),
        culture_summary: "Saur - the seven-foot lizard-kin of Vhael's deep marshes, split by an ancient duality into paths of light and shadow.".to_string(),
        stat_caps: AttributeScores::new(17, 18, 24, 22, 19, 15),
        alignment: RaceAccessRule::Restricted { values: vec!["good".to_string(), "evil".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Large,
        resistances: vec![RaceResistanceProfile { resistance_id: "disease".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "poison".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "cold".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "coalesce".to_string(), display_name: "Coalesce".to_string(), description: "Draws scattered essence back together to reform and recover.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.thornwarden".to_string(), "class.scourge_priest".to_string(), "class.lifewarden".to_string(), "class.gravebinder".to_string(), "class.blackoath_knight".to_string(), "class.currentbraider".to_string(), "class.sinewright".to_string(), "class.shapeshifter".to_string()] },
        experience_penalty_percent: 300,
        hometowns: vec![home("origin.saur.mire_nest", "Mire Nest", "The deep marsh nesting-holds of the Saur")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.saur.mire_nest".to_string(),
            skeleton_id: "skeleton.biped.saurian".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "tall, seven-foot lizard humanoid with tail and claws".to_string(),
            material_notes: "Vhael-original marsh-hide wraps, scale tones and bone ornament; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Resistant: Disease, Poison' map to framework channels; 'Vulnerable: Cold' maps to the cold channel.".to_string(), "CF states 'Resistant' / 'Vulnerable' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF: an ancient duality (Sra and Weh) leads saurians to be good or evil; encoded as the Good-or-Evil alignment restriction.".to_string(), "CF gives no explicit size tier; prose is roughly seven feet, a humanoid lizard with tail, three-fingered clawed hands, and sharp teeth. Large is the nearest framework tier.".to_string(), "CF lists no infravision for this race; infravision left false.".to_string(), "CF hometown lore (swamps of Ysigrath / Saurian Village by the Feanwynn Weald) is protected CF expression and is NOT shipped; the hometown here is an original Vhael marsh nest per #984.".to_string(), "Name/lore originalized per #984: player-facing category is the canon-original Saur; CF Saurian is the mechanics source only. Mechanics unchanged.".to_string()],
    }
}

pub(crate) fn saur_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&saur_race_spec())
}

fn home(origin_id: &str, display_name: &str, canon_place: &str) -> RaceHometown {
    RaceHometown {
        origin_id: origin_id.to_string(),
        display_name: display_name.to_string(),
        canon_place: canon_place.to_string(),
    }
}
