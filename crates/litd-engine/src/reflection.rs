//! Runtime reflection descriptors plus JSON/RON serialization with deterministic fingerprints.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::EngineError;

const DOCUMENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReflectedTypeKind {
    Component,
    Asset,
    Resource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDescriptor {
    pub name: String,
    pub type_name: String,
}

impl FieldDescriptor {
    pub fn new(name: impl Into<String>, type_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_name: type_name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDescriptor {
    pub type_name: String,
    pub kind: ReflectedTypeKind,
    pub fields: Vec<FieldDescriptor>,
}

impl TypeDescriptor {
    pub fn new(
        type_name: impl Into<String>,
        kind: ReflectedTypeKind,
        fields: Vec<FieldDescriptor>,
    ) -> Self {
        Self {
            type_name: type_name.into(),
            kind,
            fields,
        }
    }
}

pub trait ReflectType {
    fn type_descriptor() -> TypeDescriptor;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TypeRegistry {
    descriptors: BTreeMap<String, TypeDescriptor>,
}

impl TypeRegistry {
    pub fn register<T: ReflectType>(&mut self) -> Result<(), EngineError> {
        self.register_descriptor(T::type_descriptor())
    }

    pub fn register_descriptor(&mut self, descriptor: TypeDescriptor) -> Result<(), EngineError> {
        if self.descriptors.contains_key(&descriptor.type_name) {
            return Err(EngineError::ReflectionDuplicateType {
                type_name: descriptor.type_name,
            });
        }
        self.descriptors
            .insert(descriptor.type_name.clone(), descriptor);
        Ok(())
    }

    pub fn descriptor(&self, type_name: &str) -> Option<&TypeDescriptor> {
        self.descriptors.get(type_name)
    }

    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }

    pub fn type_names(&self) -> Vec<&str> {
        self.descriptors.keys().map(String::as_str).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReflectedFieldValue {
    pub name: String,
    pub type_name: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReflectedInstance {
    pub descriptor: TypeDescriptor,
    pub fields: Vec<ReflectedFieldValue>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SerializedDocument {
    pub format_version: u32,
    pub type_name: String,
    pub fingerprint: String,
    pub payload: Value,
}

pub fn inspect<T>(value: &T) -> Result<ReflectedInstance, EngineError>
where
    T: ReflectType + Serialize,
{
    let descriptor = T::type_descriptor();
    let payload = to_value(value)?;
    let object = payload
        .as_object()
        .ok_or_else(|| EngineError::SerializationFailed {
            format: "json",
            reason: format!(
                "reflected value {} must serialize to an object",
                descriptor.type_name
            ),
        })?;
    let fields = descriptor
        .fields
        .iter()
        .map(|field| ReflectedFieldValue {
            name: field.name.clone(),
            type_name: field.type_name.clone(),
            value: object.get(&field.name).cloned().unwrap_or(Value::Null),
        })
        .collect();
    Ok(ReflectedInstance {
        descriptor,
        fields,
        fingerprint: fingerprint_value(&payload)?,
    })
}

pub fn to_json_document<T>(value: &T) -> Result<String, EngineError>
where
    T: ReflectType + Serialize,
{
    let document = document_for(value)?;
    serde_json::to_string_pretty(&document).map_err(|error| serialize_error("json", error))
}

pub fn from_json_document<T>(input: &str) -> Result<T, EngineError>
where
    T: ReflectType + DeserializeOwned,
{
    let document: SerializedDocument =
        serde_json::from_str(input).map_err(|error| serialize_error("json", error))?;
    decode_document::<T>(document)
}

pub fn to_ron_document<T>(value: &T) -> Result<String, EngineError>
where
    T: ReflectType + Serialize,
{
    let document = document_for(value)?;
    ron::ser::to_string_pretty(&document, ron::ser::PrettyConfig::default())
        .map_err(|error| serialize_error("ron", error))
}

pub fn from_ron_document<T>(input: &str) -> Result<T, EngineError>
where
    T: ReflectType + DeserializeOwned,
{
    let document: SerializedDocument =
        ron::de::from_str(input).map_err(|error| serialize_error("ron", error))?;
    decode_document::<T>(document)
}

pub fn fingerprint<T: Serialize>(value: &T) -> Result<String, EngineError> {
    fingerprint_value(&to_value(value)?)
}

fn document_for<T>(value: &T) -> Result<SerializedDocument, EngineError>
where
    T: ReflectType + Serialize,
{
    let descriptor = T::type_descriptor();
    let payload = to_value(value)?;
    Ok(SerializedDocument {
        format_version: DOCUMENT_VERSION,
        type_name: descriptor.type_name,
        fingerprint: fingerprint_value(&payload)?,
        payload,
    })
}

fn decode_document<T>(document: SerializedDocument) -> Result<T, EngineError>
where
    T: ReflectType + DeserializeOwned,
{
    let expected = T::type_descriptor().type_name;
    if document.type_name != expected {
        return Err(EngineError::SerializedTypeMismatch {
            expected,
            actual: document.type_name,
        });
    }
    let actual = fingerprint_value(&document.payload)?;
    if document.fingerprint != actual {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: document.fingerprint,
            actual,
        });
    }
    serde_json::from_value(document.payload).map_err(|error| serialize_error("json", error))
}

fn to_value<T: Serialize>(value: &T) -> Result<Value, EngineError> {
    serde_json::to_value(value).map_err(|error| serialize_error("json", error))
}

fn fingerprint_value(value: &Value) -> Result<String, EngineError> {
    let bytes = serde_json::to_vec(value).map_err(|error| serialize_error("json", error))?;
    Ok(litd_hash::fnv1a64_hex(&bytes))
}

fn serialize_error(format: &'static str, error: impl std::fmt::Display) -> EngineError {
    EngineError::SerializationFailed {
        format,
        reason: error.to_string(),
    }
}

impl ReflectType for crate::math::Transform {
    fn type_descriptor() -> TypeDescriptor {
        TypeDescriptor::new(
            "litd_engine::math::Transform",
            ReflectedTypeKind::Component,
            vec![
                FieldDescriptor::new("translation", "glam::Vec3"),
                FieldDescriptor::new("rotation", "glam::Quat"),
                FieldDescriptor::new("scale", "glam::Vec3"),
            ],
        )
    }
}

impl ReflectType for crate::math::Aabb {
    fn type_descriptor() -> TypeDescriptor {
        TypeDescriptor::new(
            "litd_engine::math::Aabb",
            ReflectedTypeKind::Asset,
            vec![
                FieldDescriptor::new("min", "glam::Vec3"),
                FieldDescriptor::new("max", "glam::Vec3"),
            ],
        )
    }
}

impl ReflectType for crate::math::FixedVec3 {
    fn type_descriptor() -> TypeDescriptor {
        TypeDescriptor::new(
            "litd_engine::math::FixedVec3",
            ReflectedTypeKind::Component,
            vec![
                FieldDescriptor::new("x", "litd_engine::math::Fixed"),
                FieldDescriptor::new("y", "litd_engine::math::Fixed"),
                FieldDescriptor::new("z", "litd_engine::math::Fixed"),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Transform, Vec3};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct UnitComponent {
        name: String,
        transform: Transform,
        health: i32,
    }

    impl ReflectType for UnitComponent {
        fn type_descriptor() -> TypeDescriptor {
            TypeDescriptor::new(
                "tests::UnitComponent",
                ReflectedTypeKind::Component,
                vec![
                    FieldDescriptor::new("name", "String"),
                    FieldDescriptor::new("transform", "litd_engine::math::Transform"),
                    FieldDescriptor::new("health", "i32"),
                ],
            )
        }
    }

    #[test]
    fn registry_rejects_duplicate_type_names() {
        let mut registry = TypeRegistry::default();
        registry.register::<Transform>().unwrap();
        match registry.register::<Transform>() {
            Err(EngineError::ReflectionDuplicateType { type_name }) => {
                assert_eq!(type_name, "litd_engine::math::Transform");
            }
            other => panic!("expected ReflectionDuplicateType, got {other:?}"),
        }
    }

    #[test]
    fn inspect_and_json_roundtrip_preserve_fingerprint() {
        let unit = UnitComponent {
            name: "Ash Guard".to_string(),
            transform: Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)),
            health: 42,
        };

        let reflected = inspect(&unit).unwrap();
        let json = to_json_document(&unit).unwrap();
        let roundtrip: UnitComponent = from_json_document(&json).unwrap();

        assert_eq!(reflected.fields.len(), 3);
        assert_eq!(unit, roundtrip);
        assert_eq!(reflected.fingerprint, fingerprint(&unit).unwrap());
    }

    #[test]
    fn ron_roundtrip_and_type_mismatch_fail_loud() {
        let unit = UnitComponent {
            name: "Ash Guard".to_string(),
            transform: Transform::IDENTITY,
            health: 42,
        };
        let ron = to_ron_document(&unit).unwrap();
        let roundtrip: UnitComponent = from_ron_document(&ron).unwrap();
        assert_eq!(unit, roundtrip);

        match from_ron_document::<Transform>(&ron) {
            Err(EngineError::SerializedTypeMismatch { expected, actual }) => {
                assert_eq!(expected, "litd_engine::math::Transform");
                assert_eq!(actual, "tests::UnitComponent");
            }
            other => panic!("expected SerializedTypeMismatch, got {other:?}"),
        }
    }
}
