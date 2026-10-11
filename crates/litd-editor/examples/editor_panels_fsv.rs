use litd_engine::fsv_artifacts::env_path;
use std::{env, fs, path::PathBuf};

use litd_editor::panels::{
    EditorPanelsSnapshot, EntityKey, EntityLabel, build_calyx_content_browser_from_paths,
    build_reflection_inspector, build_scene_hierarchy_panel,
};
use litd_engine::{
    EngineError,
    calyx_paths::{canonical_gameeditor_calyx_root, validate_existing_gameeditor_calyx_path},
    ecs::{Entity, World},
    math::{Transform, Vec3},
    reflection::{FieldDescriptor, ReflectType, ReflectedTypeKind, TypeDescriptor},
    scene::SceneGraph,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
struct UnitAuthoringComponent {
    name: String,
    transform: Transform,
    health: i32,
    faction: String,
}

impl ReflectType for UnitAuthoringComponent {
    fn type_descriptor() -> TypeDescriptor {
        TypeDescriptor::new(
            "fsv::UnitAuthoringComponent",
            ReflectedTypeKind::Component,
            vec![
                FieldDescriptor::new("name", "String"),
                FieldDescriptor::new("transform", "litd_engine::math::Transform"),
                FieldDescriptor::new("health", "i32"),
                FieldDescriptor::new("faction", "String"),
            ],
        )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let stats_path = env_path(
        "LITD_EDITOR_PANELS_FSV_STATS",
        "tmp/issue1139-editor-panels-fsv.txt",
    );
    let snapshot_path = env_path(
        "LITD_EDITOR_PANELS_FSV_SNAPSHOT",
        "tmp/issue1139-editor-panels.ron",
    );

    let mut world = World::default();
    let root = entity(&mut world);
    let knight = entity(&mut world);
    let vfx = entity(&mut world);
    let mut graph = SceneGraph::default();
    graph.insert(root, Transform::from_translation(Vec3::ZERO))?;
    graph.insert(
        knight,
        Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
    )?;
    graph.insert(vfx, Transform::from_translation(Vec3::new(3.0, 2.0, -6.0)))?;
    graph.set_parent(knight, Some(root))?;
    graph.set_parent(vfx, Some(knight))?;
    graph.propagate()?;

    let hierarchy = build_scene_hierarchy_panel(
        &graph,
        &[
            label(root, "Ashen Citadel"),
            label(knight, "Unit_Knight_Captain"),
            label(vfx, "VFX_Fire_Blade"),
        ],
        Some(knight),
    )?;
    let unit = UnitAuthoringComponent {
        name: "Unit_Knight_Captain".to_string(),
        transform: graph.local_transform(knight)?,
        health: 250,
        faction: "Dawnward".to_string(),
    };
    let inspector = build_reflection_inspector(knight, "Unit_Knight_Captain", &unit)?;
    let calyx_paths = calyx_object_paths()?;
    let content_browser =
        build_calyx_content_browser_from_paths("knight fire blade", &calyx_paths)?;

    let snapshot = EditorPanelsSnapshot {
        hierarchy,
        inspector,
        content_browser,
    };
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &snapshot_path,
        ron::ser::to_string_pretty(
            &snapshot,
            ron::ser::PrettyConfig::default()
                .new_line("\n".to_string())
                .indentor("  ".to_string())
                .compact_arrays(true),
        )?,
    )?;

    let first_result = &snapshot.content_browser.results[0];
    let duplicate_count: usize = snapshot
        .content_browser
        .dedup_groups
        .iter()
        .map(|group| group.duplicate_asset_ids.len())
        .sum();
    let invalid_calyx_code = invalid_calyx_probe();
    if let Some(parent) = stats_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &stats_path,
        format!(
            "hierarchy_nodes={}\n\
             hierarchy_roots={}\n\
             hierarchy_selected={}\n\
             hierarchy_order={}\n\
             inspector_entity={}\n\
             inspector_type={}\n\
             inspector_property_count={}\n\
             inspector_fingerprint={}\n\
             inspector_properties={}\n\
             content_backend={}\n\
             content_query={}\n\
             calyx_source_count={}\n\
             calyx_result_count={}\n\
             first_result_lens_count={}\n\
             first_result_modalities={}\n\
             first_result_required_modalities={}\n\
             first_result_grounded={}\n\
             first_result_lowering_ready={}\n\
             first_result_evidence_count={}\n\
             first_result_registry_sha={}\n\
             dedup_group_count={}\n\
             dedup_duplicate_count={}\n\
             snapshot_file={}\n\
             snapshot_file_bytes={}\n\
             invalid_calyx_code={}\n\
             window_opened=false\n\
             renderer_touched=false\n",
            snapshot.hierarchy.node_count,
            snapshot.hierarchy.root_count,
            snapshot.hierarchy.selected_entity.as_deref().unwrap_or(""),
            hierarchy_order(&snapshot),
            snapshot.inspector.entity,
            snapshot.inspector.type_name,
            snapshot.inspector.properties.len(),
            snapshot.inspector.fingerprint,
            inspector_properties(&snapshot),
            snapshot.content_browser.backend,
            snapshot.content_browser.query,
            calyx_paths.len(),
            snapshot.content_browser.result_count,
            first_result.lens_hits.len(),
            snapshot.content_browser.first_lens_modalities(),
            first_result.required_modalities.join("|"),
            first_result.grounded,
            first_result.lowering_ready,
            first_result.provenance.evidence_count,
            first_result.provenance.registry_sha256,
            snapshot.content_browser.dedup_groups.len(),
            duplicate_count,
            snapshot_path.display(),
            fs::metadata(&snapshot_path)?.len(),
            invalid_calyx_code,
        ),
    )?;
    println!("wrote editor panels FSV stats to {}", stats_path.display());
    Ok(())
}

fn entity(world: &mut World) -> Entity {
    world.spawn(()).expect("empty entity spawn must work")
}

fn label(entity: Entity, label: &str) -> EntityLabel {
    EntityLabel {
        entity: EntityKey::from(entity),
        label: label.to_string(),
    }
}

fn calyx_object_paths() -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let paths = match env::var("LITD_EDITOR_PANELS_CALYX_OBJECTS") {
        Ok(value) => value.split(';').map(PathBuf::from).collect(),
        Err(_) => {
            let root = canonical_gameeditor_calyx_root()?;
            vec![
                root.join("panels/a38-bundles/objects/c59a0b53aaf0d1b8c8908381c112eb135515fb75f3f737caf080e51f37edf7a1.json"),
                root.join("panels/a38-bundles/objects/9763af8e7bd84ed4530eda20c095379147c5c9ffe5e1e1c544544fdd264e7dbf.json"),
            ]
        }
    };
    paths
        .into_iter()
        .map(|path| {
            validate_existing_gameeditor_calyx_path(&path, "editor panel Calyx object")
                .map_err(Into::into)
        })
        .collect()
}

fn hierarchy_order(snapshot: &EditorPanelsSnapshot) -> String {
    snapshot
        .hierarchy
        .nodes
        .iter()
        .map(|node| format!("{}:{}", node.depth, node.label))
        .collect::<Vec<_>>()
        .join("|")
}

fn inspector_properties(snapshot: &EditorPanelsSnapshot) -> String {
    snapshot
        .inspector
        .properties
        .iter()
        .map(|property| format!("{}={}", property.name, property.value_preview))
        .collect::<Vec<_>>()
        .join("|")
}

fn invalid_calyx_probe() -> &'static str {
    match build_calyx_content_browser_from_paths("missing", &[]) {
        Err(err @ EngineError::UiRenderInvalidConfig { .. }) => err.code(),
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}
