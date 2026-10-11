use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    math::Vec3,
    render::lighting::{ClusterGrid, FORWARD_PLUS_WGSL, RenderLight, SpotCone, cluster_lights},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let path = env::var("LITD_CLUSTERED_LIGHTING_FSV_STATS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tmp/issue1088-clustered-lighting-fsv.txt"));

    let grid = ClusterGrid::new(
        2,
        2,
        2,
        Vec3::new(-4.0, -4.0, 0.0),
        Vec3::new(4.0, 4.0, 8.0),
        8,
    );
    let lights = vec![
        RenderLight::directional("key sun", Vec3::new(0.2, 0.6, 1.0), Vec3::ONE, 3.0),
        RenderLight::point(
            "camp brazier",
            Vec3::new(0.0, 0.0, 2.0),
            2.5,
            Vec3::new(1.0, 0.65, 0.35),
            8.0,
        ),
        RenderLight::point(
            "tower crystal",
            Vec3::new(3.0, 3.0, 6.0),
            1.5,
            Vec3::new(0.35, 0.65, 1.0),
            5.0,
        ),
        RenderLight::spot(
            "gate cone",
            Vec3::new(-2.0, 2.0, 4.0),
            Vec3::new(0.0, -1.0, 0.0),
            2.0,
            SpotCone::new(0.35, 0.8),
            Vec3::new(0.8, 1.0, 0.55),
            6.0,
        ),
    ];
    let assignment = cluster_lights(grid, &lights)?;
    let overflow_code = match overflow_probe() {
        Err(
            err @ EngineError::Rendering(
                litd_engine::error::RenderingError::ClusteredLightingClusterOverflow { .. },
            ),
        ) => err.code(),
        other => panic!("expected ClusteredLightingClusterOverflow, got {other:?}"),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "cluster_count={}\n\
             directional_count={}\n\
             total_light_refs={}\n\
             non_empty_clusters={}\n\
             max_cluster_occupancy={}\n\
             max_lights_per_cluster={}\n\
             refs={}\n\
             wgsl_has_compute={}\n\
             wgsl_has_sphere_cull={}\n\
             overflow_error_code={}\n",
            assignment.cluster_count(),
            assignment.directional_count(),
            assignment.total_light_refs(),
            assignment.non_empty_cluster_count(),
            assignment.max_cluster_occupancy(),
            assignment.grid.max_lights_per_cluster,
            assignment.refs_summary(),
            FORWARD_PLUS_WGSL.contains("@compute"),
            FORWARD_PLUS_WGSL.contains("litd_sphere_intersects_cluster"),
            overflow_code,
        ),
    )?;
    println!("wrote clustered lighting FSV stats to {}", path.display());
    Ok(())
}

fn overflow_probe() -> Result<(), EngineError> {
    let grid = ClusterGrid::new(1, 1, 1, Vec3::ZERO, Vec3::splat(2.0), 1);
    let lights = vec![
        RenderLight::point("a", Vec3::ONE, 2.0, Vec3::ONE, 1.0),
        RenderLight::point("b", Vec3::ONE, 2.0, Vec3::ONE, 1.0),
    ];
    cluster_lights(grid, &lights).map(|_| ())
}
