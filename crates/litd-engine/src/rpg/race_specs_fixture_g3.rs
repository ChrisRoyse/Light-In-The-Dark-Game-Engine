use crate::rpg::{
    AttributeScores, DEEP_KIN_RACE_ID, GNOME_RACE_ID, ORC_RACE_ID, RPG_RACE_SPEC_SCHEMA,
    RaceAccessRule, RaceAttributeCaps, RaceClassAccess, RaceHometown, RaceInnateAbility,
    RaceResistanceProfile, RaceSize, RaceSpec, RaceVision, RaceVisualContract, race_attribute_caps,
};

// #1007 Gnome. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/gnome); only name/lore is originalized per #984.
pub(crate) fn gnome_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: GNOME_RACE_ID.to_string(),
        display_name: "Gnome".to_string(),
        player_facing_canon_name: "Gnome".to_string(),
        source_issue: 1007,
        canon_issues: vec![984, 985, 1007],
        mechanics_source_url: "https://wiki.carrionfields.net/race/gnome".to_string(),
        source_history_label: "CF Gnome factual mechanics source only".to_string(),
        culture_summary: "Guild-gnomes - clockwork-minded archive-tenders, illusion tinkers, and vault scribes of Galdmere's archives.".to_string(),
        stat_caps: AttributeScores::new(18, 23, 25, 20, 18, 19),
        alignment: RaceAccessRule::Restricted { values: vec!["neutral".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Small,
        resistances: vec![RaceResistanceProfile { resistance_id: "poison".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "bash".to_string(), amount: 25 }],
        innate_abilities: Vec::new(),
        vision: RaceVision { infravision: true, infravision_tiles: 15 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.ashen_warrior".to_string(), "class.galdmere_shade".to_string(), "class.sinewright".to_string(), "class.currentbraider".to_string(), "class.circlebinder".to_string(), "class.wildbinder".to_string(), "class.shapeshifter".to_string(), "class.thornwarden".to_string(), "class.lifewarden".to_string()] },
        experience_penalty_percent: 300,
        hometowns: vec![home("origin.gnome.galdmere_archive", "Archive Warren", "The guild archives of Galdmere")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.gnome.guild_scribe".to_string(),
            skeleton_id: "skeleton.biped.small".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "small, compact biped with nimble hands".to_string(),
            material_notes: "Vhael-original guild-scribe robes, lens and gear ornament; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Vulnerable: Bash' (blunt attacks, from small size) is encoded as the 'bash' affinity vulnerability via the #1631 RaceAffinity vocabulary (a narrower channel than the combat 'physical').".to_string(), "CF grants Lore and Identify at 100% (skill grants, not race-spec fields); captured here pending a skill-grant surface.".to_string(), "CF has no Inherent Abilities section for gnomes; innate_abilities left empty.".to_string(), "CF states 'Resistant: Poison' qualitatively with no numeric magnitude; the poison channel uses the framework default magnitude of 25.".to_string(), "CF gives no explicit size tier; prose is the smallest race, averaging 3'. Small is the nearest framework tier.".to_string(), "CF lists infravision explicitly but no numeric range; infravision_tiles set to the framework default of 15.".to_string()],
    }
}

pub(crate) fn gnome_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&gnome_race_spec())
}

// #1008 Deep-Kin. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/svirfneblin); only name/lore is originalized per #984.
pub(crate) fn deep_kin_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: DEEP_KIN_RACE_ID.to_string(),
        display_name: "Deep-Kin".to_string(),
        player_facing_canon_name: "Deep-Kin".to_string(),
        source_issue: 1008,
        canon_issues: vec![984, 985, 1008],
        mechanics_source_url: "https://wiki.carrionfields.net/race/svirfneblin".to_string(),
        source_history_label: "CF Svirfneblin factual mechanics source only".to_string(),
        culture_summary: "Deep-Kin - the stone-bound, disciplined deep-gnome under-culture near the Hollowgate Deeps, whose stoneskin and see-invisible gifts and rigid neutrality let them survive near thin Weave.".to_string(),
        stat_caps: AttributeScores::new(22, 18, 22, 20, 22, 19),
        alignment: RaceAccessRule::Restricted { values: vec!["neutral".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Small,
        resistances: vec![RaceResistanceProfile { resistance_id: "poison".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "disease".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "magic".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "bash".to_string(), amount: 25 }],
        innate_abilities: vec![RaceInnateAbility { ability_id: "stoneskin".to_string(), display_name: "Stoneskin".to_string(), description: "Skin hardens like stone, blunting incoming blows.".to_string() }, RaceInnateAbility { ability_id: "detect_invis".to_string(), display_name: "Detect Invis".to_string(), description: "Passively sees invisible creatures.".to_string() }],
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.circlebinder".to_string(), "class.lifewarden".to_string(), "class.currentbraider".to_string(), "class.thornwarden".to_string(), "class.shapeshifter".to_string(), "class.galdmere_shade".to_string(), "class.sinewright".to_string(), "class.ashen_warrior".to_string()] },
        experience_penalty_percent: 250,
        hometowns: vec![home("origin.deep_kin.hollowgate", "Hollowgate Roots", "The stone warrens near the Hollowgate Deeps")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.deep_kin.hollowgate".to_string(),
            skeleton_id: "skeleton.biped.small".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "small, stone-grey deep-gnome biped".to_string(),
            material_notes: "Vhael-original stone-warren garb and pale cavern greys; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF 'Resistant: Magic' is encoded as the 'magic' affinity resistance via the #1631 RaceAffinity vocabulary.".to_string(), "CF 'Vulnerable: Bash' is encoded as the 'bash' affinity vulnerability via the #1631 RaceAffinity vocabulary (narrower than the combat 'physical').".to_string(), "CF states 'Resistant' qualitatively with no numeric magnitude; encodable channels use the framework default magnitude of 25.".to_string(), "CF never states infravision (only 'have learned to see the invisible', captured as the Detect Invis innate); infravision left false.".to_string(), "CF gives no explicit size tier; prose is approximately 3'6\", slightly taller than surface gnomes. Small is the nearest framework tier.".to_string(), "Name/lore originalized per #984: player-facing category is the original Deep-Kin; CF Svirfneblin is the mechanics source only and is not shipped as a name. Mechanics unchanged.".to_string()],
    }
}

pub(crate) fn deep_kin_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&deep_kin_race_spec())
}

// #1009 Orc. Mechanics captured verbatim from Carrion Fields
// (https://wiki.carrionfields.net/race/orc); only name/lore is originalized per #984.
pub(crate) fn orc_race_spec() -> RaceSpec {
    RaceSpec {
        schema: RPG_RACE_SPEC_SCHEMA.to_string(),
        race_id: ORC_RACE_ID.to_string(),
        display_name: "Orc".to_string(),
        player_facing_canon_name: "Orc".to_string(),
        source_issue: 1009,
        canon_issues: vec![984, 985, 1009],
        mechanics_source_url: "https://wiki.carrionfields.net/race/orc".to_string(),
        source_history_label: "CF Orc factual mechanics source only".to_string(),
        culture_summary: "Reach-Edge Warband Orcs - warband families camped at the thin-Weave edge of the Reach, ruled by clan-champions and shaman-sanctioned chiefs who read strength as witnessed honor.".to_string(),
        stat_caps: AttributeScores::new(23, 15, 16, 18, 24, 13),
        alignment: RaceAccessRule::Restricted { values: vec!["evil".to_string()] },
        ethos: RaceAccessRule::Any,
        size: RaceSize::Medium,
        resistances: vec![RaceResistanceProfile { resistance_id: "disease".to_string(), amount: 25 }, RaceResistanceProfile { resistance_id: "acid".to_string(), amount: 25 }],
        vulnerabilities: vec![RaceResistanceProfile { resistance_id: "light".to_string(), amount: 25 }],
        innate_abilities: Vec::new(),
        vision: RaceVision { infravision: false, infravision_tiles: 0 },
        class_access: RaceClassAccess::Restricted { class_ids: vec!["class.reach_ravager".to_string()] },
        experience_penalty_percent: 100,
        hometowns: vec![home("origin.orc.reach_warband", "Reach Warband Camp", "The clan war-camps at the thin-Weave edge of the Reach")],
        visual: RaceVisualContract {
            renderer: "Rust/wgpu".to_string(),
            model_id: "model.kindred.orc.reach_warband".to_string(),
            skeleton_id: "skeleton.biped.heavy".to_string(),
            required_animations: vec!["idle".to_string(), "walk".to_string(), "run".to_string(), "attack_1h".to_string(), "attack_2h".to_string(), "cast".to_string(), "interact".to_string(), "death".to_string()],
            silhouette: "tall, heavy, often mis-shapen biped".to_string(),
            material_notes: "Vhael-original warband hides, bone and iron ornament; no copied race iconography.".to_string(),
        },
        implementation_notes: vec!["CF Experience Penalty is inconsistent on the source: the race page Classes section lists 0 while the race index lists 100. The index value 100 is used here; the discrepancy is captured verbatim, not reconciled.".to_string(), "CF 'Resistant: Acid' is encoded as the 'acid' affinity resistance via the #1631 RaceAffinity vocabulary; the Disease resistance uses the combat channel.".to_string(), "CF 'Vulnerable: Light' is encoded as the 'light' affinity vulnerability via the #1631 RaceAffinity vocabulary.".to_string(), "CF has no Inherent Abilities section for orcs; prose-only traits (strong immune system / eat poisonous food, hide in caves and swamps and clan villages, steal from one another, automatic Berserker, berserk-attack elves and wood-elves on sight) are captured here, innate_abilities left empty.".to_string(), "CF states orcs 'can see clearly without light' in prose but never lists an infravision ability, and pairs it with the Light vulnerability; infravision left false.".to_string(), "CF: orcs are automatically of the Berserker class; encoded as the sole allowed class. CF also states orcs are too quarrelsome to ever be orderly (an ethos note, not enforced).".to_string(), "CF states 'Resistant: Disease' qualitatively with no numeric magnitude; the disease channel uses the framework default magnitude of 25.".to_string(), "CF gives no explicit size tier; prose says taller than humans and often deformed. Medium is the nearest framework tier.".to_string()],
    }
}

pub(crate) fn orc_race_attribute_caps() -> RaceAttributeCaps {
    race_attribute_caps(&orc_race_spec())
}

fn home(origin_id: &str, display_name: &str, canon_place: &str) -> RaceHometown {
    RaceHometown {
        origin_id: origin_id.to_string(),
        display_name: display_name.to_string(),
        canon_place: canon_place.to_string(),
    }
}
