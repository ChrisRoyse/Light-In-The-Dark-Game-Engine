use std::{fs, path::Path};

use litd_engine::EngineError;
use sha2::{Digest, Sha256};

use crate::asset_products::AssetProduct;

pub(super) fn verify_product_hash(
    source_path: &Path,
    product: &AssetProduct,
) -> Result<String, EngineError> {
    let bytes = fs::read(source_path).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.gltf_import.source.read".to_string(),
        reason: format!("failed to read {}: {error}", source_path.display()),
    })?;
    if bytes.len() as u64 != product.source_bytes {
        return invalid_import(
            "source.bytes",
            format!(
                "{} manifest bytes {} != actual {}",
                product.source_relative_path,
                product.source_bytes,
                bytes.len()
            ),
        );
    }
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = format!("{:x}", hasher.finalize());
    if hash != product.source_sha256 {
        return invalid_import(
            "source.sha256",
            format!(
                "{} manifest sha256 {} != actual {}",
                product.source_relative_path, product.source_sha256, hash
            ),
        );
    }
    Ok(hash)
}

pub(super) fn validate_vec3_iter(
    iter: impl IntoIterator<Item = [f32; 3]>,
    field: &str,
    label: &str,
) -> Result<usize, EngineError> {
    let mut count = 0;
    for value in iter {
        if value.iter().any(|component| !component.is_finite()) {
            return invalid_import(field, format!("{label} contains non-finite data"));
        }
        count += 1;
    }
    Ok(count)
}

pub(super) fn invalid_import<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.gltf_import.{}", field.into()),
        reason: reason.into(),
    })
}
