use super::*;
use litd_engine::{
    ecs::World,
    math::{Transform, Vec3},
    reflection::{FieldDescriptor, ReflectedTypeKind, TypeDescriptor},
};

#[derive(Debug, Clone, Serialize)]
struct UnitAuthoring {
    name: String,
    transform: Transform,
    health: i32,
}

impl ReflectType for UnitAuthoring {
    fn type_descriptor() -> TypeDescriptor {
        TypeDescriptor::new(
            "tests::UnitAuthoring",
            ReflectedTypeKind::Component,
            vec![
                FieldDescriptor::new("name", "String"),
                FieldDescriptor::new("transform", "litd_engine::math::Transform"),
                FieldDescriptor::new("health", "i32"),
            ],
        )
    }
}

fn entity(world: &mut World) -> Entity {
    world.spawn(()).unwrap()
}

#[test]
fn hierarchy_panel_follows_scene_tree_order() {
    let mut world = World::default();
    let root = entity(&mut world);
    let child = entity(&mut world);
    let grandchild = entity(&mut world);
    let mut graph = SceneGraph::default();
    graph.insert(root, Transform::IDENTITY).unwrap();
    graph.insert(child, Transform::IDENTITY).unwrap();
    graph.insert(grandchild, Transform::IDENTITY).unwrap();
    graph.set_parent(child, Some(root)).unwrap();
    graph.set_parent(grandchild, Some(child)).unwrap();
    graph.propagate().unwrap();

    let panel = build_scene_hierarchy_panel(
        &graph,
        &[
            label(root, "World"),
            label(child, "Knight"),
            label(grandchild, "Sword Glow"),
        ],
        Some(child),
    )
    .unwrap();

    assert_eq!(panel.node_count, 3);
    assert_eq!(panel.nodes[0].label, "World");
    assert_eq!(panel.nodes[1].label, "Knight");
    assert_eq!(panel.nodes[1].depth, 1);
    assert!(panel.nodes[1].selected);
    assert_eq!(panel.nodes[2].depth, 2);
}

#[test]
fn inspector_panel_uses_reflection_fields() {
    let mut world = World::default();
    let unit = entity(&mut world);
    let component = UnitAuthoring {
        name: "Knight".to_string(),
        transform: Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)),
        health: 250,
    };

    let panel = build_reflection_inspector(unit, "Knight", &component).unwrap();

    assert_eq!(panel.type_name, "tests::UnitAuthoring");
    assert_eq!(panel.properties.len(), 3);
    assert_eq!(panel.properties[0].name, "name");
    assert_eq!(panel.properties[0].value_preview, "Knight");
    assert!(!panel.fingerprint.is_empty());
}

#[test]
fn content_browser_requires_grounded_calyx_lens_panel_and_dedups() {
    let panel = build_calyx_content_browser(
        "knight".to_string(),
        vec![calyx_panel("first", 10), calyx_panel("second", 10)],
    )
    .unwrap();

    assert_eq!(panel.result_count, 2);
    assert_eq!(panel.results[0].lens_hits.len(), 10);
    assert!(panel.results[0].grounded);
    assert!(panel.results[0].lowering_ready);
    assert_eq!(panel.dedup_groups.len(), 1);
    assert_eq!(panel.dedup_groups[0].duplicate_asset_ids.len(), 1);
}

#[test]
fn content_browser_rejects_under_covered_calyx_result() {
    match build_calyx_content_browser("bad".to_string(), vec![calyx_panel("bad", 9)]) {
        Err(EngineError::UiRenderInvalidConfig { field, .. }) => {
            assert!(field.contains("content_browser.lenses"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
}

fn label(entity: Entity, label: &str) -> EntityLabel {
    EntityLabel {
        entity: EntityKey::from(entity),
        label: label.to_string(),
    }
}

fn calyx_panel(name: &str, lens_count: usize) -> CalyxPanelObject {
    CalyxPanelObject {
        // #1559: route the fixture path through the canonical Calyx root const (#779) instead of
        // a hardcoded machine-specific checkout-root literal.
        source_path: std::path::Path::new(litd_engine::calyx_paths::GAMEEDITOR_CALYX_ROOT)
            .join("test-fixtures")
            .join(format!("{name}.json"))
            .display()
            .to_string(),
        schema_version: 1,
        name: name.to_string(),
        base_template: CalyxBaseTemplate {
            name: "constellation-24".to_string(),
            template_id: "template-shared".to_string(),
            content_lens_count: 10,
            a37_gate_eligible: true,
            a37_status: "gate_passed".to_string(),
        },
        registry_ref: CalyxRegistryRef {
            path: std::path::Path::new(litd_engine::calyx_paths::GAMEEDITOR_CALYX_ROOT)
                .join("lenses")
                .join("registry.json")
                .display()
                .to_string(),
            sha256_hex: "abc123".to_string(),
            size_bytes: 123,
            lens_count,
        },
        required_modalities: vec!["text".to_string(), "image".to_string(), "audio".to_string()],
        evidence_refs: vec![CalyxEvidenceRef {
            path: "evidence.json".to_string(),
            sha256_hex: "def456".to_string(),
            size_bytes: 456,
        }],
        lenses: (0..lens_count)
            .map(|index| CalyxLensRecord {
                lens_id: format!("lens-{index}"),
                name: format!("Lens {index}"),
                modality: if index % 3 == 0 {
                    "image"
                } else if index % 3 == 1 {
                    "text"
                } else {
                    "audio"
                }
                .to_string(),
                manifest: format!("lens-{index}.json"),
            })
            .collect(),
    }
}
