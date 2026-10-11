use super::*;

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0e-5
}

fn approx3(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|axis| approx(a[axis], b[axis]))
}

#[test]
fn box_2x2x2_has_known_topology_and_positions() {
    let spec = PrimitiveSpec::Box {
        size: [2.0, 2.0, 2.0],
    };
    let mesh = generate_primitive(&spec, "unit.box").unwrap();
    // 6 faces * 4 corners, 6 faces * 2 triangles * 3 indices.
    assert_eq!(mesh.vertices.len(), 24);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.triangle_count(), 12);
    assert!(approx3(mesh.bounds_min, [-1.0, -1.0, -1.0]));
    assert!(approx3(mesh.bounds_max, [1.0, 1.0, 1.0]));
    // First +X face corners (see build_box).
    assert!(approx3(mesh.vertices[0].position, [1.0, -1.0, -1.0]));
    assert!(approx3(mesh.vertices[1].position, [1.0, -1.0, 1.0]));
    assert!(approx3(mesh.vertices[2].position, [1.0, 1.0, 1.0]));
    // Every corner sits on the |x|=|y|=|z|=1 cube.
    for vertex in &mesh.vertices {
        for axis in 0..3 {
            assert!(approx(vertex.position[axis].abs(), 1.0));
        }
    }
}

#[test]
fn frozen_box_is_byte_identical_across_regeneration() {
    let spec = PrimitiveSpec::Box {
        size: [2.0, 2.0, 2.0],
    };
    let first = generate_primitive_frozen(&spec, "det.box").unwrap();
    let second = generate_primitive_frozen(&spec, "det.box").unwrap();
    assert_eq!(first.content_hash, second.content_hash);
    assert_eq!(first.vertex_bytes, second.vertex_bytes);
    assert_eq!(first.index_bytes, second.index_bytes);
    assert_eq!(first.vertex_sha256, second.vertex_sha256);
    // 24 vertices * 48 bytes, 36 indices * 4 bytes.
    assert_eq!(first.vertex_bytes.len(), 24 * 48);
    assert_eq!(first.index_bytes.len(), 36 * 4);
    assert_eq!(first.vertex_stride, 48);
    first.verify().unwrap();
}

#[test]
fn different_mesh_id_yields_different_content_address() {
    let spec = PrimitiveSpec::Box {
        size: [1.0, 1.0, 1.0],
    };
    let a = generate_primitive_frozen(&spec, "id.a").unwrap();
    let b = generate_primitive_frozen(&spec, "id.b").unwrap();
    // Same geometry bytes, distinct logical address.
    assert_eq!(a.vertex_bytes, b.vertex_bytes);
    assert_ne!(a.content_hash, b.content_hash);
}

#[test]
fn tangents_are_orthonormal_with_unit_handedness() {
    let spec = PrimitiveSpec::Box {
        size: [2.0, 2.0, 2.0],
    };
    let mesh = generate_primitive(&spec, "tan.box").unwrap();
    for vertex in &mesh.vertices {
        let t = vertex.tangent;
        let length = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
        assert!(
            approx(length, 1.0),
            "tangent must be unit length, got {length}"
        );
        let dot = t[0] * vertex.normal[0] + t[1] * vertex.normal[1] + t[2] * vertex.normal[2];
        assert!(
            dot.abs() <= 1.0e-4,
            "tangent must be orthogonal to normal, dot={dot}"
        );
        assert!(
            approx(t[3].abs(), 1.0),
            "handedness must be +/-1, got {}",
            t[3]
        );
    }
}

#[test]
fn all_primitive_kinds_finalize_and_freeze() {
    let specs = [
        PrimitiveSpec::Plane {
            width: 4.0,
            depth: 3.0,
            subdivisions_x: 2,
            subdivisions_z: 3,
        },
        PrimitiveSpec::Cylinder {
            radius: 1.0,
            height: 2.0,
            radial_segments: 12,
            height_segments: 2,
        },
        PrimitiveSpec::Cone {
            radius: 1.0,
            height: 2.0,
            radial_segments: 12,
        },
        PrimitiveSpec::Sphere {
            radius: 1.5,
            segments: 16,
            rings: 8,
        },
        PrimitiveSpec::Torus {
            major_radius: 1.0,
            minor_radius: 0.3,
            major_segments: 16,
            minor_segments: 8,
        },
    ];
    for spec in &specs {
        let frozen = generate_primitive_frozen(spec, spec.kind_id()).unwrap();
        frozen.verify().unwrap();
        assert!(frozen.vertex_count > 0);
        assert!(frozen.index_count.is_multiple_of(3) && frozen.index_count > 0);
        assert_eq!(frozen.index_count / 3, frozen.triangle_count);
    }
}

#[test]
fn zero_size_primitive_is_rejected_loudly() {
    let spec = PrimitiveSpec::Box {
        size: [0.0, 2.0, 2.0],
    };
    let error = generate_primitive(&spec, "bad.box").unwrap_err();
    assert!(matches!(error, EngineError::ProcGenInvalidConfig { .. }));
    assert!(error.to_string().contains("zero-size"));
}

#[test]
fn invalid_subdivision_count_is_rejected_loudly() {
    let too_few = PrimitiveSpec::Cylinder {
        radius: 1.0,
        height: 1.0,
        radial_segments: 2,
        height_segments: 1,
    };
    let error = generate_primitive(&too_few, "bad.cyl").unwrap_err();
    assert!(error.to_string().contains("below the minimum"));

    let too_many = PrimitiveSpec::Sphere {
        radius: 1.0,
        segments: 9999,
        rings: 8,
    };
    let error = generate_primitive(&too_many, "bad.sphere").unwrap_err();
    assert!(error.to_string().contains("exceeds the maximum"));
}

fn ramp_heights(width: u32, depth: u32) -> Vec<f32> {
    (0..depth)
        .flat_map(|z| (0..width).map(move |x| (x + z) as f32 * 0.5))
        .collect()
}

#[test]
fn heightfield_1x1_is_refused_loudly() {
    let spec = HeightfieldSpec {
        tile_id: "degenerate".to_string(),
        width: 1,
        depth: 1,
        cell_size: 1.0,
        heights: vec![3.0],
        lod: 0,
        skirt: 0.0,
        splat_bands: Vec::new(),
    };
    let error = generate_heightfield(&spec).unwrap_err();
    assert!(error.to_string().contains("at least 2x2"));
}

#[test]
fn heightfield_3x3_builds_expected_grid() {
    let spec = HeightfieldSpec {
        tile_id: "tile.lod0".to_string(),
        width: 3,
        depth: 3,
        cell_size: 2.0,
        heights: ramp_heights(3, 3),
        lod: 0,
        skirt: 0.0,
        splat_bands: Vec::new(),
    };
    let product = generate_heightfield(&spec).unwrap();
    assert_eq!(product.lod_stride, 1);
    assert_eq!(product.grid_width, 3);
    assert_eq!(product.grid_depth, 3);
    assert_eq!(product.mesh.vertices.len(), 9);
    // 2x2 cells * 2 triangles * 3 indices.
    assert_eq!(product.mesh.indices.len(), 24);
    assert_eq!(product.splat_weights.len(), 9);
    product.mesh.freeze().unwrap().verify().unwrap();
}

#[test]
fn heightfield_lod_decimates_and_shares_boundaries() {
    let heights = ramp_heights(5, 5);
    let lod0 = generate_heightfield(&HeightfieldSpec {
        tile_id: "t".to_string(),
        width: 5,
        depth: 5,
        cell_size: 1.0,
        heights: heights.clone(),
        lod: 0,
        skirt: 0.0,
        splat_bands: Vec::new(),
    })
    .unwrap();
    let lod1 = generate_heightfield(&HeightfieldSpec {
        tile_id: "t".to_string(),
        width: 5,
        depth: 5,
        cell_size: 1.0,
        heights,
        lod: 1,
        skirt: 0.0,
        splat_bands: Vec::new(),
    })
    .unwrap();
    assert_eq!(lod0.grid_width, 5);
    assert_eq!(lod1.grid_width, 3);
    assert_eq!(lod1.lod_stride, 2);
    // Shared corner: both LODs place a vertex at the same tile corner.
    assert!(approx3(
        lod0.mesh.vertices[0].position,
        lod1.mesh.vertices[0].position
    ));
}

#[test]
fn heightfield_odd_lod_stride_is_refused() {
    // (width-1)=3 is not divisible by stride 2.
    let spec = HeightfieldSpec {
        tile_id: "t".to_string(),
        width: 4,
        depth: 4,
        cell_size: 1.0,
        heights: ramp_heights(4, 4),
        lod: 1,
        skirt: 0.0,
        splat_bands: Vec::new(),
    };
    let error = generate_heightfield(&spec).unwrap_err();
    assert!(error.to_string().contains("must divide"));
}

#[test]
fn heightfield_skirt_adds_perimeter_geometry() {
    let base = HeightfieldSpec {
        tile_id: "t".to_string(),
        width: 3,
        depth: 3,
        cell_size: 1.0,
        heights: ramp_heights(3, 3),
        lod: 0,
        skirt: 0.0,
        splat_bands: Vec::new(),
    };
    let no_skirt = generate_heightfield(&base).unwrap();
    let with_skirt = generate_heightfield(&HeightfieldSpec { skirt: 0.5, ..base }).unwrap();
    assert!(!no_skirt.has_skirt);
    assert!(with_skirt.has_skirt);
    assert!(with_skirt.mesh.vertices.len() > no_skirt.mesh.vertices.len());
    assert_eq!(
        with_skirt.mesh.vertices.len(),
        with_skirt.splat_weights.len()
    );
    with_skirt.mesh.freeze().unwrap().verify().unwrap();
}

#[test]
fn heightfield_splat_weights_normalize_and_track_height() {
    let spec = HeightfieldSpec {
        tile_id: "t".to_string(),
        width: 3,
        depth: 3,
        cell_size: 1.0,
        heights: vec![0.0, 0.0, 0.0, 5.0, 5.0, 5.0, 10.0, 10.0, 10.0],
        lod: 0,
        skirt: 0.0,
        splat_bands: vec![
            SplatBand {
                material_id: "grass".to_string(),
                height_min: 0.0,
                height_max: 3.0,
                feather: 1.0,
            },
            SplatBand {
                material_id: "rock".to_string(),
                height_min: 7.0,
                height_max: 12.0,
                feather: 1.0,
            },
        ],
    };
    let product = generate_heightfield(&spec).unwrap();
    assert_eq!(product.splat_layer_count, 2);
    for weights in &product.splat_weights {
        let sum: f32 = weights.iter().sum();
        assert!(approx(sum, 1.0), "splat weights must sum to 1, got {sum}");
    }
    // Low corner (height 0) is grass-dominant; high corner (height 10) rock.
    assert!(product.splat_weights[0][0] > product.splat_weights[0][1]);
    let last = product.splat_weights.len() - 1;
    assert!(product.splat_weights[last][1] > product.splat_weights[last][0]);
}

fn scatter_spec(seed: u64, count: u32) -> ScatterSpec {
    ScatterSpec {
        scatter_id: "rocks".to_string(),
        seed,
        count,
        area: [20.0, 20.0],
        prototypes: vec![
            ScatterPrototype {
                prototype_id: "rock.small".to_string(),
                weight: 3.0,
            },
            ScatterPrototype {
                prototype_id: "rock.large".to_string(),
                weight: 1.0,
            },
        ],
        min_scale: 0.5,
        max_scale: 2.0,
        random_yaw: true,
    }
}

#[test]
fn scatter_is_deterministic_for_a_seed() {
    let a = generate_scatter(&scatter_spec(42, 64)).unwrap();
    let b = generate_scatter(&scatter_spec(42, 64)).unwrap();
    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.instances, b.instances);
    let c = generate_scatter(&scatter_spec(43, 64)).unwrap();
    assert_ne!(a.content_hash, c.content_hash);
}

#[test]
fn scatter_respects_bounds_scale_and_prototypes() {
    let product = generate_scatter(&scatter_spec(7, 500)).unwrap();
    assert_eq!(product.instance_count, 500);
    for instance in &product.instances {
        assert!(instance.prototype_index < 2);
        assert!(instance.position[0].abs() <= 10.0 + 1.0e-4);
        assert!(instance.position[2].abs() <= 10.0 + 1.0e-4);
        assert!(instance.uniform_scale >= 0.5 && instance.uniform_scale <= 2.0);
        assert!(instance.rotation_y >= 0.0 && instance.rotation_y <= std::f32::consts::TAU);
    }
    // Weighted 3:1 selection should favour the first prototype.
    let small = product
        .instances
        .iter()
        .filter(|instance| instance.prototype_index == 0)
        .count();
    assert!(
        small > 250,
        "expected majority small rocks, got {small}/500"
    );
}

#[test]
fn scatter_rejects_degenerate_input() {
    let mut spec = scatter_spec(1, 0);
    assert!(
        generate_scatter(&spec)
            .unwrap_err()
            .to_string()
            .contains("at least 1")
    );
    spec.count = 4;
    spec.prototypes.clear();
    assert!(generate_scatter(&spec).is_err());
}

#[test]
fn store_roundtrips_frozen_mesh_by_content_hash() {
    // #1535: TestTempDir auto-cleans on Drop (no raw env::temp_dir() leak).
    let dir = crate::test_temp::TestTempDir::new("procgen_store_box");
    let store = ProcGenStore::open(dir.path()).unwrap();
    let frozen = generate_primitive_frozen(
        &PrimitiveSpec::Box {
            size: [2.0, 2.0, 2.0],
        },
        "store.box",
    )
    .unwrap();
    let stored = store.store_mesh(&frozen).unwrap();
    assert_eq!(stored.content_hash, frozen.content_hash);
    assert!(stored.manifest_path.exists());

    let loaded = store.load_mesh(&frozen.content_hash).unwrap();
    assert_eq!(loaded.content_hash, frozen.content_hash);
    assert_eq!(loaded.vertex_bytes, frozen.vertex_bytes);
    assert_eq!(loaded.index_bytes, frozen.index_bytes);
    assert_eq!(loaded.vertices().len(), 24);
    assert_eq!(loaded.indices().len(), 36);
    loaded.verify().unwrap();

    // A bogus address is refused, not silently defaulted.
    assert!(store.load_mesh("not-a-hash").is_err());
}
