use std::{fs, path::PathBuf};

use litd_editor::{
    history::{EditorDocument, EditorObject, stable_fingerprint},
    prefabs::{
        PREFAB_ROOT_FINGERPRINT_PROPERTY, PREFAB_TEMPLATE_FINGERPRINT_PROPERTY,
        PREFAB_TEMPLATE_PREFAB_ID_PROPERTY, PrefabDefinition, PrefabInstance, PrefabLibrary,
        PrefabNestedInstance, PrefabObjectTemplate, PrefabPropertyOverride, apply_prefab_instance,
        prefab_generated_object_id,
    },
};
use litd_engine::math::{Transform, Vec3};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, path) = litd_editor::fsv_support::artifact_path_from_env(
        "LITD_PREFABS_FSV",
        "issue1224_prefabs",
        "manual-fsv.json",
    )?;
    let artifact_before = path_state(&path);
    let mut document = fixture_document()?;
    let initial_document_fingerprint = stable_fingerprint(&document)?;
    let mut library = PrefabLibrary::new(4)?;
    library.insert_definition(sign_definition()?)?;
    library.insert_definition(tavern_definition("tavern_mesh_v1", "source greeting")?)?;
    let old_source = library.definitions["tavern"].content_fingerprint.clone();

    let north = library
        .instantiate(
            "north_tavern",
            "tavern",
            Transform::from_translation(Vec3::new(10.0, 0.0, 2.0)),
        )?
        .with_parent("world");
    let south = library
        .instantiate(
            "south_tavern",
            "tavern",
            Transform::from_translation(Vec3::new(-10.0, 0.0, 2.0)),
        )?
        .with_parent("world")
        .with_override(PrefabPropertyOverride::new(
            "npc",
            "dialogue",
            "local rumor",
        )?);
    let create_north = apply_prefab_instance(&mut document, &library, &north)?;
    let create_south = apply_prefab_instance(&mut document, &library, &south)?;
    let after_create = prefab_state(&document)?;

    library.insert_definition(tavern_definition("tavern_mesh_v2", "source quest")?)?;
    let update_north = apply_prefab_instance(&mut document, &library, &north)?;
    let update_south = apply_prefab_instance(&mut document, &library, &south)?;
    let after_source_edit = prefab_state(&document)?;
    let final_document_fingerprint = stable_fingerprint(&document)?;

    let nested_edge = nested_edge_state(&document)?;
    let deleted_source_edge = deleted_source_edge(&document, &north)?;
    let bound_edge = instance_bound_edge()?;

    let mut snapshot = json!({
        "schema": "litd.issue1224.prefabs_fsv.v1",
        "issue": 1224,
        "source_of_truth": "EditorDocument.objects persisted in this artifact",
        "artifact_path": path,
        "before": {
            "artifact": artifact_before,
            "document_fingerprint": initial_document_fingerprint,
            "object_ids": document.objects.keys().cloned().collect::<Vec<_>>(),
            "prefab_count": library.definitions.len(),
            "tavern_source_fingerprint_v1": old_source
        },
        "happy_path": {
            "before": {
                "instances": ["north_tavern", "south_tavern"],
                "source_mesh": "tavern_mesh_v1",
                "south_override": {"template_path": "npc", "dialogue": "local rumor"}
            },
            "after_initial_create": {
                "north_report": create_north,
                "south_report": create_south,
                "readback": after_create
            },
            "after_source_edit": {
                "north_report": update_north,
                "south_report": update_south,
                "readback": after_source_edit,
                "expected": {
                    "north_building_mesh": "tavern_mesh_v2",
                    "south_building_mesh": "tavern_mesh_v2",
                    "south_npc_dialogue": "local rumor"
                }
            }
        },
        "edges": [nested_edge, deleted_source_edge, bound_edge],
        "after": {
            "document_fingerprint": final_document_fingerprint,
            "artifact": {"exists": false, "bytes": 0}
        }
    });

    write_document_with_final_state(&path, &mut snapshot)?;
    let report_readback = fsv_run.record_json_file(&path)?;
    println!(
        "wrote prefab FSV to {} sha256={}",
        path.display(),
        report_readback.sha256
    );
    println!(
        "final readback north mesh={} south mesh={} south dialogue={}",
        object_property(&document, "north_tavern", "building", "mesh")?,
        object_property(&document, "south_tavern", "building", "mesh")?,
        object_property(&document, "south_tavern", "npc", "dialogue")?
    );
    Ok(())
}

fn prefab_state(
    document: &EditorDocument,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let keys = [
        ("north_building", "north_tavern", "building"),
        ("south_building", "south_tavern", "building"),
        ("north_npc", "north_tavern", "npc"),
        ("south_npc", "south_tavern", "npc"),
        ("north_sign", "north_tavern", "hanging_sign/sign_mesh"),
        ("south_sign", "south_tavern", "hanging_sign/sign_mesh"),
    ];
    let mut objects = serde_json::Map::new();
    for (label, instance, path) in keys {
        let id = prefab_generated_object_id(instance, path)?;
        let object = document
            .objects
            .get(&id)
            .ok_or_else(|| format!("missing generated object {id}"))?;
        objects.insert(
            label.to_string(),
            json!({
                "id": object.id,
                "kind": object.kind,
                "parent": object.parent,
                "mesh": object.properties.get("mesh"),
                "dialogue": object.properties.get("dialogue"),
                "message": object.properties.get("message"),
                "root_fingerprint": object.properties.get(PREFAB_ROOT_FINGERPRINT_PROPERTY),
                "template_prefab": object.properties.get(PREFAB_TEMPLATE_PREFAB_ID_PROPERTY),
                "template_fingerprint": object.properties.get(PREFAB_TEMPLATE_FINGERPRINT_PROPERTY),
            }),
        );
    }
    Ok(json!({"object_count": document.objects.len(), "objects": objects}))
}

fn nested_edge_state(
    document: &EditorDocument,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let sign_id = prefab_generated_object_id("north_tavern", "hanging_sign/sign_mesh")?;
    let building_id = prefab_generated_object_id("north_tavern", "building")?;
    let sign = document
        .objects
        .get(&sign_id)
        .ok_or_else(|| format!("missing nested sign {sign_id}"))?;
    Ok(json!({
        "edge": "nested_prefab",
        "before": {"nested_definition": "sign", "parent_template": "building"},
        "after": {
            "sign_id": sign_id,
            "parent": sign.parent,
            "expected_parent": building_id,
            "template_prefab": sign.properties.get(PREFAB_TEMPLATE_PREFAB_ID_PROPERTY),
            "message": sign.properties.get("message")
        }
    }))
}

fn deleted_source_edge(
    document: &EditorDocument,
    instance: &PrefabInstance,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut missing_library = PrefabLibrary::new(4)?;
    missing_library.insert_definition(sign_definition()?)?;
    let mut probe = document.clone();
    let before = stable_fingerprint(&probe)?;
    let error = apply_prefab_instance(&mut probe, &missing_library, instance)
        .unwrap_err()
        .to_string();
    let after = stable_fingerprint(&probe)?;
    Ok(json!({
        "edge": "deleted_source_instance",
        "before": {"document_fingerprint": before, "source_present": false},
        "after": {"document_fingerprint": after, "unchanged": before == after, "error": error}
    }))
}

fn instance_bound_edge() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut document = fixture_document()?;
    let mut library = PrefabLibrary::new(1)?;
    library.insert_definition(sign_definition()?)?;
    library.insert_definition(tavern_definition("tavern_mesh_v1", "source greeting")?)?;
    let first = library
        .instantiate("only_tavern", "tavern", Transform::IDENTITY)?
        .with_parent("world");
    apply_prefab_instance(&mut document, &library, &first)?;
    let before = stable_fingerprint(&document)?;
    let second = library
        .instantiate(
            "overflow_tavern",
            "tavern",
            Transform::from_translation(Vec3::new(30.0, 0.0, 0.0)),
        )?
        .with_parent("world");
    let error = apply_prefab_instance(&mut document, &library, &second)
        .unwrap_err()
        .to_string();
    let after = stable_fingerprint(&document)?;
    Ok(json!({
        "edge": "instance_count_bounded",
        "before": {"max_instances_per_prefab": 1, "document_fingerprint": before},
        "after": {"document_fingerprint": after, "unchanged": before == after, "error": error}
    }))
}

fn object_property(
    document: &EditorDocument,
    instance_id: &str,
    template_path: &str,
    key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let id = prefab_generated_object_id(instance_id, template_path)?;
    let object = document
        .objects
        .get(&id)
        .ok_or_else(|| format!("missing generated object {id}"))?;
    object
        .properties
        .get(key)
        .cloned()
        .ok_or_else(|| format!("missing property {key} on {id}").into())
}

fn fixture_document() -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = EditorDocument::new("calyx://world/prefab-fsv", "calyx-rev-prefab")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    Ok(document)
}

fn sign_definition() -> Result<PrefabDefinition, Box<dyn std::error::Error>> {
    Ok(PrefabDefinition::new(
        "sign",
        "Hanging Sign",
        "#1224",
        vec![
            PrefabObjectTemplate::new("sign_mesh", "Doodad", None, Transform::IDENTITY)
                .with_property("mesh", "sign_mesh")
                .with_property("message", "open"),
        ],
        vec![],
    )?)
}

fn tavern_definition(
    mesh: &str,
    dialogue: &str,
) -> Result<PrefabDefinition, Box<dyn std::error::Error>> {
    Ok(PrefabDefinition::new(
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
            .with_override(PrefabPropertyOverride::new(
                "sign_mesh",
                "message",
                "open",
            )?),
        ],
    )?)
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
            if disk_len != expected_len {
                return Err(format!(
                    "FSV artifact write verification failed for {}: embedded after.bytes={}, disk bytes={}",
                    path.display(),
                    expected_len,
                    disk_len
                )
                .into());
            }
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
