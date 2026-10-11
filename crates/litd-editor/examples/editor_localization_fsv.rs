use std::{collections::BTreeMap, fs, path::PathBuf};

use litd_editor::localization::{
    LocalizedStringTable, TextDirection, TranslationBundle, export_translation_bundle,
    import_translation_bundle, load_string_table, measure_text_layout, write_string_table,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_LOCALIZATION_FSV",
        "issue1219_localization",
        "manual-fsv.json",
    )?;
    let before_artifact = path_state(&path);
    let root = path
        .parent()
        .map(PathBuf::from)
        .ok_or("issue1219 FSV output path has no parent")?;
    fs::create_dir_all(&root)?;
    let table_path = root.join("string-table.json");
    let bundle_path = root.join("translations-es-ES.json");
    let imported_table_path = root.join("imported-string-table.json");

    let mut table = localized_table_with_translations()?;
    let before_locale = table.active_locale.clone();
    table.set_active_locale("es-ES")?;
    let active_readback = table.render_active("npc.elder.greeting")?;
    let mut args = BTreeMap::new();
    args.insert("hero".to_string(), "Asha".to_string());
    let formatted = table.render_locale_with_args("npc.elder.greeting", "es-ES", &args)?;
    ensure(
        active_readback.text == "La sombra se espesa, {hero}. Que buscas?",
        "active locale did not render Spanish greeting",
    )?;
    ensure(
        formatted.text == "La sombra se espesa, Asha. Que buscas?",
        "format args did not render into Spanish greeting",
    )?;
    let table_write = write_string_table(&table_path, &table)?;
    let loaded = load_string_table(&table_path)?;
    ensure(
        loaded == table,
        "loaded string table did not equal written table",
    )?;

    let exported = export_translation_bundle(&table, "es-ES")?;
    let bundle_write = write_text_file(&bundle_path, &exported)?;
    let bundle_readback = fs::read_to_string(&bundle_path)?;
    let bundle: TranslationBundle = serde_json::from_str(&bundle_readback)?;
    ensure(
        bundle.units.iter().all(|unit| {
            unit.placeholders == table.entries[&unit.key].placeholders
                && !unit.target_text.is_empty()
        }),
        "exported bundle did not preserve placeholder lists",
    )?;
    let mut imported = localized_table_defaults_only()?;
    let import_report = import_translation_bundle(&mut imported, &bundle_readback)?;
    imported.set_active_locale("es-ES")?;
    let imported_write = write_string_table(&imported_table_path, &imported)?;
    let imported_readback = load_string_table(&imported_table_path)?;
    let imported_render = imported_readback.render_active("ui.quest.accept")?;
    ensure(
        imported_render.text == "Aceptar mision: {quest}",
        "imported table did not render Spanish quest button",
    )?;

    let missing = missing_translation_edge(&table)?;
    let placeholder = placeholder_preserved_edge(&bundle, &imported_readback)?;
    let layout = rtl_and_long_layout_edge(&table)?;
    let invalid = invalid_placeholder_edge(&table)?;

    let mut snapshot = json!({
        "schema": "litd.issue1219.localization_fsv.v1",
        "issue": 1219,
        "source_of_truth": "string-table.json exact file readback plus exported translation bundle and imported table readback",
        "artifact_path": path,
        "before": {
            "artifact": before_artifact,
            "active_locale": before_locale
        },
        "happy_path": {
            "before": {"target_locale": "es-ES", "key": "npc.elder.greeting", "args": {"hero": "Asha"}},
            "after": {
                "active_render": active_readback,
                "formatted_render": formatted,
                "table_write": table_write,
                "loaded_active_locale": loaded.active_locale,
                "bundle_write": bundle_write,
                "import_report": import_report,
                "imported_write": imported_write,
                "imported_render": imported_render
            }
        },
        "edges": [missing, placeholder, layout, invalid],
        "after": {"artifact": {"exists": false, "bytes": 0}}
    });
    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote localization FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!(
        "active_locale={} active_text={} imported_text={} fallback_used={}",
        snapshot["happy_path"]["after"]["loaded_active_locale"],
        snapshot["happy_path"]["after"]["active_render"]["text"],
        snapshot["happy_path"]["after"]["imported_render"]["text"],
        snapshot["edges"][0]["after"]["fallback_used"]
    );
    Ok(())
}

fn missing_translation_edge(
    source: &LocalizedStringTable,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut table = source.clone();
    table.set_active_locale("fr-FR")?;
    let readback = table.render_active("ui.quest.accept")?;
    ensure(
        readback.fallback_used,
        "missing translation did not mark fallback",
    )?;
    ensure(
        readback
            .diagnostic
            .as_deref()
            .unwrap_or("")
            .contains("missing translation"),
        "missing translation did not include diagnostic",
    )?;
    Ok(json!({
        "edge": "missing_translation_falls_back_loudly",
        "before": {"active_locale": "fr-FR", "key": "ui.quest.accept"},
        "after": readback
    }))
}

fn placeholder_preserved_edge(
    bundle: &TranslationBundle,
    imported: &LocalizedStringTable,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let unit = bundle
        .units
        .iter()
        .find(|unit| unit.key == "npc.elder.greeting")
        .ok_or("missing greeting unit in bundle")?;
    let readback = imported.render_locale("npc.elder.greeting", "es-ES")?;
    ensure(
        unit.placeholders == vec!["hero".to_string()] && readback.placeholders == unit.placeholders,
        "placeholder list did not survive export/import",
    )?;
    Ok(json!({
        "edge": "placeholder_preserved",
        "before": {"source_text": unit.source_text, "target_text": unit.target_text},
        "after": {"bundle_placeholders": unit.placeholders, "render_placeholders": readback.placeholders}
    }))
}

fn rtl_and_long_layout_edge(
    table: &LocalizedStringTable,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let rtl = table.render_locale("npc.elder.greeting", "ar")?;
    let long = table.render_locale("ui.long.warning", "en-US")?;
    let layout = measure_text_layout(&long, 32)?;
    ensure(
        rtl.direction == TextDirection::Rtl,
        "Arabic readback was not RTL",
    )?;
    ensure(
        layout.line_count > 1 && !layout.overflow,
        "long text layout probe failed",
    )?;
    Ok(json!({
        "edge": "rtl_and_long_string_layout",
        "before": {"rtl_locale": "ar", "long_key": "ui.long.warning", "max_chars_per_line": 32},
        "after": {"rtl": rtl, "long_layout": layout}
    }))
}

fn invalid_placeholder_edge(
    source: &LocalizedStringTable,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut table = source.clone();
    let before = table.stable_fingerprint()?;
    let error = table
        .set_translation("npc.elder.greeting", "de-DE", "Die Schatten werden tiefer.")
        .unwrap_err()
        .to_string();
    let after = table.stable_fingerprint()?;
    ensure(
        before == after,
        "invalid placeholder mutation changed table fingerprint",
    )?;
    Ok(json!({
        "edge": "invalid_placeholder_rejected_atomically",
        "before": {"fingerprint": before, "attempt_locale": "de-DE"},
        "after": {"fingerprint": after, "unchanged": before == after, "error": error}
    }))
}

fn localized_table_with_translations() -> Result<LocalizedStringTable, Box<dyn std::error::Error>> {
    let mut table = localized_table_defaults_only()?;
    table.set_translation(
        "npc.elder.greeting",
        "es-ES",
        "La sombra se espesa, {hero}. Que buscas?",
    )?;
    table.set_translation("ui.quest.accept", "es-ES", "Aceptar mision: {quest}")?;
    table.set_translation(
        "ui.long.warning",
        "es-ES",
        "Esta advertencia larga del editor debe ajustarse limpiamente dentro de un campo estrecho del inspector.",
    )?;
    table.set_translation(
        "npc.elder.greeting",
        "ar",
        "ايها {hero}، الظلال تزداد عمقا. ماذا تطلب؟",
    )?;
    Ok(table)
}

fn localized_table_defaults_only() -> Result<LocalizedStringTable, Box<dyn std::error::Error>> {
    let mut table = LocalizedStringTable::new("en-US")?;
    table.upsert_default(
        "npc.elder.greeting",
        "The shadow deepens, {hero}. What do you seek?",
        "Elder Cairn opening dialogue",
    )?;
    table.upsert_default(
        "ui.quest.accept",
        "Accept quest: {quest}",
        "Quest accept button",
    )?;
    table.upsert_default(
        "ui.long.warning",
        "This is a deliberately long editor warning that must wrap cleanly inside a narrow inspector field.",
        "Long string layout probe",
    )?;
    Ok(table)
}

fn write_text_file(
    path: &PathBuf,
    text: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text.as_bytes())?;
    let readback = fs::read_to_string(path)?;
    ensure(
        readback == text,
        "text file readback did not match written text",
    )?;
    Ok(json!({"path": path, "bytes": readback.len()}))
}

fn write_document_with_final_state(
    path: &PathBuf,
    document: &mut serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut expected_len = 0_u64;
    for _ in 0..16 {
        document["after"]["artifact"] = json!({"exists": true, "bytes": expected_len});
        let bytes = serde_json::to_vec_pretty(document)?;
        let actual_len = u64::try_from(bytes.len())?;
        if actual_len == expected_len {
            fs::write(path, &bytes)?;
            let disk_len = fs::metadata(path)?.len();
            ensure(
                disk_len == expected_len,
                "FSV artifact byte readback mismatch",
            )?;
            return Ok(());
        }
        expected_len = actual_len;
    }
    Err(format!(
        "FSV artifact byte-count stabilization failed for {}",
        path.display()
    )
    .into())
}

fn path_state(path: &PathBuf) -> serde_json::Value {
    match fs::metadata(path) {
        Ok(metadata) => json!({"exists": true, "bytes": metadata.len()}),
        Err(_) => json!({"exists": false}),
    }
}

fn ensure(condition: bool, message: &str) -> Result<(), Box<dyn std::error::Error>> {
    if condition {
        Ok(())
    } else {
        Err(message.to_string().into())
    }
}
