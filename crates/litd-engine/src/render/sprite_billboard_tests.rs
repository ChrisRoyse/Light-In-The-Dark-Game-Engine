use litd_hash::sha256_hex;

use crate::{
    gpu::GpuContext,
    render::sprite_billboard::{
        SpriteBillboardGpuScene, SpriteBillboardLibrarySelection, SpriteTextureRecord,
        build_sprite_billboard_library_readback, render_sprite_billboard_library_gpu_with_context,
    },
};

/// A solid `fill`-colored `width`x1 texture whose manifest hash matches its
/// pixels, so it passes per-texture validation (distinct `fill` avoids the
/// duplicate-content-address guard) and only the *combined* atlas can reject it.
fn wide_texture(texture_id: &str, width: u32, fill: u8) -> SpriteTextureRecord {
    let rgba8 = vec![fill; (width * 4) as usize];
    SpriteTextureRecord {
        texture_id: texture_id.to_string(),
        source_relative_path: format!("generated/issue1484/{texture_id}.rgba8"),
        width,
        height: 1,
        rgba8_sha256: sha256_hex(&rgba8),
        rgba8,
        tags: vec!["issue1484".to_string()],
    }
}

#[test]
fn issue868_defaults_are_content_addressed_searchable_billboards() {
    let library = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    let readback = build_sprite_billboard_library_readback(&library).unwrap();

    assert_eq!(readback.schema, super::SPRITE_BILLBOARD_SCHEMA);
    assert_eq!(readback.texture_count, 3);
    assert_eq!(readback.material_count, 3);
    assert_eq!(readback.sheet_count, 1);
    assert_eq!(readback.billboard_count, 4);
    assert_eq!(readback.search_row_count, 11);
    assert_eq!(readback.total_vertex_count, 16);
    assert_eq!(readback.total_index_count, 24);
    assert_eq!(readback.triangle_count, 8);
    assert!(
        readback
            .texture_content_addresses
            .iter()
            .all(|address| address.starts_with("sha256:") && address.len() == 71)
    );
    assert_eq!(readback.meshes[0].frame_uv, [0.0, 0.0, 0.5, 0.5]);
    assert_eq!(readback.meshes[1].frame_uv, [0.0, 0.5, 0.5, 1.0]);
    assert!(
        readback
            .search_rows
            .iter()
            .any(|row| row.terms.contains(&"billboard".to_string())
                && row.terms.contains(&"hero".to_string()))
    );
}

#[test]
fn issue868_sprite_billboard_edges_fail_closed() {
    let mut duplicate_content = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    let mut cloned = duplicate_content.textures[0].clone();
    cloned.texture_id = "texture.issue868.duplicate_pixels".to_string();
    duplicate_content.textures.push(cloned);
    assert!(
        build_sprite_billboard_library_readback(&duplicate_content)
            .unwrap_err()
            .to_string()
            .contains("duplicate content address")
    );

    let mut bad_hash = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    bad_hash.textures[0].rgba8_sha256 =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    assert!(
        build_sprite_billboard_library_readback(&bad_hash)
            .unwrap_err()
            .to_string()
            .contains("rgba8_sha256")
    );

    let mut missing_material = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    missing_material.billboards[0].material_id = "mat.sprite.issue868.missing".to_string();
    assert!(
        build_sprite_billboard_library_readback(&missing_material)
            .unwrap_err()
            .to_string()
            .contains("billboard.material_id")
    );

    let mut bad_frame = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    bad_frame.billboards[0].frame_index = 4;
    assert!(
        build_sprite_billboard_library_readback(&bad_frame)
            .unwrap_err()
            .to_string()
            .contains("frame_index")
    );

    let mut zero_size = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    zero_size.billboards[0].size[0] = 0.0;
    assert!(
        build_sprite_billboard_library_readback(&zero_size)
            .unwrap_err()
            .to_string()
            .contains("billboard.size")
    );
}

#[test]
fn issue1484_combined_atlas_overflow_fails_closed() {
    // Individually-valid textures whose combined width exceeds the GPU 2D
    // texture dimension. Before #1484 the atlas byte length wrapped u32 and
    // panicked on `copy_from_slice`; now it must fail loud with a structured
    // error before any GPU work.
    let mut oversized = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    oversized
        .textures
        .push(wide_texture("texture.issue1484.wide_a", 5_000, 0x11));
    oversized
        .textures
        .push(wide_texture("texture.issue1484.wide_b", 5_000, 0x22));

    let err = build_sprite_billboard_library_readback(&oversized)
        .expect_err("combined atlas 10048px wide must be rejected, not packed");
    let message = err.to_string();
    assert!(
        message.contains("exceeds max GPU 2D texture dimension"),
        "unexpected error: {message}"
    );

    // A combined atlas that still fits the bound continues to succeed.
    let mut within_bound = SpriteBillboardLibrarySelection::issue868_defaults().unwrap();
    within_bound
        .textures
        .push(wide_texture("texture.issue1484.fits", 4_000, 0x33));
    build_sprite_billboard_library_readback(&within_bound)
        .expect("a combined atlas within the dimension bound must validate");
}

#[test]
fn issue868_sprite_billboard_library_renders_on_real_gpu() {
    let context = pollster::block_on(GpuContext::new_headless()).unwrap();
    let scene = SpriteBillboardGpuScene::issue868_defaults(640, 240).unwrap();
    let frame = pollster::block_on(render_sprite_billboard_library_gpu_with_context(
        &context, &scene,
    ))
    .unwrap();

    assert_eq!(frame.stats.texture_count, 3);
    assert_eq!(frame.stats.material_count, 3);
    assert_eq!(frame.stats.billboard_count, 4);
    assert_eq!(frame.stats.search_row_count, 11);
    assert_eq!(frame.stats.total_vertex_count, 16);
    assert_eq!(frame.stats.total_index_count, 24);
    assert_eq!(frame.stats.triangle_count, 8);
    assert_eq!(frame.stats.draw_calls, 1);
    assert_eq!(frame.stats.billboard_readbacks.len(), 4);
    assert!(frame.stats.output_metrics.nonblack_pixel_ratio > 0.015);
    assert!(frame.stats.output_metrics.edge_contrast > 0.001);
}
