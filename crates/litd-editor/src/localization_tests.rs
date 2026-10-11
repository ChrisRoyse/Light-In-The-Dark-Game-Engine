use super::*;

fn table_fixture() -> LocalizedStringTable {
    let mut table = LocalizedStringTable::new("en-US").unwrap();
    table
        .upsert_default(
            "npc.elder.greeting",
            "The shadow deepens, {hero}. What do you seek?",
            "Elder Cairn opening dialogue",
        )
        .unwrap();
    table
        .upsert_default(
            "ui.quest.accept",
            "Accept quest: {quest}",
            "Quest accept button",
        )
        .unwrap();
    table
        .set_translation(
            "npc.elder.greeting",
            "es-ES",
            "La sombra se espesa, {hero}. Que buscas?",
        )
        .unwrap();
    table
}

#[test]
fn string_table_switches_locale_and_formats_known_args() {
    let mut table = table_fixture();
    table.set_active_locale("es-ES").unwrap();
    let readback = table.render_active("npc.elder.greeting").unwrap();
    let mut args = BTreeMap::new();
    args.insert("hero".to_string(), "Asha".to_string());
    let formatted = table
        .render_locale_with_args("npc.elder.greeting", "es-ES", &args)
        .unwrap();

    assert_eq!(readback.resolved_locale, "es-ES");
    assert_eq!(readback.direction, TextDirection::Ltr);
    assert!(!readback.fallback_used);
    assert_eq!(readback.placeholders, vec!["hero".to_string()]);
    assert_eq!(
        formatted.text,
        "La sombra se espesa, Asha. Que buscas?".to_string()
    );
}

#[test]
fn export_import_translation_round_trips_placeholders_and_file_readback() {
    let mut source = table_fixture();
    source
        .set_translation("ui.quest.accept", "es-ES", "Aceptar mision: {quest}")
        .unwrap();
    let exported = export_translation_bundle(&source, "es-ES").unwrap();
    let mut imported = LocalizedStringTable::new("en-US").unwrap();
    imported
        .upsert_default(
            "npc.elder.greeting",
            "The shadow deepens, {hero}. What do you seek?",
            "Elder Cairn opening dialogue",
        )
        .unwrap();
    imported
        .upsert_default(
            "ui.quest.accept",
            "Accept quest: {quest}",
            "Quest accept button",
        )
        .unwrap();

    let report = import_translation_bundle(&mut imported, &exported).unwrap();
    imported.set_active_locale("es-ES").unwrap();
    // #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
    let _temp_root = litd_engine::test_temp::TestTempDir::new("editor-issue1219-string-table");
    let path = _temp_root.path().join("string-table.json");
    let write = write_string_table(&path, &imported).unwrap();
    let loaded = load_string_table(&path).unwrap();

    assert_eq!(report.updated_count, 2);
    assert_ne!(report.before_fingerprint, report.after_fingerprint);
    assert_eq!(write.bytes, std::fs::metadata(&path).unwrap().len());
    assert_eq!(loaded, imported);
    assert_eq!(
        loaded.render_active("ui.quest.accept").unwrap().text,
        "Aceptar mision: {quest}"
    );
}

#[test]
fn missing_translation_falls_back_loudly_without_silent_success() {
    let mut table = table_fixture();
    table.set_active_locale("fr-FR").unwrap();

    let readback = table.render_active("ui.quest.accept").unwrap();

    assert_eq!(readback.text, "Accept quest: {quest}");
    assert_eq!(readback.resolved_locale, "en-US");
    assert!(readback.fallback_used);
    assert!(readback.diagnostic.unwrap().contains("missing translation"));
}

#[test]
fn placeholder_mismatch_and_invalid_locale_fail_without_mutating() {
    let mut table = table_fixture();
    let before = table.stable_fingerprint().unwrap();
    let placeholder_error = table
        .set_translation("npc.elder.greeting", "de-DE", "Die Schatten werden tiefer.")
        .unwrap_err();
    let locale_error = table.set_active_locale("de_DE").unwrap_err();

    assert_eq!(placeholder_error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert!(
        placeholder_error
            .to_string()
            .contains("placeholder mismatch")
    );
    assert!(locale_error.to_string().contains("BCP-47"));
    assert_eq!(table.stable_fingerprint().unwrap(), before);
}

#[test]
fn rtl_and_long_text_layout_are_explicit_readbacks() {
    let mut table = table_fixture();
    table
        .set_translation(
            "npc.elder.greeting",
            "ar",
            "ايها {hero}، الظلال تزداد عمقا. ماذا تطلب؟",
        )
        .unwrap();
    table
        .upsert_default(
            "ui.long.warning",
            "This is a deliberately long editor warning that must wrap cleanly inside a narrow inspector field.",
            "Long string layout probe",
        )
        .unwrap();
    let rtl = table.render_locale("npc.elder.greeting", "ar").unwrap();
    let long = table.render_locale("ui.long.warning", "en-US").unwrap();
    let layout = measure_text_layout(&long, 32).unwrap();

    assert_eq!(rtl.direction, TextDirection::Rtl);
    assert!(layout.line_count > 1);
    assert!(!layout.overflow);
    assert_eq!(layout.direction, TextDirection::Ltr);
}
