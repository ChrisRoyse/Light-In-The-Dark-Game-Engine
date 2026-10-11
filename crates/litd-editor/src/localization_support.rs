use std::{collections::BTreeSet, fs, path::Path};

use litd_engine::EngineError;

use super::{
    LOCALIZATION_SCHEMA_VERSION, LocalizedStringReadback, LocalizedStringTable,
    StringTableWriteReadback, TextLayoutProbe, TranslationBundle, TranslationImportReport,
    TranslationUnit, invalid_localization, invalid_localization_error, validate_locale_id,
    validate_placeholder_match, validate_text,
};

pub fn export_translation_bundle(
    table: &LocalizedStringTable,
    target_locale: &str,
) -> Result<String, EngineError> {
    table.validate()?;
    validate_locale_id("target_locale", target_locale)?;
    if target_locale == table.default_locale {
        return invalid_localization("target_locale", "target locale must differ from default");
    }
    let units = table
        .entries
        .values()
        .map(|entry| TranslationUnit {
            key: entry.key.clone(),
            description: entry.description.clone(),
            source_text: entry.default_text.clone(),
            target_text: entry
                .translations
                .get(target_locale)
                .map(|text| text.text.clone())
                .unwrap_or_default(),
            placeholders: entry.placeholders.clone(),
        })
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&TranslationBundle {
        schema_version: LOCALIZATION_SCHEMA_VERSION,
        source_locale: table.default_locale.clone(),
        target_locale: target_locale.to_string(),
        units,
    })
    .map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: error.to_string(),
    })
}

pub fn import_translation_bundle(
    table: &mut LocalizedStringTable,
    text: &str,
) -> Result<TranslationImportReport, EngineError> {
    table.validate()?;
    let bundle: TranslationBundle =
        serde_json::from_str(text).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: error.to_string(),
        })?;
    validate_bundle(table, &bundle)?;
    let before_fingerprint = table.stable_fingerprint()?;
    let mut next = table.clone();
    let mut keys = Vec::new();
    for unit in &bundle.units {
        next.set_translation(&unit.key, &bundle.target_locale, &unit.target_text)?;
        keys.push(unit.key.clone());
    }
    let after_fingerprint = next.stable_fingerprint()?;
    if before_fingerprint == after_fingerprint {
        return invalid_localization("import.no_change", "translation import changed no entries");
    }
    *table = next;
    Ok(TranslationImportReport {
        target_locale: bundle.target_locale,
        updated_count: keys.len(),
        keys,
        before_fingerprint,
        after_fingerprint,
    })
}

pub fn write_string_table(
    path: &Path,
    table: &LocalizedStringTable,
) -> Result<StringTableWriteReadback, EngineError> {
    table.validate()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_localization_error("write.parent", error))?;
    }
    let bytes =
        serde_json::to_vec_pretty(table).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: error.to_string(),
        })?;
    fs::write(path, &bytes).map_err(|error| io_localization_error("write", error))?;
    let readback = fs::read(path).map_err(|error| io_localization_error("readback", error))?;
    if readback != bytes {
        return invalid_localization(
            "write.readback",
            "string table bytes read back differently than written",
        );
    }
    Ok(StringTableWriteReadback {
        path: path.display().to_string(),
        bytes: u64::try_from(readback.len()).map_err(|error| EngineError::SerializationFailed {
            format: "usize",
            reason: error.to_string(),
        })?,
        fingerprint: table.stable_fingerprint()?,
    })
}

pub fn load_string_table(path: &Path) -> Result<LocalizedStringTable, EngineError> {
    let bytes = fs::read(path).map_err(|error| io_localization_error("load", error))?;
    let table: LocalizedStringTable =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: error.to_string(),
        })?;
    table.validate()?;
    Ok(table)
}

pub fn measure_text_layout(
    readback: &LocalizedStringReadback,
    max_chars_per_line: usize,
) -> Result<TextLayoutProbe, EngineError> {
    if max_chars_per_line == 0 {
        return invalid_localization("layout.max_chars_per_line", "must be greater than zero");
    }
    let mut line_count = 1_usize;
    let mut current = 0_usize;
    let mut max_line_chars = 0_usize;
    for word in readback.text.split_whitespace() {
        let width = word.chars().count();
        if current == 0 {
            current = width;
        } else if current + 1 + width <= max_chars_per_line {
            current += 1 + width;
        } else {
            max_line_chars = max_line_chars.max(current);
            line_count += 1;
            current = width;
        }
    }
    max_line_chars = max_line_chars.max(current);
    Ok(TextLayoutProbe {
        direction: readback.direction,
        max_chars_per_line,
        line_count,
        max_line_chars,
        overflow: max_line_chars > max_chars_per_line,
    })
}

fn validate_bundle(
    table: &LocalizedStringTable,
    bundle: &TranslationBundle,
) -> Result<(), EngineError> {
    if bundle.schema_version != LOCALIZATION_SCHEMA_VERSION {
        return invalid_localization("bundle.schema", "unexpected translation bundle schema");
    }
    if bundle.source_locale != table.default_locale {
        return invalid_localization(
            "bundle.source_locale",
            format!(
                "bundle source {} does not match table default {}",
                bundle.source_locale, table.default_locale
            ),
        );
    }
    validate_locale_id("bundle.target_locale", &bundle.target_locale)?;
    let mut seen = BTreeSet::new();
    for unit in &bundle.units {
        if !seen.insert(unit.key.clone()) {
            return invalid_localization("bundle.unit.key", format!("duplicate key {}", unit.key));
        }
        let entry = table.entries.get(&unit.key).ok_or_else(|| {
            invalid_localization_error("bundle.unit.key", format!("unknown key {}", unit.key))
        })?;
        if unit.source_text != entry.default_text || unit.placeholders != entry.placeholders {
            return invalid_localization(
                "bundle.unit.source",
                format!("source text or placeholders drifted for {}", unit.key),
            );
        }
        validate_text("bundle.unit.target_text", &unit.target_text)?;
        validate_placeholder_match(&entry.placeholders, &unit.target_text)?;
    }
    Ok(())
}

fn io_localization_error(field: &str, error: std::io::Error) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.localization.{field}"),
        reason: error.to_string(),
    }
}
