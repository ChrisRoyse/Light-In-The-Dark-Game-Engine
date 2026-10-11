//! Shared fixtures for the #806/#807/#811 terrain-lowering FSV harnesses.
//!
//! Every harness authors terrain into an `EditorWorldRecord`, persists it through
//! the REAL Calyx world-record path (`write_world_record` -> Aster), reads it back,
//! parses the stored bytes, and lowers THAT readback document -- so the lowered
//! runtime artifact is proven against the persisted source of truth, not an
//! in-memory value.

use std::path::Path;

use litd_editor::{
    history::{EditorDocument, stable_fingerprint},
    terrain_pathing::terrain_pathing_summary,
    world_creation::{
        EditorWorldCreationSpec, EditorWorldRecord, WorldAsterReadback, create_editor_world_record,
        parse_world_record, read_world_record, write_world_record,
    },
};

/// A square authored world of `dim` x `dim` tiles.
pub fn world_record(slug: &str, dim: u32) -> EditorWorldRecord {
    let mut spec = EditorWorldCreationSpec::issue1210_default();
    spec.calyx_document_id = format!("calyx://world/{slug}");
    spec.revision_id = format!("{slug}-rev-1");
    spec.properties.display_name = format!("Terrain lowering {slug}");
    spec.properties.description = format!("Authored terrain for {slug} lowering FSV");
    spec.properties.width_tiles = dim;
    spec.properties.height_tiles = dim;
    spec.properties.playable_bounds.max_tile_x_exclusive = dim as i32;
    spec.properties.playable_bounds.max_tile_z_exclusive = dim as i32;
    spec.properties.random_height_seed = None;
    create_editor_world_record(spec).unwrap()
}

/// Re-derive the record's cached counts/fingerprint after mutating its document so
/// `write_world_record`'s `validate_world_record` accepts it.
pub fn sync(record: &mut EditorWorldRecord, label: &str) -> Result<(), Box<dyn std::error::Error>> {
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.terrain_cell_count = record.document.terrain_cells.len();
    record.default_tile_count = record
        .document
        .terrain_cells
        .values()
        .filter(|cell| cell.tile_id == record.spec.properties.initial_tile_id)
        .count();
    record.summary = format!(
        "world={} phase={} pathing={} fingerprint={}",
        record.spec.properties.display_name,
        label,
        terrain_pathing_summary(&record.document),
        record.document_fingerprint
    );
    Ok(())
}

/// The persisted world plus the document parsed back out of the stored Aster bytes.
pub struct Persisted {
    pub readback: WorldAsterReadback,
    pub document: EditorDocument,
}

/// Persist `record` to the Aster vault dir, read it back, and parse the stored bytes.
pub fn persist_and_readback(
    aster_dir: &Path,
    record: &EditorWorldRecord,
) -> Result<Persisted, Box<dyn std::error::Error>> {
    write_world_record(aster_dir, record)?;
    let readback = read_world_record(aster_dir, &record.spec.calyx_document_id)?
        .ok_or_else(|| format!("missing Aster row for {}", record.spec.calyx_document_id))?;
    let document = parse_world_record(&readback.bytes)?.document;
    Ok(Persisted { readback, document })
}
