use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::{Fixed, Transform, Vec3},
    reflection::{
        FieldDescriptor, ReflectType, ReflectedTypeKind, TypeDescriptor, TypeRegistry,
        from_json_document, from_ron_document, inspect, to_json_document, to_ron_document,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct UnitComponent {
    name: String,
    transform: Transform,
    health: i32,
}

impl ReflectType for UnitComponent {
    fn type_descriptor() -> TypeDescriptor {
        TypeDescriptor::new(
            "fsv::UnitComponent",
            ReflectedTypeKind::Component,
            vec![
                FieldDescriptor::new("name", "String"),
                FieldDescriptor::new("transform", "litd_engine::math::Transform"),
                FieldDescriptor::new("health", "i32"),
            ],
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct AbilityAsset {
    id: String,
    cooldown: Fixed,
}

impl ReflectType for AbilityAsset {
    fn type_descriptor() -> TypeDescriptor {
        TypeDescriptor::new(
            "fsv::AbilityAsset",
            ReflectedTypeKind::Asset,
            vec![
                FieldDescriptor::new("id", "String"),
                FieldDescriptor::new("cooldown", "litd_engine::math::Fixed"),
            ],
        )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_REFLECTION_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1084-reflection-fsv.txt"));

    let mut registry = TypeRegistry::default();
    registry.register::<UnitComponent>()?;
    registry.register::<AbilityAsset>()?;
    registry.register::<Transform>()?;

    let unit = UnitComponent {
        name: "Ash Guard".to_string(),
        transform: Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)),
        health: 42,
    };
    let reflected = inspect(&unit)?;
    let json = to_json_document(&unit)?;
    let ron = to_ron_document(&unit)?;
    let json_roundtrip: UnitComponent = from_json_document(&json)?;
    let ron_roundtrip: UnitComponent = from_ron_document(&ron)?;
    let mismatch_code = match from_json_document::<AbilityAsset>(&json) {
        Err(err @ EngineError::SerializedTypeMismatch { .. }) => err.code(),
        other => panic!("expected SerializedTypeMismatch, got {other:?}"),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "registered_types={}\n\
             first_registered_type={}\n\
             unit_field_count={}\n\
             fingerprint={}\n\
             json_contains_type={}\n\
             ron_contains_type={}\n\
             json_roundtrip_equal={}\n\
             ron_roundtrip_equal={}\n\
             transform_descriptor_fields={}\n\
             mismatch_error_code={}\n",
            registry.len(),
            registry.type_names()[0],
            reflected.fields.len(),
            reflected.fingerprint,
            json.contains("\"type_name\": \"fsv::UnitComponent\""),
            ron.contains("type_name: \"fsv::UnitComponent\""),
            json_roundtrip == unit,
            ron_roundtrip == unit,
            registry
                .descriptor("litd_engine::math::Transform")
                .map(|descriptor| descriptor.fields.len())
                .unwrap_or_default(),
            mismatch_code,
        ),
    )?;
    println!("wrote reflection FSV stats to {}", path.display());
    Ok(())
}
