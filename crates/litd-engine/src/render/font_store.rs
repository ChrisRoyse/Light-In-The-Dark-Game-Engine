use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;

use super::{FontAtlas, FontAtlasImage, verify_font_atlas_integrity};
use crate::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

const FONT_ATLAS_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCE";
const FONT_ATLAS_VAULT_PURPOSE: &[u8] = b"litd-engine-font-atlas-issue1363";
const FONT_ATLAS_COLLECTION: &str = "litd_engine_font_atlas";
const FONT_ATLAS_JSON_ROW: &[u8] = b"font/atlas/latest";
const FONT_ATLAS_JSON_COLUMN: &[u8] = b"atlas_json";

#[derive(Debug, Clone)]
pub struct FontAtlasJsonReadback {
    pub path: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    pub fnv1a64: String,
    pub atlas: FontAtlas,
}

#[derive(Debug, Clone)]
pub struct FontAtlasBmpReadback {
    pub path: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub magic: String,
}

pub fn write_font_atlas_json(
    path: impl AsRef<Path>,
    atlas: &FontAtlas,
) -> Result<FontAtlasJsonReadback, EngineError> {
    verify_font_atlas_integrity(atlas)?;
    let path = path.as_ref();
    validate_path(path)?;
    let bytes = serde_json::to_vec_pretty(atlas)
        .map_err(|error| persistence_failed("serialize_font_atlas_json", error.to_string()))?;
    let stored = write_calyx_json_bytes(&font_atlas_spec(path), &bytes)
        .map_err(|error| persistence_failed("write_font_atlas_json", error))?;
    if stored.bytes != bytes {
        return Err(persistence_failed(
            "verify_font_atlas_json",
            "Aster font atlas bytes did not match serialized JSON",
        ));
    }
    let readback = load_font_atlas_json(path)?;
    if readback.atlas != *atlas {
        return Err(EngineError::UiRenderReadbackMismatch {
            field: "font_atlas_json".to_string(),
            expected: atlas.fnv1a64.clone(),
            actual: readback.fnv1a64,
        });
    }
    Ok(readback)
}

pub fn load_font_atlas_json(path: impl AsRef<Path>) -> Result<FontAtlasJsonReadback, EngineError> {
    let path = path.as_ref();
    validate_path(path)?;
    let readback = read_calyx_json_bytes(&font_atlas_spec(path))
        .map_err(|error| persistence_failed("read_font_atlas_json", error))?
        .ok_or_else(|| {
            persistence_failed(
                "read_font_atlas_json",
                format!("missing Calyx Aster font atlas row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let atlas = serde_json::from_slice::<FontAtlas>(&bytes).map_err(|error| {
        persistence_failed(
            "deserialize_font_atlas_json",
            format!("{}: {error}", path.display()),
        )
    })?;
    verify_font_atlas_integrity(&atlas)?;
    Ok(FontAtlasJsonReadback {
        path: path.to_path_buf(),
        bytes: bytes.len(),
        sha256: sha256_hex(&bytes),
        fnv1a64: atlas.fnv1a64.clone(),
        atlas,
    })
}

pub fn write_font_atlas_bmp(
    path: impl AsRef<Path>,
    image: &FontAtlasImage,
) -> Result<FontAtlasBmpReadback, EngineError> {
    let path = path.as_ref();
    validate_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            persistence_failed(
                "create_font_atlas_bmp_parent",
                format!("{}: {error}", parent.display()),
            )
        })?;
    }
    write_bmp_synced(path, &image.bmp_bytes())?;
    load_font_atlas_bmp(path)
}

pub fn load_font_atlas_bmp(path: impl AsRef<Path>) -> Result<FontAtlasBmpReadback, EngineError> {
    let path = path.as_ref();
    validate_path(path)?;
    let bytes = fs::read(path).map_err(|error| {
        persistence_failed(
            "read_font_atlas_bmp",
            format!("{}: {error}", path.display()),
        )
    })?;
    if bytes.len() < 54 || &bytes[0..2] != b"BM" {
        return Err(persistence_failed(
            "decode_font_atlas_bmp",
            "BMP header is missing BM signature",
        ));
    }
    let pixel_offset = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let dib_header_size = u32::from_le_bytes(bytes[14..18].try_into().unwrap());
    let width = u32::from_le_bytes(bytes[18..22].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let planes = u16::from_le_bytes(bytes[26..28].try_into().unwrap());
    let bits_per_pixel = u16::from_le_bytes(bytes[28..30].try_into().unwrap());
    let compression = u32::from_le_bytes(bytes[30..34].try_into().unwrap());
    if dib_header_size != 40
        || pixel_offset != 54
        || width == 0
        || height == 0
        || planes != 1
        || bits_per_pixel != 24
        || compression != 0
    {
        return Err(persistence_failed(
            "decode_font_atlas_bmp",
            "BMP must be uncompressed 24-bit BI_RGB with a BITMAPINFOHEADER",
        ));
    }
    let row_stride = ((width as usize) * 3).div_ceil(4) * 4;
    let expected_len = pixel_offset
        .checked_add(row_stride.checked_mul(height as usize).ok_or_else(|| {
            persistence_failed("decode_font_atlas_bmp", "BMP pixel byte count overflowed")
        })?)
        .ok_or_else(|| persistence_failed("decode_font_atlas_bmp", "BMP file size overflowed"))?;
    if bytes.len() != expected_len {
        return Err(EngineError::UiRenderReadbackMismatch {
            field: "font_atlas_bmp.bytes".to_string(),
            expected: expected_len.to_string(),
            actual: bytes.len().to_string(),
        });
    }
    Ok(FontAtlasBmpReadback {
        path: path.to_path_buf(),
        bytes: bytes.len(),
        sha256: sha256_hex(&bytes),
        width,
        height,
        magic: "BM".to_string(),
    })
}

fn validate_path(path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(persistence_failed(
            "font_atlas_path",
            "path must be non-empty",
        ));
    }
    Ok(())
}

fn write_bmp_synced(path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            persistence_failed(
                "open_font_atlas_artifact",
                format!("{}: {error}", path.display()),
            )
        })?;
    file.write_all(bytes).map_err(|error| {
        persistence_failed(
            "write_font_atlas_artifact",
            format!("{}: {error}", path.display()),
        )
    })?;
    file.sync_all().map_err(|error| {
        persistence_failed(
            "sync_font_atlas_artifact",
            format!("{}: {error}", path.display()),
        )
    })?;
    Ok(())
}

fn font_atlas_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: FONT_ATLAS_VAULT_ID,
        purpose: FONT_ATLAS_VAULT_PURPOSE,
        collection: FONT_ATLAS_COLLECTION,
        row_key: FONT_ATLAS_JSON_ROW,
        column: FONT_ATLAS_JSON_COLUMN,
    }
}

fn persistence_failed(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderPersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    }
}
