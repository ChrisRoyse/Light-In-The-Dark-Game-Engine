#[path = "starter_templates_builder_arena.rs"]
mod arena;
#[path = "starter_templates_builder_common.rs"]
mod common;
#[path = "starter_templates_builder_genres.rs"]
mod genres;
#[path = "starter_templates_builder_genres_common.rs"]
mod genres_common;
#[path = "starter_templates_builder_melee.rs"]
mod melee;
#[path = "starter_templates_builder_tower.rs"]
mod tower;

use litd_engine::EngineError;

use crate::starter_templates::StarterTemplateDefinition;

pub(super) fn rpg_arena_template() -> Result<StarterTemplateDefinition, EngineError> {
    arena::rpg_arena_template()
}

pub(super) fn orpg_template() -> Result<StarterTemplateDefinition, EngineError> {
    genres::orpg_template()
}

pub(super) fn moba_aos_template() -> Result<StarterTemplateDefinition, EngineError> {
    genres::moba_aos_template()
}

pub(super) fn hero_defense_template() -> Result<StarterTemplateDefinition, EngineError> {
    genres::hero_defense_template()
}

pub(super) fn hero_arena_template() -> Result<StarterTemplateDefinition, EngineError> {
    genres::hero_arena_template()
}

pub(super) fn survival_chaos_template() -> Result<StarterTemplateDefinition, EngineError> {
    genres::survival_chaos_template()
}

pub(super) fn mini_game_template() -> Result<StarterTemplateDefinition, EngineError> {
    genres::mini_game_template()
}

pub(super) fn maze_escape_template() -> Result<StarterTemplateDefinition, EngineError> {
    genres::maze_escape_template()
}

pub(super) fn melee_skirmish_template() -> Result<StarterTemplateDefinition, EngineError> {
    melee::melee_skirmish_template()
}

pub(super) fn tower_defense_template() -> Result<StarterTemplateDefinition, EngineError> {
    tower::tower_defense_template()
}
