use crate::EngineError;

use super::{
    abilities_readback::build_ability_readback,
    abilities_types::{AbilityCatalog, AbilityCatalogReadback},
    abilities_validate::validate_ability_catalog,
};

pub fn build_ability_catalog_readback(
    catalog: &AbilityCatalog,
) -> Result<AbilityCatalogReadback, EngineError> {
    let validation = validate_ability_catalog(catalog)?;
    build_ability_readback(catalog, validation.max_prerequisite_depth)
}
