//! Public #829 lowering entry points for the #823/#824/#826/#828 object-data editors. Each
//! `lower_*_from_document_path` reads the persisted (Aster) document from disk -- never caller
//! memory -- extracts + validates the domain record, and fingerprints it; each write/read pair
//! is a thin instantiation of the generic valve in `object_editor_lower`.

use std::path::Path;

use litd_engine::EngineError;

use super::buff_fields::{ObjectEditorLoweredBuffDocument, lowered_buff_document};
use super::interface_fields::{ObjectEditorLoweredInterfaceDocument, lowered_interface_document};
use super::lower::{
    LoweredToml, LoweredTomlReadback, finalize_lowering, read_lowered, write_lowered,
};
use super::scatter_fields::{ObjectEditorLoweredScatterDocument, lowered_scatter_document};
use super::upgrade_fields::{ObjectEditorLoweredUpgradeDocument, lowered_upgrade_document};
use crate::mcp_object_tools::read_mcp_object_document;

macro_rules! p2_lowering_api {
    ($lower_fn:ident, $write_fn:ident, $read_fn:ident, $build:ident, $doc:ty) => {
        pub fn $lower_fn(
            document_path: impl AsRef<Path>,
            object_id: &str,
        ) -> Result<LoweredToml, EngineError> {
            let readback = read_mcp_object_document(document_path)?;
            let document = $build(&readback.document, object_id)?;
            finalize_lowering(document)
        }

        pub fn $write_fn(
            path: impl AsRef<Path>,
            artifact: &LoweredToml,
        ) -> Result<LoweredTomlReadback<$doc>, EngineError> {
            write_lowered(path, artifact)
        }

        pub fn $read_fn(
            path: impl AsRef<Path>,
            expected_fingerprint: &str,
        ) -> Result<LoweredTomlReadback<$doc>, EngineError> {
            read_lowered(path, expected_fingerprint)
        }
    };
}

p2_lowering_api!(
    lower_buff_from_document_path,
    write_lowered_buff_toml,
    read_lowered_buff_toml,
    lowered_buff_document,
    ObjectEditorLoweredBuffDocument
);
p2_lowering_api!(
    lower_upgrade_from_document_path,
    write_lowered_upgrade_toml,
    read_lowered_upgrade_toml,
    lowered_upgrade_document,
    ObjectEditorLoweredUpgradeDocument
);
p2_lowering_api!(
    lower_scatter_from_document_path,
    write_lowered_scatter_toml,
    read_lowered_scatter_toml,
    lowered_scatter_document,
    ObjectEditorLoweredScatterDocument
);
p2_lowering_api!(
    lower_interface_from_document_path,
    write_lowered_interface_toml,
    read_lowered_interface_toml,
    lowered_interface_document,
    ObjectEditorLoweredInterfaceDocument
);
