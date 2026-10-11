use super::*;

#[test]
fn gallery_ships_melee_and_story_templates() {
    let templates = list_templates().unwrap();
    let ids: Vec<&str> = templates.iter().map(|t| t.template_id.as_str()).collect();
    assert!(ids.contains(&MELEE_SKIRMISH_TEMPLATE));
    assert!(ids.contains(&STORY_CINEMATIC_TEMPLATE));
}

#[test]
fn melee_template_has_two_starts_and_regions() {
    let summary = template_summary(MELEE_SKIRMISH_TEMPLATE).unwrap();
    assert_eq!(summary.start_location_count, 2);
    // Two authored base regions plus the auto-injected playable_bounds region.
    assert_eq!(summary.region_count, 3);
    assert_eq!(summary.named_camera_count, 2);
    assert_eq!(summary.cinematic_timeline_count, 0);
    let document = build_template_document(MELEE_SKIRMISH_TEMPLATE).unwrap();
    assert!(document.regions.contains_key("region.player_one_base"));
    assert!(document.regions.contains_key("region.player_two_base"));
}

#[test]
fn story_template_ships_a_real_cinematic() {
    let document = build_template_document(STORY_CINEMATIC_TEMPLATE).unwrap();
    assert_eq!(document.cinematic_timelines.len(), 1);
    let timeline = &document.cinematic_timelines["cine.intro"];
    assert_eq!(timeline.camera_keys.len(), 2);
    // Every camera the cutscene references really exists on the template document.
    for key in &timeline.camera_keys {
        assert!(document.named_cameras.contains_key(&key.camera_ref));
    }
    document.validate().unwrap();
}

#[test]
fn clone_gets_fresh_identity_and_leaves_source_untouched() {
    let source = build_template_document(MELEE_SKIRMISH_TEMPLATE).unwrap();
    let source_id_before = source.calyx_document_id.clone();
    let mut clone =
        clone_template_document(&source, "calyx://project/my-melee", "my-melee-rev-1").unwrap();
    assert_eq!(clone.calyx_document_id, "calyx://project/my-melee");
    assert_ne!(clone.calyx_document_id, source.calyx_document_id);
    // Mutating the clone does not touch the in-memory source.
    clone
        .upsert_region(EditorRegion::new(
            "region.extra",
            "Extra",
            Vec3::new(4.0, 0.0, 4.0),
            Vec3::new(1.0, 4.0, 1.0),
        ))
        .unwrap();
    assert!(clone.regions.contains_key("region.extra"));
    assert!(!source.regions.contains_key("region.extra"));
    assert_eq!(source.calyx_document_id, source_id_before);
}

#[test]
fn unknown_template_fails_loud() {
    assert!(build_template_document("template.does.not.exist").is_err());
    assert!(template_summary("template.does.not.exist").is_err());
}

#[test]
fn empty_clone_identity_is_rejected() {
    let source = build_template_document(MELEE_SKIRMISH_TEMPLATE).unwrap();
    assert!(clone_template_document(&source, "", "rev").is_err());
    assert!(clone_template_document(&source, "id", "  ").is_err());
}
