use super::*;
use crate::{
    math::{Vec2, Vec4},
    render::ui::UiRect,
};

fn quad() -> GlyphQuad {
    GlyphQuad {
        rect_px: UiRect::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)).unwrap(),
        uv: UiRect::new(Vec2::new(0.25, 0.5), Vec2::new(0.5, 0.75)).unwrap(),
        color: Vec4::new(1.0, 0.5, 0.25, 0.75),
        codepoint: 'A' as u32,
        glyph_id: 'A' as u32,
    }
}

#[test]
fn glyph_quad_expands_to_two_clockwise_triangles() {
    let vertices = build_vertices([200, 100], &[quad()], 1).unwrap();
    assert_eq!(vertices.len(), 6);
    assert_eq!(vertices[0].position_ndc, [-1.0, 1.0]);
    assert_eq!(vertices[1].position_ndc, [0.0, 1.0]);
    assert_eq!(vertices[2].position_ndc, [0.0, 0.0]);
    assert_eq!(vertices[5].position_ndc, [-1.0, 0.0]);
    assert_eq!(vertices[0].uv, [0.25, 0.5]);
    assert_eq!(vertices[2].uv, [0.5, 0.75]);
    assert_eq!(vertices[0].color, [1.0, 0.5, 0.25, 0.75]);
}

#[test]
fn empty_plan_produces_no_vertices() {
    assert!(build_vertices([1280, 720], &[], 100).unwrap().is_empty());
}

#[test]
fn zero_viewport_and_capacity_overflow_fail_loud() {
    let viewport_error = build_vertices([0, 720], &[quad()], 1)
        .unwrap_err()
        .to_string();
    assert!(viewport_error.contains("viewport"));

    let capacity_error = build_vertices([1280, 720], &[quad(), quad()], 1)
        .unwrap_err()
        .to_string();
    assert!(capacity_error.contains("capacity"));
}

#[test]
fn non_finite_color_fails_loud() {
    let mut invalid = quad();
    invalid.color.x = f32::NAN;
    let error = build_vertices([1280, 720], &[invalid], 1)
        .unwrap_err()
        .to_string();
    assert!(error.contains("color"));
}

#[test]
fn shader_samples_sdf_and_alpha_blends() {
    assert!(RUNTIME_TEXT_GPU_WGSL.contains("textureSample"));
    assert!(RUNTIME_TEXT_GPU_WGSL.contains("smoothstep"));
    assert!(RUNTIME_TEXT_GPU_WGSL.contains("fwidth"));
}
