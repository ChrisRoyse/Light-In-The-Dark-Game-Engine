use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::history::EditorDocument;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ObjectDataKind {
    Unit,
    Hero,
    Ability,
    Item,
    Buff,
    Upgrade,
    Destructible,
    Doodad,
    GameInterface,
    GameplayConstant,
    GameplayTable,
    Tech,
    // #822 commerce records (recipes, shops/marketplaces, creep drop tables). Distinct object
    // kinds so they can be authored/copied from base data and lowered as commerce records.
    Recipe,
    Shop,
    DropTable,
}

impl ObjectDataKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unit => "Unit",
            Self::Hero => "Hero",
            Self::Ability => "Ability",
            Self::Item => "Item",
            Self::Buff => "Buff",
            Self::Upgrade => "Upgrade",
            Self::Destructible => "Destructible",
            Self::Doodad => "Doodad",
            Self::GameInterface => "GameInterface",
            Self::GameplayConstant => "GameplayConstant",
            Self::GameplayTable => "GameplayTable",
            Self::Tech => "Tech",
            Self::Recipe => "Recipe",
            Self::Shop => "Shop",
            Self::DropTable => "DropTable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectBaseTemplate {
    pub base_object_id: String,
    pub kind: ObjectDataKind,
    pub display_name: String,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectPropertyDelta {
    pub set_kind: Option<ObjectDataKind>,
    pub upsert_properties: BTreeMap<String, String>,
    pub remove_properties: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameplayTable {
    pub table_id: String,
    pub category: String,
    pub rows: Vec<GameplayTableRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameplayTableRow {
    pub row_id: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopyBaseObjectArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_document: Option<EditorDocument>,
    pub new_object_id: String,
    pub rawcode: String,
    pub base: ObjectBaseTemplate,
    pub overrides: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyObjectDeltaArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_document: Option<EditorDocument>,
    pub object_id: String,
    pub delta: ObjectPropertyDelta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpsertGameplayTablesArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_document: Option<EditorDocument>,
    pub tables: Vec<GameplayTable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadObjectDataArgs {
    pub document_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowerUnitArgs {
    pub document_path: PathBuf,
    pub artifact_path: PathBuf,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLoweredUnitArgs {
    pub artifact_path: PathBuf,
    pub expected_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowerAbilityArgs {
    pub document_path: PathBuf,
    pub artifact_path: PathBuf,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLoweredAbilityArgs {
    pub artifact_path: PathBuf,
    pub expected_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowerHeroArgs {
    pub document_path: PathBuf,
    pub artifact_path: PathBuf,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLoweredHeroArgs {
    pub artifact_path: PathBuf,
    pub expected_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowerItemArgs {
    pub document_path: PathBuf,
    pub artifact_path: PathBuf,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLoweredItemArgs {
    pub artifact_path: PathBuf,
    pub expected_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowerShopArgs {
    pub document_path: PathBuf,
    pub artifact_path: PathBuf,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLoweredShopArgs {
    pub artifact_path: PathBuf,
    pub expected_fingerprint: String,
}

/// #825: lower the whole authored tech tree (every `Tech` object) through the #829 valve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowerTechRequirementsArgs {
    pub document_path: PathBuf,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLoweredTechRequirementsArgs {
    pub artifact_path: PathBuf,
    pub expected_fingerprint: String,
}

/// #827: lower the singleton `GameplayConstant` record through the #829 valve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowerGameplayConstantsArgs {
    pub document_path: PathBuf,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadLoweredGameplayConstantsArgs {
    pub artifact_path: PathBuf,
    pub expected_fingerprint: String,
}
