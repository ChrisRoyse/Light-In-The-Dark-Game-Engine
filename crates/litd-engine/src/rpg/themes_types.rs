use serde::{Deserialize, Serialize};

pub const RPG_THEME_SCHEMA: &str = "litd.rpg.themes.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeRules {
    pub schema: String,
    pub sphere_choice_deadline_level: u16,
    pub direct_sphere_score: i32,
    pub role_sphere_score: i32,
    pub required_tag_score: i32,
    pub forbidden_tag_penalty: i32,
    pub minimum_religion_fit_score: i32,
}

impl ThemeRules {
    pub fn mud_defaults() -> Self {
        Self {
            schema: RPG_THEME_SCHEMA.to_string(),
            sphere_choice_deadline_level: 10,
            direct_sphere_score: 50,
            role_sphere_score: 15,
            required_tag_score: 10,
            forbidden_tag_penalty: 40,
            minimum_religion_fit_score: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeCatalog {
    pub schema: String,
    pub catalog_id: String,
    pub spheres: Vec<SphereDefinition>,
    pub roles: Vec<RoleThemeDefinition>,
    pub religions: Vec<ReligionThemeDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SphereDefinition {
    pub sphere_id: String,
    pub display_name: String,
    pub theme_tags: Vec<String>,
    pub affinity_sphere_ids: Vec<String>,
    pub opposing_sphere_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleThemeDefinition {
    pub role_id: String,
    pub display_name: String,
    pub primary_sphere_id: String,
    pub supporting_sphere_ids: Vec<String>,
    pub required_theme_tags: Vec<String>,
    pub optional_theme_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReligionThemeDefinition {
    pub religion_id: String,
    pub deity_id: String,
    pub deity_name: String,
    pub sphere_ids: Vec<String>,
    pub required_theme_tags: Vec<String>,
    pub forbidden_theme_tags: Vec<String>,
    pub empowerment_class_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterThemeInput {
    pub character_id: String,
    pub class_id: String,
    pub level: u16,
    pub chosen_sphere_id: Option<String>,
    pub role_id: String,
    pub desired_religion_id: Option<String>,
    pub declared_theme_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeCatalogReadback {
    pub schema: String,
    pub catalog_id: String,
    pub sphere_count: usize,
    pub role_count: usize,
    pub religion_count: usize,
    pub theme_tag_count: usize,
    pub deity_sphere_edge_count: usize,
    pub sphere_ids: Vec<String>,
    pub role_ids: Vec<String>,
    pub religion_ids: Vec<String>,
    pub input_sha256: String,
    pub result_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterThemeReadback {
    pub schema: String,
    pub character_id: String,
    pub class_id: String,
    pub level: u16,
    pub chosen_sphere_id: Option<String>,
    pub role_id: String,
    pub desired_religion_id: Option<String>,
    pub sphere_deadline_level: u16,
    pub sphere_selection_required: bool,
    pub role_sphere_match: String,
    pub role_theme_tags: Vec<String>,
    pub declared_theme_tags: Vec<String>,
    pub effective_theme_tags: Vec<String>,
    pub religion_match: Option<ReligionThemeMatchReadback>,
    pub catalog_sha256: String,
    pub input_sha256: String,
    pub rules_sha256: String,
    pub result_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReligionThemeMatchReadback {
    pub religion_id: String,
    pub deity_id: String,
    pub deity_name: String,
    pub class_requires_empowerment: bool,
    pub direct_sphere_match: bool,
    pub score: i32,
    pub grade: String,
    pub religion_fit_accepted: bool,
    pub empowerment_fit_accepted: bool,
    pub matched_sphere_ids: Vec<String>,
    pub missing_required_tags: Vec<String>,
    pub forbidden_tags_present: Vec<String>,
    pub score_breakdown: Vec<ThemeScoreComponent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeScoreComponent {
    pub component: String,
    pub delta: i32,
    pub reason: String,
}
