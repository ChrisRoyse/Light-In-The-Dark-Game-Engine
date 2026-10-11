use crate::{
    EngineError,
    rpg::{
        CharacterThemeInput, RPG_THEME_SCHEMA, ReligionThemeDefinition, RoleThemeDefinition,
        SphereDefinition, ThemeCatalog, ThemeRules, evaluate_character_theme,
    },
};

pub(crate) fn happy_rules() -> ThemeRules {
    ThemeRules::mud_defaults()
}

pub(crate) fn happy_catalog() -> ThemeCatalog {
    ThemeCatalog {
        schema: RPG_THEME_SCHEMA.to_string(),
        catalog_id: "catalog.issue964.themes".to_string(),
        spheres: vec![
            sphere("sphere.healing", "Healing", &["compassion", "restoration"]),
            sphere("sphere.mercy", "Mercy", &["compassion", "forgiveness"]),
            sphere(
                "sphere.protection",
                "Protection",
                &["oathbound", "guardian"],
            ),
            sphere("sphere.combat", "Combat", &["valor", "discipline"]),
            sphere("sphere.honor", "Honor", &["oathbound", "truth"]),
            sphere("sphere.war", "War", &["valor", "victory"]),
            sphere("sphere.victory", "Victory", &["triumph", "discipline"]),
            sphere("sphere.fire", "Fire", &["fervor", "destruction"]),
            sphere("sphere.death", "Death", &["mortality", "transition"]),
            sphere("sphere.greed", "Greed", &["ambition", "covetousness"]),
        ],
        roles: vec![
            role(
                "role.sentinel_mender",
                "Sentinel Mender",
                "sphere.healing",
                &["sphere.mercy", "sphere.protection"],
                &["compassion"],
                &["oathbound", "guardian"],
            ),
            role(
                "role.battle_oath",
                "Battle Oath",
                "sphere.combat",
                &["sphere.honor", "sphere.war"],
                &["valor"],
                &["oathbound", "discipline"],
            ),
            role(
                "role.ash_seeker",
                "Ash Seeker",
                "sphere.fire",
                &["sphere.death", "sphere.greed"],
                &["ambition"],
                &["fervor", "destruction"],
            ),
        ],
        religions: vec![
            religion(
                "religion.dawn_court",
                "deity.luminara",
                "Luminara",
                &["sphere.healing", "sphere.mercy", "sphere.protection"],
                &["compassion", "oathbound"],
                &["greed"],
            ),
            religion(
                "religion.iron_vow",
                "deity.vorran",
                "Vorran",
                &[
                    "sphere.combat",
                    "sphere.honor",
                    "sphere.war",
                    "sphere.victory",
                ],
                &["valor", "oathbound"],
                &["deception"],
            ),
            religion(
                "religion.ember_crown",
                "deity.ashar",
                "Ashar",
                &["sphere.fire", "sphere.death", "sphere.greed"],
                &["ambition"],
                &["mercy"],
            ),
        ],
    }
}

pub(crate) fn happy_input() -> CharacterThemeInput {
    CharacterThemeInput {
        character_id: "char.ashen.mender".to_string(),
        class_id: "class.healer".to_string(),
        level: 12,
        chosen_sphere_id: Some("sphere.healing".to_string()),
        role_id: "role.sentinel_mender".to_string(),
        desired_religion_id: Some("religion.dawn_court".to_string()),
        declared_theme_tags: vec!["oathbound".to_string(), "compassion".to_string()],
    }
}

pub(crate) fn mismatch_input() -> CharacterThemeInput {
    let mut input = happy_input();
    input.desired_religion_id = Some("religion.ember_crown".to_string());
    input
}

pub(crate) fn duplicate_sphere() -> Result<(), EngineError> {
    let mut catalog = happy_catalog();
    catalog.spheres.push(catalog.spheres[0].clone());
    evaluate_character_theme(&happy_input(), &catalog, &happy_rules()).map(|_| ())
}

pub(crate) fn unknown_role_sphere() -> Result<(), EngineError> {
    let mut catalog = happy_catalog();
    catalog.roles[0].primary_sphere_id = "sphere.missing".to_string();
    evaluate_character_theme(&happy_input(), &catalog, &happy_rules()).map(|_| ())
}

pub(crate) fn missing_sphere_after_deadline() -> Result<(), EngineError> {
    let mut input = happy_input();
    input.chosen_sphere_id = None;
    evaluate_character_theme(&input, &happy_catalog(), &happy_rules()).map(|_| ())
}

pub(crate) fn unknown_religion() -> Result<(), EngineError> {
    let mut input = happy_input();
    input.desired_religion_id = Some("religion.missing".to_string());
    evaluate_character_theme(&input, &happy_catalog(), &happy_rules()).map(|_| ())
}

pub(crate) fn forbidden_duplicate_tag() -> Result<(), EngineError> {
    let mut input = happy_input();
    input.declared_theme_tags.push("compassion".to_string());
    evaluate_character_theme(&input, &happy_catalog(), &happy_rules()).map(|_| ())
}

fn sphere(id: &str, name: &str, tags: &[&str]) -> SphereDefinition {
    SphereDefinition {
        sphere_id: id.to_string(),
        display_name: name.to_string(),
        theme_tags: ids(tags),
        affinity_sphere_ids: Vec::new(),
        opposing_sphere_ids: Vec::new(),
    }
}

fn role(
    id: &str,
    name: &str,
    primary: &str,
    supporting: &[&str],
    required: &[&str],
    optional: &[&str],
) -> RoleThemeDefinition {
    RoleThemeDefinition {
        role_id: id.to_string(),
        display_name: name.to_string(),
        primary_sphere_id: primary.to_string(),
        supporting_sphere_ids: ids(supporting),
        required_theme_tags: ids(required),
        optional_theme_tags: ids(optional),
    }
}

fn religion(
    id: &str,
    deity_id: &str,
    deity_name: &str,
    spheres: &[&str],
    required: &[&str],
    forbidden: &[&str],
) -> ReligionThemeDefinition {
    ReligionThemeDefinition {
        religion_id: id.to_string(),
        deity_id: deity_id.to_string(),
        deity_name: deity_name.to_string(),
        sphere_ids: ids(spheres),
        required_theme_tags: ids(required),
        forbidden_theme_tags: ids(forbidden),
        empowerment_class_ids: ids(&[
            "class.healer",
            "class.paladin",
            "class.shaman",
            "class.druid",
        ]),
    }
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}
