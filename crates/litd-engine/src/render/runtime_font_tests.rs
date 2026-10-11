use super::*;

#[test]
fn canonical_runtime_font_is_embedded_versioned_licensed_and_hash_verified() {
    verify_runtime_font_contract().unwrap();
    let (request, build) =
        build_runtime_font_atlas((32_u8..=126).map(char::from).collect(), 40.0, 64, 8, 16).unwrap();
    assert_eq!(
        request.font_path.display().to_string(),
        RUNTIME_FONT_SOURCE_ID
    );
    assert_eq!(build.atlas.source_font_sha256, RUNTIME_FONT_SOURCE_SHA256);
    assert_eq!(
        epaint_default_fonts::UBUNTU_LIGHT.len(),
        RUNTIME_FONT_SOURCE_BYTES
    );
    assert_eq!(RUNTIME_FONT_SOURCE_VERSION, "epaint_default_fonts@0.33.3");
    assert_eq!(RUNTIME_FONT_LICENSE, "Ubuntu-font-1.0");
    assert!(RUNTIME_FONT_LICENSE_NOTICE.contains("UBUNTU FONT LICENCE Version 1.0"));
}

#[test]
fn embedded_font_hash_mismatch_fails_before_font_parse() {
    let request = FontAtlasBuildRequest::for_text("embedded/tampered.ttf", "A", 40.0, 64, 8, 1);
    let error =
        FontAtlas::from_verified_font_bytes(request, b"not a font", &"0".repeat(64)).unwrap_err();
    assert!(error.to_string().contains("font.source_sha256"));
}

#[test]
fn missing_license_notice_fails_the_runtime_font_contract() {
    let error = verify_runtime_font_contract_parts(
        epaint_default_fonts::UBUNTU_LIGHT,
        RUNTIME_FONT_SOURCE_BYTES,
        RUNTIME_FONT_SOURCE_SHA256,
        b"",
        RUNTIME_FONT_LICENSE_NOTICE_SHA256,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("runtime_font.license_notice_sha256")
    );
}
