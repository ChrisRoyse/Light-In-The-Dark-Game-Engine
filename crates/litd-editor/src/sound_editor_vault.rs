use litd_engine::EngineError;

use crate::asset_vault::AssetVaultRecord;

use super::{SoundCategory, SoundLibraryEntry, import_sound_file, invalid_sound_editor};

pub fn sound_library_entry_from_vault_record(
    record: &AssetVaultRecord,
) -> Result<SoundLibraryEntry, EngineError> {
    let entry = import_sound_file(
        &record.absolute_path,
        sound_category_for_vault_record(record)?,
        sound_asset_id_for_vault_record(record),
    )?;
    if entry.sha256 != record.sha256 || entry.bytes_on_disk != record.bytes_on_disk {
        return invalid_sound_editor(
            "vault.provenance",
            format!("vault metadata mismatch for {}", record.relative_path),
        );
    }
    Ok(SoundLibraryEntry {
        source_path: record.relative_path.clone(),
        ..entry
    })
}

pub fn sound_category_for_vault_record(
    record: &AssetVaultRecord,
) -> Result<SoundCategory, EngineError> {
    if record.asset_type != "audio" {
        return invalid_sound_editor(
            "vault.asset_type",
            format!(
                "sound library requires audio asset_type, found {}",
                record.asset_type
            ),
        );
    }
    let key = format!("{} {}", record.asset_id, record.relative_path).to_ascii_lowercase();
    if key.contains("interface") || key.contains("ui-audio") {
        return Ok(SoundCategory::Interface);
    }
    if key.contains("preview") {
        return Ok(SoundCategory::Ambient);
    }
    if ["footstep", "knife", "cloth", "belt"]
        .iter()
        .any(|needle| key.contains(needle))
    {
        return Ok(SoundCategory::Units);
    }
    if ["door", "book", "coin", "metal", "pot", "creak", "chop"]
        .iter()
        .any(|needle| key.contains(needle))
    {
        return Ok(SoundCategory::Doodads);
    }
    Ok(SoundCategory::Imported)
}

fn sound_asset_id_for_vault_record(record: &AssetVaultRecord) -> String {
    format!(
        "snd_{}_{}",
        symbol_segment(&record.asset_id),
        symbol_segment(&record.file_stem())
    )
}

fn symbol_segment(value: &str) -> String {
    let mut output = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while output.contains("__") {
        output = output.replace("__", "_");
    }
    output.trim_matches('_').to_string()
}
