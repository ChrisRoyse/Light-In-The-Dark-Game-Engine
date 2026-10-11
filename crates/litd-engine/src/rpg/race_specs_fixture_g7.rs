use crate::rpg::{
    AttributeScores, CENTAUR_RACE_ID, RPG_RACE_SPEC_SCHEMA, RaceAccessRule, RaceAttributeCaps,
    RaceClassAccess, RaceHometown, RaceInnateAbility, RaceSize, RaceSpec, RaceVision,
    RaceVisualContract, race_attribute_caps,
};

// #1019 Centaur. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/centaur); only name/lore is originalized per #984.
pub(crate) fn centaur_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: CENTAUR_RACE_ID.to_string(),
        display_name: "Centaur".to_string(),
        player_facing_canon_name: "Centaur".to_string(),
        source_issue: 1019,
        canon_issues: vec![984, 985, 1019],
        mechanics_source_url: "https://wiki.carrionfields.net/race/centaur".to_string(),
        source_history_label: "CF Centaur factual mechanics source only".to_string(),
        culture_summary: "Centaur - the war-remembering, wilderness-nomad horse-folk of Vhael's old forests and moors, forsaking their savage past for goodness or neutrality.".to_string(),
        stat_caps: AttributeScores::new(22, 18, 23, 20, 20, 20),
        alignment: RaceAccessRule::Restricted { values: vec!["good".to_string(), "neutral".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Large,
        resistances: Vec::new(),
        vulnerabilities: Vec::new(),
        innate_abilities: vec![RaceInnateAbility { ability_id: "rear".to_string(), display_name: "Rear".to_string(), description: "Rears up to strike with the front hooves or kicks back with the hind legs.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.thornwarden".to_string(), "class.wildbinder".to_string(), "class.wandersong".to_string(), "class.scourge_priest".to_string()] },
        experience_penalty_percent: 500,
        hometowns: vec![home("origin.centaur.thornwild_range", "Thornwild Range", "The old-forest and moor ranges of the wandering Centaur")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.centaur.thornwild_range".to_string(),
            skeleton_id: "skeleton.quadruped.centaur".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "large centaur: humanoid torso rising from a four-legged equine body".to_string(),
            material_notes: "Vhael-original nomad-warrior barding, natural equestrian coat tones, flowing hair and hoof-shod legs; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF Centaur page has no Attributes section; no Resistant or Vulnerable entry is listed, so both are left empty (not fabricated).".to_string(), "CF inherent ability 'Rear' (helpfile 3314): natural front-hoof kick or hind-leg kickback; captured as the sole innate ability.".to_string(), "CF alignment is Good or Neutral ('the centaurs have forsaken the wickedness of their pasts and embrace a path of either goodness or neutrality'); encoded as the Good-or-Neutral alignment restriction. CF states no ethos restriction; ethos left Any.".to_string(), "CF gives no explicit Size tier; prose states 'They are large in size and unlike bipeds are unable to crawl.' Large is the nearest framework tier.".to_string(), "CF anatomy/gear prose (cannot crawl; shoes on two sets of hooves; armor on two sets of legs; pack on the equine back; cannot wear biped belts; hates boats) has no race-spec field yet; captured here pending an equipment-slot / movement surface.".to_string(), "CF marks Centaur a SEASONAL race ('only available at certain times of the year'; helpfile 3315); there is no availability-window field in the framework, so it is captured here and left unencoded.".to_string(), "CF lists no infravision for this race; infravision left false.".to_string(), "CF gives no hometown field (only 'remote wilderness' prose); the hometown here is an original Vhael range per #984. No canon Vhael kindred entry exists for Centaur yet (#1059/#1449 open); player-facing name/culture must be ratified there before public release.".to_string(), "Name/lore originalized per #984: player-facing category retains the descriptive Centaur label pending a ratified Vhael kindred name; CF Centaur is the mechanics source only. Mechanics unchanged.".to_string()],
    }
}

pub(crate) fn centaur_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&centaur_race_spec())
}

fn home(origin_id: &str, display_name: &str, canon_place: &str) -> RaceHometown {
    RaceHometown {
        origin_id: origin_id.to_string(),
        display_name: display_name.to_string(),
        canon_place: canon_place.to_string(),
    }
}
