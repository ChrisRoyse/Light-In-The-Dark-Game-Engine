//! Canonical, self-contained font source for the packaged `litd-game` host.
//!
//! The bytes and licence notice are compile-time inputs from a locked permissive
//! dependency / tracked notice. Runtime construction re-hashes both before
//! parsing, with no machine-local font lookup or fallback.

use litd_hash::sha256_hex;

use crate::EngineError;

use super::font::{FontAtlas, FontAtlasBuild, FontAtlasBuildRequest};

pub const RUNTIME_FONT_SOURCE_ID: &str = "embedded/epaint_default_fonts/0.33.3/Ubuntu-Light.ttf";
pub const RUNTIME_FONT_SOURCE_VERSION: &str = "epaint_default_fonts@0.33.3";
pub const RUNTIME_FONT_SOURCE_BYTES: usize = 361_676;
pub const RUNTIME_FONT_SOURCE_SHA256: &str =
    "80307b8da7649aa4ee4d484b232140e3ce1ec0ca093073d3c53c8f5a5ced7a70";
pub const RUNTIME_FONT_LICENSE: &str = "Ubuntu-font-1.0";
pub const RUNTIME_FONT_LICENSE_NOTICE: &str =
    include_str!("../../../../docs/licenses/Ubuntu-Font-License-1.0.txt");
pub const RUNTIME_FONT_LICENSE_NOTICE_SHA256: &str =
    "2f0015108d68627bd788d313f529c21ff4da2c2c42a5e1f3883acc83480f9002";

pub fn build_runtime_font_atlas(
    glyphs: Vec<char>,
    font_px: f32,
    cell_px: u32,
    spread_px: u32,
    columns: u32,
) -> Result<(FontAtlasBuildRequest, FontAtlasBuild), EngineError> {
    verify_runtime_font_contract()?;
    let request = FontAtlasBuildRequest::new(
        RUNTIME_FONT_SOURCE_ID,
        glyphs,
        font_px,
        cell_px,
        spread_px,
        columns,
    );
    let build = FontAtlas::from_verified_font_bytes(
        request.clone(),
        epaint_default_fonts::UBUNTU_LIGHT,
        RUNTIME_FONT_SOURCE_SHA256,
    )?;
    Ok((request, build))
}

pub fn verify_runtime_font_contract() -> Result<(), EngineError> {
    verify_runtime_font_contract_parts(
        epaint_default_fonts::UBUNTU_LIGHT,
        RUNTIME_FONT_SOURCE_BYTES,
        RUNTIME_FONT_SOURCE_SHA256,
        RUNTIME_FONT_LICENSE_NOTICE.as_bytes(),
        RUNTIME_FONT_LICENSE_NOTICE_SHA256,
    )
}

fn verify_runtime_font_contract_parts(
    font_bytes: &[u8],
    expected_font_bytes: usize,
    expected_font_sha256: &str,
    license_notice: &[u8],
    expected_notice_sha256: &str,
) -> Result<(), EngineError> {
    verify_hash(
        "runtime_font.source_sha256",
        font_bytes,
        expected_font_sha256,
    )?;
    if font_bytes.len() != expected_font_bytes {
        return Err(EngineError::UiRenderReadbackMismatch {
            field: "runtime_font.source_bytes".to_string(),
            expected: expected_font_bytes.to_string(),
            actual: font_bytes.len().to_string(),
        });
    }
    verify_hash(
        "runtime_font.license_notice_sha256",
        license_notice,
        expected_notice_sha256,
    )
}

fn verify_hash(field: &'static str, bytes: &[u8], expected: &str) -> Result<(), EngineError> {
    let actual = sha256_hex(bytes);
    if actual != expected {
        return Err(EngineError::UiRenderReadbackMismatch {
            field: field.to_string(),
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "runtime_font_tests.rs"]
mod tests;
