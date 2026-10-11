use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path},
};

use litd_engine::{EngineError, math::Vec3};

const DEGENERATE_TRIANGLE_AREA_EPSILON_SQUARED: f32 = 1.0e-20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GltfValidationReport {
    pub mesh_count: usize,
    pub primitive_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub degenerate_triangle_count: usize,
    pub buffer_count: usize,
    pub image_count: usize,
    pub dependencies: Vec<String>,
}

pub fn validate_gltf_source(
    source_absolute_path: &Path,
    source_relative_path: &str,
) -> Result<GltfValidationReport, EngineError> {
    let extension = source_absolute_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(extension.as_deref(), Some("gltf" | "glb")) {
        return invalid_gltf("source.extension", "expected .gltf or .glb source");
    }
    let preflight = gltf::Gltf::open(source_absolute_path).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.asset_product.gltf.parse".to_string(),
            reason: format!(
                "{} failed glTF metadata parse: {error}",
                source_absolute_path.display()
            ),
        }
    })?;
    let required_extensions = preflight.document.extensions_required().collect::<Vec<_>>();
    if !required_extensions.is_empty() {
        return invalid_gltf(
            "extensions_required",
            format!(
                "core glTF importer rejects required extensions: {}",
                required_extensions.join("|")
            ),
        );
    }
    let dependencies = collect_dependencies(
        &preflight.document,
        source_absolute_path,
        source_relative_path,
    )?;
    let (document, buffers, images) =
        gltf::import(source_absolute_path).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_product.gltf.import".to_string(),
            reason: format!(
                "{} failed glTF import: {error}",
                source_absolute_path.display()
            ),
        })?;
    let mut primitive_count = 0;
    let mut vertex_count = 0;
    let mut index_count = 0;
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return invalid_gltf(
                    "primitive.mode",
                    format!(
                        "{source_relative_path} mesh {} primitive {} uses unsupported mode {:?}; editor imports require triangle primitives",
                        mesh.index(),
                        primitive.index(),
                        primitive.mode()
                    ),
                );
            }
            primitive_count += 1;
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let positions =
                reader
                    .read_positions()
                    .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                        field: "editor.asset_product.gltf.positions".to_string(),
                        reason: format!("primitive {primitive_count} is missing POSITION data"),
                    })?;
            let positions = positions.collect::<Vec<_>>();
            validate_positions(
                &positions,
                source_relative_path,
                mesh.index(),
                primitive.index(),
            )?;
            if positions.is_empty() {
                return invalid_gltf("positions", "POSITION accessor is empty");
            }
            vertex_count += positions.len();
            let indices = reader
                .read_indices()
                .map(|indices| indices.into_u32().collect::<Vec<_>>())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            if indices.is_empty() || indices.len() % 3 != 0 {
                return invalid_gltf(
                    "indices",
                    format!(
                        "{source_relative_path} mesh {} primitive {} has {} indices; triangle primitives require a non-empty index count divisible by 3",
                        mesh.index(),
                        primitive.index(),
                        indices.len()
                    ),
                );
            }
            validate_indices(
                &indices,
                positions.len(),
                source_relative_path,
                mesh.index(),
                primitive.index(),
            )?;
            reject_degenerate_triangles(
                &indices,
                &positions,
                source_relative_path,
                mesh.index(),
                primitive.index(),
            )?;
            index_count += indices.len();
        }
    }
    let mesh_count = document.meshes().count();
    if mesh_count == 0 || primitive_count == 0 {
        return invalid_gltf(
            "geometry",
            "glTF source must contain at least one mesh primitive",
        );
    }
    if document.scenes().count() == 0 {
        return invalid_gltf("scenes", "glTF source must contain at least one scene");
    }
    Ok(GltfValidationReport {
        mesh_count,
        primitive_count,
        vertex_count,
        index_count,
        triangle_count: index_count / 3,
        degenerate_triangle_count: 0,
        buffer_count: buffers.len(),
        image_count: images.len(),
        dependencies,
    })
}

fn validate_positions(
    positions: &[[f32; 3]],
    source_relative_path: &str,
    mesh_index: usize,
    primitive_index: usize,
) -> Result<(), EngineError> {
    for (position_index, position) in positions.iter().enumerate() {
        if position.iter().any(|value| !value.is_finite()) {
            return invalid_gltf(
                "positions",
                format!(
                    "{source_relative_path} mesh {mesh_index} primitive {primitive_index} POSITION[{position_index}] contains non-finite data"
                ),
            );
        }
    }
    Ok(())
}

fn validate_indices(
    indices: &[u32],
    vertex_count: usize,
    source_relative_path: &str,
    mesh_index: usize,
    primitive_index: usize,
) -> Result<(), EngineError> {
    for (index_offset, index) in indices.iter().copied().enumerate() {
        if index as usize >= vertex_count {
            return invalid_gltf(
                "indices",
                format!(
                    "{source_relative_path} mesh {mesh_index} primitive {primitive_index} index[{index_offset}]={index} exceeds POSITION count {vertex_count}"
                ),
            );
        }
    }
    Ok(())
}

fn reject_degenerate_triangles(
    indices: &[u32],
    positions: &[[f32; 3]],
    source_relative_path: &str,
    mesh_index: usize,
    primitive_index: usize,
) -> Result<(), EngineError> {
    let mut degenerate_count = 0;
    let mut first_triangle_index = 0;
    let mut first_indices = [0_u32; 3];
    for (triangle_index, triangle) in indices.chunks_exact(3).enumerate() {
        if triangle_is_degenerate(triangle, positions) {
            if degenerate_count == 0 {
                first_triangle_index = triangle_index;
                first_indices = [triangle[0], triangle[1], triangle[2]];
            }
            degenerate_count += 1;
        }
    }
    if degenerate_count > 0 {
        return invalid_gltf(
            "geometry.degenerate_triangles",
            format!(
                "{source_relative_path} mesh {mesh_index} primitive {primitive_index} contains {degenerate_count} degenerate triangle(s); first_triangle={first_triangle_index}; first_indices={},{},{}; clean or replace the source asset before import",
                first_indices[0], first_indices[1], first_indices[2],
            ),
        );
    }
    Ok(())
}

fn triangle_is_degenerate(triangle: &[u32], positions: &[[f32; 3]]) -> bool {
    if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[0] == triangle[2] {
        return true;
    }
    let a = Vec3::from_array(positions[triangle[0] as usize]);
    let b = Vec3::from_array(positions[triangle[1] as usize]);
    let c = Vec3::from_array(positions[triangle[2] as usize]);
    let area_squared = (b - a).cross(c - a).length_squared();
    !area_squared.is_finite() || area_squared <= DEGENERATE_TRIANGLE_AREA_EPSILON_SQUARED
}

fn collect_dependencies(
    document: &gltf::Document,
    source_absolute_path: &Path,
    source_relative_path: &str,
) -> Result<Vec<String>, EngineError> {
    let source_parent =
        source_absolute_path
            .parent()
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.asset_product.gltf.source_parent".to_string(),
                reason: format!(
                    "source path has no parent: {}",
                    source_absolute_path.display()
                ),
            })?;
    let relative_parent = Path::new(source_relative_path).parent().ok_or_else(|| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.asset_product.gltf.relative_parent".to_string(),
            reason: format!("relative path has no parent: {source_relative_path}"),
        }
    })?;
    let mut dependencies = BTreeSet::new();
    for buffer in document.buffers() {
        if let gltf::buffer::Source::Uri(uri) = buffer.source() {
            push_dependency(uri, source_parent, relative_parent, &mut dependencies)?;
        }
    }
    for image in document.images() {
        if let gltf::image::Source::Uri { uri, .. } = image.source() {
            push_dependency(uri, source_parent, relative_parent, &mut dependencies)?;
        }
    }
    Ok(dependencies.into_iter().collect())
}

fn push_dependency(
    uri: &str,
    source_parent: &Path,
    relative_parent: &Path,
    dependencies: &mut BTreeSet<String>,
) -> Result<(), EngineError> {
    if uri.starts_with("data:") {
        return Ok(());
    }
    if uri.contains(':') {
        return invalid_gltf(
            "dependency.uri",
            format!("external URI is not local: {uri}"),
        );
    }
    let decoded = percent_decode_uri(uri)?;
    let dependency_path = Path::new(&decoded);
    reject_escaping_path(dependency_path, "dependency.uri")?;
    let absolute = source_parent.join(dependency_path);
    let metadata = fs::metadata(&absolute).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_product.gltf.dependency.exists".to_string(),
        reason: format!(
            "failed to inspect dependency {}: {error}",
            absolute.display()
        ),
    })?;
    if !metadata.is_file() {
        return invalid_gltf(
            "dependency.kind",
            format!("dependency is not a file: {}", absolute.display()),
        );
    }
    let relative = relative_parent.join(dependency_path);
    reject_escaping_path(&relative, "dependency.relative")?;
    dependencies.insert(path_to_forward_slashes(&relative));
    Ok(())
}

fn reject_escaping_path(path: &Path, field: &'static str) -> Result<(), EngineError> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return invalid_gltf(
            field,
            format!("path escapes asset directory: {}", path.display()),
        );
    }
    Ok(())
}

fn percent_decode_uri(uri: &str) -> Result<String, EngineError> {
    let mut bytes = Vec::with_capacity(uri.len());
    let mut iter = uri.as_bytes().iter().copied();
    while let Some(byte) = iter.next() {
        if byte != b'%' {
            bytes.push(byte);
            continue;
        }
        let Some(high) = iter.next() else {
            return invalid_gltf("dependency.uri", format!("invalid percent escape in {uri}"));
        };
        let Some(low) = iter.next() else {
            return invalid_gltf("dependency.uri", format!("invalid percent escape in {uri}"));
        };
        let Some(decoded) = hex_pair(high, low) else {
            return invalid_gltf("dependency.uri", format!("invalid percent escape in {uri}"));
        };
        bytes.push(decoded);
    }
    String::from_utf8(bytes).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_product.gltf.dependency.uri".to_string(),
        reason: format!("dependency URI is not UTF-8: {error}"),
    })
}

fn hex_pair(high: u8, low: u8) -> Option<u8> {
    Some(hex_value(high)? * 16 + hex_value(low)?)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn path_to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn invalid_gltf<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_product.gltf.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "gltf_validation_tests.rs"]
mod tests;
