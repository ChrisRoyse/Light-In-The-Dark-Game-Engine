use crate::{
    gpu::GpuContext,
    render::primitive::{
        PrimitiveGallerySelection, PrimitiveGpuScene, PrimitiveKind, PrimitiveSpec,
        build_primitive_gallery_readback, build_primitive_mesh,
        render_primitive_gallery_gpu_with_context,
    },
};

#[test]
fn blockout_defaults_cover_all_requested_primitives() {
    let gallery = PrimitiveGallerySelection::blockout_defaults().unwrap();
    let readback = build_primitive_gallery_readback(&gallery).unwrap();

    assert_eq!(readback.primitive_count, 9);
    assert_eq!(
        readback.kind_ids,
        vec![
            "box", "sphere", "cylinder", "torus", "tube", "plane", "sprite", "points", "lines"
        ]
    );
    assert_eq!(readback.triangle_mesh_count, 7);
    assert_eq!(readback.point_mesh_count, 1);
    assert_eq!(readback.line_mesh_count, 1);
    assert_eq!(readback.total_vertex_count, 588);
    assert_eq!(readback.total_index_count, 2284);
    assert_eq!(readback.meshes[0].triangle_count, 12);
    assert_eq!(readback.meshes[7].point_count, 16);
    assert_eq!(readback.meshes[8].line_count, 6);
}

#[test]
fn primitive_builder_edges_fail_closed() {
    let bad_sphere = spec(
        "bad.sphere",
        PrimitiveKind::Sphere {
            radius: 1.0,
            segments: 2,
            rings: 8,
        },
    );
    assert!(
        build_primitive_mesh(&bad_sphere)
            .unwrap_err()
            .to_string()
            .contains("sphere.segments")
    );

    let bad_tube = spec(
        "bad.tube",
        PrimitiveKind::Tube {
            outer_radius: 0.5,
            inner_radius: 0.5,
            height: 1.0,
            radial_segments: 8,
        },
    );
    assert!(
        build_primitive_mesh(&bad_tube)
            .unwrap_err()
            .to_string()
            .contains("tube.radius")
    );

    let bad_lines = spec(
        "bad.lines",
        PrimitiveKind::Lines {
            points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        },
    );
    assert!(
        build_primitive_mesh(&bad_lines)
            .unwrap_err()
            .to_string()
            .contains("line-list")
    );

    let mut gallery = PrimitiveGallerySelection::blockout_defaults().unwrap();
    gallery.primitives[1].primitive_id = gallery.primitives[0].primitive_id.clone();
    assert!(
        build_primitive_gallery_readback(&gallery)
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
}

#[test]
fn primitive_gallery_renders_on_real_gpu() {
    let context = pollster::block_on(GpuContext::new_headless()).unwrap();
    let scene = PrimitiveGpuScene::blockout_defaults(720, 240).unwrap();
    let frame =
        pollster::block_on(render_primitive_gallery_gpu_with_context(&context, &scene)).unwrap();

    assert_eq!(frame.stats.primitive_count, 9);
    assert_eq!(frame.stats.draw_calls, 9);
    assert_eq!(frame.stats.triangle_mesh_count, 7);
    assert_eq!(frame.stats.line_mesh_count, 1);
    assert_eq!(frame.stats.point_mesh_count, 1);
    assert_eq!(
        frame.stats.rendered_topologies,
        vec![
            "triangle_list",
            "triangle_list",
            "triangle_list",
            "triangle_list",
            "triangle_list",
            "triangle_list",
            "triangle_list",
            "point_list",
            "line_list"
        ]
    );
    assert!(frame.stats.output_metrics.nonblack_pixel_ratio > 0.015);
}

fn spec(id: &str, kind: PrimitiveKind) -> PrimitiveSpec {
    PrimitiveSpec {
        primitive_id: id.to_string(),
        kind,
        color: [0.5, 0.8, 1.0, 1.0],
    }
}
