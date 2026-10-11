use super::*;

use crate::history::{EditorDocument, EditorObject, stable_fingerprint};
use litd_engine::math::{Transform, Vec3};

#[test]
fn prefab_instances_propagate_source_edits_and_keep_overrides() {
    let mut document = fixture_document();
    let mut library = PrefabLibrary::new(4).unwrap();
    library.insert_definition(sign_definition()).unwrap();
    library
        .insert_definition(tavern_definition("tavern_mesh_v1", "source greeting"))
        .unwrap();
    let old_source = library
        .definition("tavern")
        .unwrap()
        .content_fingerprint
        .clone();
    let first = library
        .instantiate(
            "north_tavern",
            "tavern",
            Transform::from_translation(Vec3::new(10.0, 0.0, 2.0)),
        )
        .unwrap()
        .with_parent("world");
    let second = library
        .instantiate(
            "south_tavern",
            "tavern",
            Transform::from_translation(Vec3::new(-10.0, 0.0, 2.0)),
        )
        .unwrap()
        .with_parent("world")
        .with_override(PrefabPropertyOverride::new("npc", "dialogue", "local rumor").unwrap());

    apply_prefab_instance(&mut document, &library, &first).unwrap();
    apply_prefab_instance(&mut document, &library, &second).unwrap();
    let north_building = prefab_generated_object_id("north_tavern", "building").unwrap();
    let south_building = prefab_generated_object_id("south_tavern", "building").unwrap();
    let south_npc = prefab_generated_object_id("south_tavern", "npc").unwrap();
    assert_eq!(
        document.objects[&north_building].properties.get("mesh"),
        Some(&"tavern_mesh_v1".to_string())
    );

    library
        .insert_definition(tavern_definition("tavern_mesh_v2", "source quest"))
        .unwrap();
    let north_report = apply_prefab_instance(&mut document, &library, &first).unwrap();
    let south_report = apply_prefab_instance(&mut document, &library, &second).unwrap();

    assert!(north_report.source_changed);
    assert_eq!(north_report.previous_source_fingerprint, old_source);
    assert!(south_report.source_changed);
    assert_eq!(south_report.preserved_override_count, 1);
    assert_eq!(
        document.objects[&north_building].properties.get("mesh"),
        Some(&"tavern_mesh_v2".to_string())
    );
    assert_eq!(
        document.objects[&south_building].properties.get("mesh"),
        Some(&"tavern_mesh_v2".to_string())
    );
    assert_eq!(
        document.objects[&south_npc].properties.get("dialogue"),
        Some(&"local rumor".to_string())
    );
    assert_eq!(
        document.objects[&north_building]
            .properties
            .get(PREFAB_ROOT_FINGERPRINT_PROPERTY),
        Some(&north_report.source_fingerprint)
    );
    document.validate().unwrap();
}

#[test]
fn nested_prefab_keeps_its_own_source_link_and_parentage() {
    let mut document = fixture_document();
    let mut library = PrefabLibrary::new(2).unwrap();
    library.insert_definition(sign_definition()).unwrap();
    library
        .insert_definition(tavern_definition("tavern_mesh_v1", "source greeting"))
        .unwrap();
    let instance = library
        .instantiate("nested_tavern", "tavern", Transform::IDENTITY)
        .unwrap()
        .with_parent("world");
    apply_prefab_instance(&mut document, &library, &instance).unwrap();

    let building = prefab_generated_object_id("nested_tavern", "building").unwrap();
    let sign = prefab_generated_object_id("nested_tavern", "hanging_sign/sign_mesh").unwrap();
    assert_eq!(document.objects[&sign].parent, Some(building));
    assert_eq!(
        document.objects[&sign]
            .properties
            .get(PREFAB_TEMPLATE_PREFAB_ID_PROPERTY),
        Some(&"sign".to_string())
    );
    assert_eq!(
        document.objects[&sign].properties.get("message"),
        Some(&"open".to_string())
    );
}

#[test]
fn missing_source_stale_new_instance_and_instance_bound_fail_atomically() {
    let mut document = fixture_document();
    let mut library = PrefabLibrary::new(1).unwrap();
    library.insert_definition(sign_definition()).unwrap();
    library
        .insert_definition(tavern_definition("tavern_mesh_v1", "source greeting"))
        .unwrap();
    let first = library
        .instantiate("first_tavern", "tavern", Transform::IDENTITY)
        .unwrap()
        .with_parent("world");
    apply_prefab_instance(&mut document, &library, &first).unwrap();

    let before_bound = stable_fingerprint(&document).unwrap();
    let second = library
        .instantiate(
            "second_tavern",
            "tavern",
            Transform::from_translation(Vec3::new(12.0, 0.0, 0.0)),
        )
        .unwrap()
        .with_parent("world");
    let bound_error = apply_prefab_instance(&mut document, &library, &second).unwrap_err();
    assert_eq!(bound_error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert_eq!(stable_fingerprint(&document).unwrap(), before_bound);
    assert!(
        !document
            .objects
            .contains_key(&prefab_generated_object_id("second_tavern", "building").unwrap())
    );

    let mut missing_library = PrefabLibrary::new(1).unwrap();
    missing_library
        .insert_definition(sign_definition())
        .unwrap();
    let before_missing = stable_fingerprint(&document).unwrap();
    let missing_error = apply_prefab_instance(&mut document, &missing_library, &first).unwrap_err();
    assert!(
        missing_error
            .to_string()
            .contains("missing prefab definition tavern")
    );
    assert_eq!(stable_fingerprint(&document).unwrap(), before_missing);

    let mut edited_library = PrefabLibrary::new(2).unwrap();
    edited_library.insert_definition(sign_definition()).unwrap();
    edited_library
        .insert_definition(tavern_definition("tavern_mesh_v2", "source quest"))
        .unwrap();
    let before_stale = stable_fingerprint(&document).unwrap();
    let stale_new = PrefabInstance {
        instance_id: "fresh_stale_tavern".to_string(),
        source_fingerprint: first.source_fingerprint.clone(),
        ..first.clone()
    };
    let stale_error =
        apply_prefab_instance(&mut document, &edited_library, &stale_new).unwrap_err();
    assert!(stale_error.to_string().contains("uses stale source"));
    assert_eq!(stable_fingerprint(&document).unwrap(), before_stale);
}

fn fixture_document() -> EditorDocument {
    let mut document = EditorDocument::new("calyx://world/prefab-fsv", "calyx-rev-prefab").unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
}

fn sign_definition() -> PrefabDefinition {
    PrefabDefinition::new(
        "sign",
        "Hanging Sign",
        "#1224",
        vec![
            PrefabObjectTemplate::new("sign_mesh", "Doodad", None, Transform::IDENTITY)
                .with_property("mesh", "sign_mesh")
                .with_property("message", "open"),
        ],
        vec![],
    )
    .unwrap()
}

fn tavern_definition(mesh: &str, dialogue: &str) -> PrefabDefinition {
    PrefabDefinition::new(
        "tavern",
        "Reusable Tavern",
        "#1224",
        vec![
            PrefabObjectTemplate::new("building", "Doodad", None, Transform::IDENTITY)
                .with_property("mesh", mesh)
                .with_property("hit_points", "900"),
            PrefabObjectTemplate::new(
                "npc",
                "Unit",
                Some("building".to_string()),
                Transform::from_translation(Vec3::new(1.0, 0.0, 2.0)),
            )
            .with_property("name", "Innkeeper")
            .with_property("dialogue", dialogue),
            PrefabObjectTemplate::new(
                "trigger",
                "Trigger",
                Some("building".to_string()),
                Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)),
            )
            .with_property("script", "open_shop"),
        ],
        vec![
            PrefabNestedInstance::new(
                "hanging_sign",
                "sign",
                Some("building".to_string()),
                Transform::from_translation(Vec3::new(0.0, 3.0, -1.0)),
            )
            .with_override(PrefabPropertyOverride::new("sign_mesh", "message", "open").unwrap()),
        ],
    )
    .unwrap()
}
