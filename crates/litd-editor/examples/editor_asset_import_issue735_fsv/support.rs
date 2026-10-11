fn real_record(
    catalog: &AssetVaultCatalog,
    relative_path: &str,
) -> Result<AssetVaultRecord, Box<dyn std::error::Error>> {
    catalog
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .ok_or_else(|| format!("missing real vault record {relative_path}").into())
}

fn error_json<T>(
    result: Result<T, EngineError>,
    ok_message: &'static str,
) -> Result<Value, Box<dyn std::error::Error>> {
    match result {
        Ok(_) => Err(io::Error::new(io::ErrorKind::InvalidData, ok_message).into()),
        Err(error) => Ok(json!({
            "ok": false,
            "error_code": error.code(),
            "message": error.to_string()
        })),
    }
}

fn write_json(path: &Path, value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
