use crate::rpg::{
    AttributeScores, DWARF_RACE_ID, ONYX_KIN_RACE_ID, RPG_RACE_SPEC_SCHEMA, RaceAccessRule,
    RaceAttributeCaps, RaceClassAccess, RaceHometown, RaceInnateAbility, RaceResistanceProfile,
    RaceSize, RaceSpec, RaceVision, RaceVisualContract, WOOD_ELF_RACE_ID, race_attribute_caps,
};

// #1004 Wood-Elf. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/wood-elf); only name/lore is originalized per #984.
pub(crate) fn wood_elf_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: WOOD_ELF_RACE_ID.to_string(),
        display_name: "Wood-Elf".to_string(),
        player_facing_canon_name: "Wood-Elf".to_string(),
        source_issue: 1004,
        canon_issues: vec![984, 985, 1004],
        mechanics_source_url: "https://wiki.carrionfields.net/race/wood-elf".to_string(),
        source_history_label: "CF Wood-Elf factual mechanics source only".to_string(),
        culture_summary: "Thornwild Wood-Elves - twilight-dwellers of the Thornwilds who chose the cycles of wood and season over honed magecraft; name-keepers and balance-holders uneasy in stone cities.".to_string(),
        stat_caps: AttributeScores::new(19, 23, 21, 24, 17, 22),
        alignment: RaceAccessRule::Restricted { values: vec!["neutral".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: Vec::new(),
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "iron".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "infravision".to_string(), display_name: "Infravision".to_string(), description: "Sees in the dark by heat and shadow without a light source.".to_string() }, RaceInnateAbility { ability_id: "wildwalk".to_string(), display_name: "Wild Step".to_string(), description: "Passes silently through wild and wooded terrain.".to_string() }, RaceInnateAbility { ability_id: "nature_adept".to_string(), display_name: "Nature Adept".to_string(), description: "Naturally adept at nature skills - Camouflage, Fashion Staff, and Entangle come easier or work better.".to_string() }],
        vision: RaceVision { infravision: true, infravision_tiles: 15 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.thornwarden".to_string(), "class.wildbinder".to_string(), "class.wandersong".to_string(), "class.lifewarden".to_string()] },
        experience_penalty_percent: 400,
        hometowns: vec![home("origin.wood_elf.thornwild", "Thornwild Reach", "The twilight woods of the Thornwilds")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.wood_elf.thornwild".to_string(),
            skeleton_id: "skeleton.biped.slender".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "medium biped, lean woodland proportions".to_string(),
            material_notes: "Vhael-original woodland raiment, bark and leaf greens; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Vulnerable: Iron' is encoded as the 'iron' affinity vulnerability via the #1631 RaceAffinity vocabulary.".to_string(), "CF: wood-elves must stay balanced (Neutral) and have chaotic tendencies; the alignment restriction is encoded, the chaotic-tendency ethos note is captured (no order/chaos restriction enforced).".to_string(), "CF gives no explicit size tier; prose says slightly stronger and heartier than high elves. Medium is the nearest framework tier.".to_string(), "CF lists infravision explicitly but no numeric range; infravision_tiles set to the framework default of 15.".to_string()],
    }
}

pub(crate) fn wood_elf_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&wood_elf_race_spec())
}

// #1005 Dwarf. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/dwarf); only name/lore is originalized per #984.
pub(crate) fn dwarf_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: DWARF_RACE_ID.to_string(),
        display_name: "Dwarf".to_string(),
        player_facing_canon_name: "Dwarf".to_string(),
        source_issue: 1005,
        canon_issues: vec![984, 985, 1005],
        mechanics_source_url: "https://wiki.carrionfields.net/race/dwarf".to_string(),
        source_history_label: "CF Dwarf factual mechanics source only".to_string(),
        culture_summary: "Karthroot Dwarves - forge-clans and tunnel-wardens grown from the stone-roots beneath Karthspire; bell-casters and oath-smiths whose stubbornness is proof against the Hollow.".to_string(),
        stat_caps: AttributeScores::new(22, 19, 19, 18, 25, 18),
        alignment: RaceAccessRule::Restricted { values: vec!["good".to_string(), "neutral".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: vec![RaceResistanceProfile { resistance_id: "disease".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "poison".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "magic".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "drowning".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "detect_secret_passage".to_string(), display_name: "Detect Secret Passage".to_string(), description: "Senses hidden doors and secret passages worked into stone.".to_string() }, RaceInnateAbility { ability_id: "forgecraft".to_string(), display_name: "Forgecraft".to_string(), description: "Inborn mastery of the forge and the working of metal.".to_string() }, RaceInnateAbility { ability_id: "armor_deflection".to_string(), display_name: "Dwarven Armor Deflection".to_string(), description: "Turns aside blows with the heavy set of dwarven armor.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.lampbearer".to_string(), "class.scourge_priest".to_string(), "class.lifewarden".to_string()] },
        experience_penalty_percent: 250,
        hometowns: vec![home("origin.dwarf.karthroot", "Karthroot Hold", "The forge-holds in Karthspire's stone roots")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.dwarf.karthroot".to_string(),
            skeleton_id: "skeleton.biped.stocky".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "short, broad, heavily built biped".to_string(),
            material_notes: "Vhael-original forge-clan mail, bell-metal ornament, and stone greys; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Resistant: Magic' is encoded as the 'magic' affinity resistance via the #1631 RaceAffinity vocabulary.".to_string(), "CF 'Vulnerable: Drowning' (Hydrophobia; difficulty diving) is encoded as the 'drowning' affinity vulnerability via the #1631 RaceAffinity vocabulary.".to_string(), "CF states 'Resistant' / 'Vulnerable' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF gives no explicit size tier; prose is near 4' but remarkably stocky. Medium is the nearest framework tier for their broad mass.".to_string(), "CF lists no infravision for dwarves; infravision left false.".to_string()],
    }
}

pub(crate) fn dwarf_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&dwarf_race_spec())
}

// #1006 Onyx-Kin. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/duergar); only name/lore is originalized per #984.
pub(crate) fn onyx_kin_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: ONYX_KIN_RACE_ID.to_string(),
        display_name: "Onyx-Kin".to_string(),
        player_facing_canon_name: "Onyx-Kin".to_string(),
        source_issue: 1006,
        canon_issues: vec![984, 985, 1006],
        mechanics_source_url: "https://wiki.carrionfields.net/race/duergar".to_string(),
        source_history_label: "CF Duergar factual mechanics source only".to_string(),
        culture_summary: "Onyx-Kin - the sundered, Hollow-hardened deep-dwarf under-culture beneath Karthspire, split from the hill-dwarves and bent to the deep dark under the Onyx Dominion.".to_string(),
        stat_caps: AttributeScores::new(23, 16, 18, 22, 22, 16),
        alignment: RaceAccessRule::Restricted { values: vec!["evil".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: vec![RaceResistanceProfile { resistance_id: "poison".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "disease".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "magic".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "holy".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "light".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "whitesteel".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "detect_hidden".to_string(), display_name: "Detect Hidden".to_string(), description: "Passively notices hidden creatures and things nearby.".to_string() }, RaceInnateAbility { ability_id: "infravision".to_string(), display_name: "Infravision".to_string(), description: "Sees in the dark by heat and shadow without a light source.".to_string() }],
        vision: RaceVision { infravision: true, infravision_tiles: 15 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.galdmere_shade".to_string(), "class.blackoath_knight".to_string(), "class.scourge_priest".to_string()] },
        experience_penalty_percent: 250,
        hometowns: vec![home("origin.onyx_kin.deep_dominion", "Deep Dominion Warren", "The deep warrens beneath Karthspire's Onyx Dominion")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.onyx_kin.deep_dominion".to_string(),
            skeleton_id: "skeleton.biped.stocky".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "short, lean-but-broad deep-dwarf biped".to_string(),
            material_notes: "Vhael-original deep-warren mail and onyx-black stone tones; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Resistant: Magic' is encoded as the 'magic' affinity resistance via the #1631 RaceAffinity vocabulary.".to_string(), "CF 'Vulnerable: Light' and 'Vulnerable: Whitesteel' are encoded as the 'light' and 'whitesteel' affinity vulnerabilities via the #1631 RaceAffinity vocabulary; the Holy vulnerability uses the combat channel.".to_string(), "CF grants Pick Lock at 100% (a skill grant, not a race-spec field); captured here pending a skill-grant surface.".to_string(), "CF states 'Resistant' / 'Vulnerable' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF gives no explicit size tier; prose is approximately 4'6\", slightly taller and thinner than surface dwarves. Medium is the nearest framework tier.".to_string(), "CF lists infravision explicitly but no numeric range; infravision_tiles set to the framework default of 15.".to_string(), "Name/lore originalized per #984: player-facing category is the original Onyx-Kin; CF Duergar is the mechanics source only and is not shipped as a name. Mechanics unchanged.".to_string()],
    }
}

pub(crate) fn onyx_kin_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&onyx_kin_race_spec())
}

fn home(origin_id: &str, display_name: &str, canon_place: &str) -> RaceHometown {
    RaceHometown {
        origin_id: origin_id.to_string(),
        display_name: display_name.to_string(),
        canon_place: canon_place.to_string(),
    }
}
