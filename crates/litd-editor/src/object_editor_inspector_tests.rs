use super::ObjectEditorInspectorCache;
use crate::object_editor::issue732_base_document;

// #1485 finding 6: prove the inspector cache reuses the built view while the generation is
// unchanged and rebuilds (reflecting the mutation) the moment the generation advances.

#[test]
fn builds_view_for_initial_generation() {
    let document = issue732_base_document();
    let mut cache = ObjectEditorInspectorCache::default();
    assert_eq!(cache.cached_generation(), None);

    let view = cache.get_or_build(0, &document, "base.unit.footman");
    assert_eq!(view.browser_rows.len(), 3);
    assert_eq!(view.selected_display_name.as_deref(), Some("Footman"));
    let validation = view.validation.as_ref().expect("selected unit validates");
    assert_eq!(validation.object_id, "base.unit.footman");
    assert_eq!(cache.cached_generation(), Some(0));
}

#[test]
fn reuses_cached_view_when_generation_unchanged() {
    let mut document = issue732_base_document();
    let mut cache = ObjectEditorInspectorCache::default();
    let baseline_rows = cache
        .get_or_build(0, &document, "base.unit.footman")
        .browser_rows
        .len();
    assert_eq!(baseline_rows, 3);

    // Mutate the document but do NOT bump the generation. A correct cache must NOT observe
    // this change — it returns the previously built view unchanged.
    document.objects.remove("ability.issue732.defend");
    let view = cache.get_or_build(0, &document, "base.unit.footman");
    assert_eq!(
        view.browser_rows.len(),
        3,
        "stale generation must reuse the cached view"
    );
    assert_eq!(cache.cached_generation(), Some(0));
}

#[test]
fn rebuilds_when_generation_advances() {
    let mut document = issue732_base_document();
    let mut cache = ObjectEditorInspectorCache::default();
    assert_eq!(
        cache
            .get_or_build(0, &document, "base.unit.footman")
            .browser_rows
            .len(),
        3
    );

    document.objects.remove("ability.issue732.defend");
    let view = cache.get_or_build(1, &document, "base.unit.footman");
    assert_eq!(
        view.browser_rows.len(),
        2,
        "advancing the generation must rebuild the view"
    );
    assert_eq!(cache.cached_generation(), Some(1));
}

#[test]
fn missing_selection_yields_no_validation() {
    let document = issue732_base_document();
    let mut cache = ObjectEditorInspectorCache::default();
    let view = cache.get_or_build(0, &document, "does.not.exist");
    assert!(view.selected_display_name.is_none());
    assert!(view.validation.is_none());
    // The browser list is still populated over all objects even when the selection is gone.
    assert_eq!(view.browser_rows.len(), 3);
}
