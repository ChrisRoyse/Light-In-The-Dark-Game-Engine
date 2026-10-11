use crate::rpg::{
    AttributeScores, CLOUD_GIANT_RACE_ID, FIRE_GIANT_RACE_ID, FROST_GIANT_RACE_ID,
    RPG_RACE_SPEC_SCHEMA, RaceAccessRule, RaceAttributeCaps, RaceClassAccess, RaceHometown,
    RaceInnateAbility, RaceResistanceProfile, RaceSize, RaceSpec, RaceVision, RaceVisualContract,
    race_attribute_caps,
};

// #1013 Fire Giant. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/fire-giant); only name/lore is originalized per #984.
pub(crate) fn fire_giant_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: FIRE_GIANT_RACE_ID.to_string(),
        display_name: "Fire Giant".to_string(),
        player_facing_canon_name: "Fire Giant".to_string(),
        source_issue: 1013,
        canon_issues: vec![984, 985, 1013],
        mechanics_source_url: "https://wiki.carrionfields.net/race/fire-giant".to_string(),
        source_history_label: "CF Fire Giant factual mechanics source only".to_string(),
        culture_summary: "Fire giants - volcano-dwelling brute-kin of the deep fire, strong and stout and cruel.".to_string(),
        stat_caps: AttributeScores::new(25, 15, 17, 16, 23, 17),
        alignment: RaceAccessRule::Restricted { values: vec!["evil".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Giant,
        resistances: vec![RaceResistanceProfile { resistance_id: "physical".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "fire".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "cold".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "lavawalk".to_string(), display_name: "Lavawalk".to_string(), description: "Walks unharmed across lava and molten stone.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.blackoath_knight".to_string(), "class.scourge_priest".to_string(), "class.thornwarden".to_string()] },
        experience_penalty_percent: 500,
        hometowns: vec![home("origin.fire_giant.volcano_hold", "Emberdeep Hold", "The deep volcano halls of the fire giants")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.fire_giant.emberdeep".to_string(),
            skeleton_id: "skeleton.biped.giant".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "broad, brutish giant, ten to eleven feet".to_string(),
            material_notes: "Vhael-original char-black plate, ember-red tones and volcanic ornament; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Resistant: Bash, Slash, Pierce, Fire' - the Bash/Slash/Pierce trio maps to the framework 'physical' channel, and Fire maps to the fire channel.".to_string(), "CF states 'Resistant' / 'Vulnerable' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF grants Bash and Enhanced Damage at 100% (skill grants, not race-spec fields); captured here pending a skill-grant surface.".to_string(), "CF gives no explicit discrete size category; prose is 10 to 11 feet tall. Giant is the framework tier.".to_string(), "CF lists no infravision for fire giants; infravision left false.".to_string(), "CF label anomaly captured verbatim: CON reads 'Hearty (23)' where CF pairs 'Hearty' with 21 and 22 elsewhere.".to_string()],
    }
}

pub(crate) fn fire_giant_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&fire_giant_race_spec())
}

// #1014 Frost Giant. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/frost-giant); only name/lore is originalized per #984.
pub(crate) fn frost_giant_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: FROST_GIANT_RACE_ID.to_string(),
        display_name: "Frost Giant".to_string(),
        player_facing_canon_name: "Frost Giant".to_string(),
        source_issue: 1014,
        canon_issues: vec![984, 985, 1014],
        mechanics_source_url: "https://wiki.carrionfields.net/race/frost-giant".to_string(),
        source_history_label: "CF Frost Giant factual mechanics source only".to_string(),
        culture_summary: "Frost giants - reclusive, heartless brute-kin of the frozen mountains and icy caves.".to_string(),
        stat_caps: AttributeScores::new(25, 15, 17, 15, 24, 17),
        alignment: RaceAccessRule::Restricted { values: vec!["evil".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Giant,
        resistances: vec![RaceResistanceProfile { resistance_id: "physical".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "cold".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "fire".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "frostblade".to_string(), display_name: "Frostblade".to_string(), description: "Sheathes a weapon in killing frost.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.scourge_priest".to_string(), "class.thornwarden".to_string()] },
        experience_penalty_percent: 500,
        hometowns: vec![home("origin.frost_giant.frozen_hold", "Rimefast Hold", "The icy caves and frozen mountain-forests of the frost giants")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.frost_giant.rimefast".to_string(),
            skeleton_id: "skeleton.biped.giant".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "tall, cold-eyed giant, fifteen feet".to_string(),
            material_notes: "Vhael-original rime-white furs, ice-blue tones and frost ornament; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Resistant: Bash, Slash, Pierce, Cold' - the Bash/Slash/Pierce trio maps to the framework 'physical' channel, and Cold maps to the cold channel.".to_string(), "CF states 'Resistant' / 'Vulnerable' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF grants Bash and Enhanced Damage at 100% (skill grants, not race-spec fields); captured here pending a skill-grant surface.".to_string(), "CF gives no explicit discrete size category; prose is 15 feet tall. Giant is the framework tier.".to_string(), "CF narrative mentions eyes that see heat signatures in the cold, but lists no infravision ability; not promoted, infravision left false.".to_string(), "CF label anomaly captured verbatim: DEX reads 'Average (15)' where Fire and Cloud giants read 'Average (16)'.".to_string()],
    }
}

pub(crate) fn frost_giant_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&frost_giant_race_spec())
}

// #1015 Cloud Giant. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/cloud-giant); only name/lore is originalized per #984.
pub(crate) fn cloud_giant_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: CLOUD_GIANT_RACE_ID.to_string(),
        display_name: "Cloud Giant".to_string(),
        player_facing_canon_name: "Cloud Giant".to_string(),
        source_issue: 1015,
        canon_issues: vec![984, 985, 1015],
        mechanics_source_url: "https://wiki.carrionfields.net/race/cloud-giant".to_string(),
        source_history_label: "CF Cloud Giant factual mechanics source only".to_string(),
        culture_summary: "Cloud giants - sky-dwelling flyers of the high mountains, curse-bound to neutrality and fearful of the holy and negative powers.".to_string(),
        stat_caps: AttributeScores::new(25, 16, 17, 16, 22, 18),
        alignment: RaceAccessRule::Restricted { values: vec!["neutral".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Giant,
        resistances: vec![RaceResistanceProfile { resistance_id: "physical".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "cold".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "holy".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "disease".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "negative".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "flight".to_string(), display_name: "Flight".to_string(), description: "Takes to the air and flies under its own power.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.thornwarden".to_string(), "class.lifewarden".to_string(), "class.wildbinder".to_string()] },
        experience_penalty_percent: 500,
        hometowns: vec![home("origin.cloud_giant.sky_hold", "Skyhold Village", "The high mountain villages of the cloud giants")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.cloud_giant.skyhold".to_string(),
            skeleton_id: "skeleton.biped.giant".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "lofty pale giant, near twelve feet, winged flyer".to_string(),
            material_notes: "Vhael-original cloud-white raiment, sky-pale tones and feather motifs; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Resistant: Bash, Slash, Pierce, Cold' - the Bash/Slash/Pierce trio maps to the framework 'physical' channel, and Cold maps to the cold channel.".to_string(), "CF 'Vulnerable: Holy, Negative, Disease' - Holy and Disease map to combat channels; 'Negative' is encoded as the 'negative' affinity vulnerability via the #1631 RaceAffinity vocabulary.".to_string(), "CF states 'Resistant' / 'Vulnerable' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF: an ancient curse forces cloud giants to stay neutral and leaves them god-fearing and vulnerable to holy and negative powers; the Neutral alignment restriction is encoded.".to_string(), "CF grants Bash and Enhanced Damage at 100% (skill grants, not race-spec fields); captured here pending a skill-grant surface.".to_string(), "CF gives no explicit discrete size category; prose is about 12' tall. Giant is the framework tier.".to_string(), "CF lists no infravision for cloud giants; infravision left false.".to_string()],
    }
}

pub(crate) fn cloud_giant_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&cloud_giant_race_spec())
}

fn home(origin_id: &str, display_name: &str, canon_place: &str) -> RaceHometown {
    RaceHometown {
        origin_id: origin_id.to_string(),
        display_name: display_name.to_string(),
        canon_place: canon_place.to_string(),
    }
}
