use std::{fs, path::Path};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

pub fn source_readback(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let physical_path = fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(path),
    )?;
    let bytes = fs::read(&physical_path)?;
    Ok(json!({
        "path": path,
        "physical_path": physical_path,
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
    }))
}
