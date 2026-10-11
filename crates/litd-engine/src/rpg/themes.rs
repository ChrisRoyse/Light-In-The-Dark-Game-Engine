use crate::EngineError;

use super::{
    CharacterThemeInput, CharacterThemeReadback, ThemeCatalog, ThemeCatalogReadback, ThemeRules,
    themes_readback::{
        build_character_theme_readback, build_theme_catalog_readback as build_theme_catalog,
    },
    themes_validate::{
        validate_character_theme_input, validate_theme_catalog, validate_theme_rules,
    },
};

pub fn build_theme_catalog_readback(
    catalog: &ThemeCatalog,
) -> Result<ThemeCatalogReadback, EngineError> {
    validate_theme_catalog(catalog)?;
    build_theme_catalog(catalog)
}

pub fn evaluate_character_theme(
    input: &CharacterThemeInput,
    catalog: &ThemeCatalog,
    rules: &ThemeRules,
) -> Result<CharacterThemeReadback, EngineError> {
    validate_theme_rules(rules)?;
    let indexes = validate_theme_catalog(catalog)?;
    validate_character_theme_input(input, rules, &indexes)?;
    let catalog_readback = build_theme_catalog(catalog)?;
    build_character_theme_readback(input, catalog, rules, &catalog_readback)
}
