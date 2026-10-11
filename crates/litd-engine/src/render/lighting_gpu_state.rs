use super::*;

pub(super) fn validate(
    settings: ForwardPlusGpuSettings,
    grid: ClusterGrid,
    rig: &LightingRig,
) -> Result<(), EngineError> {
    if settings.width < MIN_TARGET_WIDTH || settings.height < MIN_TARGET_HEIGHT {
        return invalid_lighting("forward_plus.target_size", "must be at least 64x64");
    }
    let pixels = settings.width as u64 * settings.height as u64;
    if pixels > MAX_TARGET_PIXELS {
        return invalid_lighting(
            "forward_plus.target_size",
            "must not exceed 4096x4096 pixels",
        );
    }
    grid.validate()?;
    if settings.shading_z_slice >= grid.z_slices {
        return invalid_lighting("forward_plus.shading_z_slice", "must be inside z_slices");
    }
    if grid.max_lights_per_cluster > MAX_LIGHTS_PER_CLUSTER {
        return invalid_lighting(
            "forward_plus.max_lights_per_cluster",
            "must not exceed 4096 for one GPU dispatch",
        );
    }
    rig.ambient.validate()?;
    let lights = &rig.lights;
    if lights.len() > MAX_LIGHTS {
        return invalid_lighting("forward_plus.lights", "must not exceed 65535 lights");
    }
    cluster_lights(grid, lights)?;
    checked_ref_count(grid.cluster_count()?, grid.max_lights_per_cluster)?;
    Ok(())
}

pub(super) fn checked_ref_count(
    cluster_count: usize,
    max_lights: usize,
) -> Result<usize, EngineError> {
    cluster_count
        .checked_mul(max_lights)
        .ok_or_else(|| invalid_lighting_error("forward_plus.light_indices", "buffer size overflow"))
}

pub(super) fn params(
    settings: ForwardPlusGpuSettings,
    grid: ClusterGrid,
    light_count: usize,
    cluster_count: usize,
) -> Result<ClusterParamsGpu, EngineError> {
    Ok(ClusterParamsGpu {
        cluster_count: to_u32("forward_plus.cluster_count", cluster_count)?,
        light_count: to_u32("forward_plus.light_count", light_count)?,
        max_lights_per_cluster: to_u32(
            "forward_plus.max_lights_per_cluster",
            grid.max_lights_per_cluster,
        )?,
        x_slices: grid.x_slices,
        y_slices: grid.y_slices,
        z_slices: grid.z_slices,
        target_width: settings.width,
        target_height: settings.height,
        shading_z_slice: settings.shading_z_slice,
        _pad: [0; 3],
    })
}

pub(super) fn cluster_bounds(
    grid: ClusterGrid,
    count: usize,
) -> Result<Vec<ClusterBoundsGpu>, EngineError> {
    (0..count)
        .map(|index| {
            let bounds = grid.cluster_bounds(index)?;
            Ok(ClusterBoundsGpu {
                min_pad: [bounds.min.x, bounds.min.y, bounds.min.z, 0.0],
                max_pad: [bounds.max.x, bounds.max.y, bounds.max.z, 0.0],
            })
        })
        .collect()
}

pub(super) fn light_records(lights: &[RenderLight]) -> Vec<LightRecordGpu> {
    if lights.is_empty() {
        return vec![LightRecordGpu::zeroed()];
    }
    lights
        .iter()
        .map(|light| match light.kind {
            LightKind::Directional { direction_to_light } => {
                light_record(Vec3::ZERO, 0.0, direction_to_light, light, [0.0, 0.0])
            }
            LightKind::Point { position, radius } => {
                light_record(position, radius, Vec3::ZERO, light, [0.0, 0.0])
            }
            LightKind::Spot {
                position,
                direction,
                radius,
                cone,
            } => light_record(
                position,
                radius,
                direction,
                light,
                [cone.inner_angle_rad, cone.outer_angle_rad],
            ),
        })
        .collect()
}

pub(super) fn ambient_record(ambient: &AmbientLight) -> AmbientLightGpu {
    AmbientLightGpu {
        color_intensity: [
            ambient.color.x,
            ambient.color.y,
            ambient.color.z,
            ambient.intensity,
        ],
    }
}

pub(super) fn gpu_refs(
    headers: &[ClusterHeaderGpu],
    indices: &[u32],
    cluster_count: usize,
    max_lights: usize,
) -> Result<Vec<Vec<usize>>, EngineError> {
    if headers.len() != cluster_count {
        return Err(readback_mismatch(
            "forward_plus.header_count",
            cluster_count,
            headers.len(),
        ));
    }
    let mut refs = Vec::with_capacity(cluster_count);
    for (cluster_index, header) in headers.iter().enumerate() {
        let [offset, count, clipped, _] = header.offset_count_clipped_pad;
        if clipped != 0 {
            return Err(readback_mismatch(
                "forward_plus.clipped_refs",
                0,
                format!("cluster {cluster_index} clipped {clipped}"),
            ));
        }
        let expected_offset = cluster_index * max_lights;
        if offset as usize != expected_offset {
            return Err(readback_mismatch(
                "forward_plus.cluster_offset",
                expected_offset,
                offset,
            ));
        }
        if count as usize > max_lights {
            return Err(readback_mismatch(
                "forward_plus.cluster_count",
                max_lights,
                count,
            ));
        }
        let end = offset as usize + count as usize;
        let slice = indices
            .get(offset as usize..end)
            .ok_or_else(|| readback_mismatch("forward_plus.indices_range", indices.len(), end))?;
        refs.push(slice.iter().map(|index| *index as usize).collect());
    }
    Ok(refs)
}

pub(crate) struct LightingStatsArgs<'a> {
    pub(crate) context: &'a GpuContext,
    pub(crate) settings: ForwardPlusGpuSettings,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) assignment: &'a ClusteredLightAssignment,
    pub(crate) ambient: &'a AmbientLight,
    pub(crate) light_count: usize,
    pub(crate) gpu_refs: &'a [Vec<usize>],
    pub(crate) gpu_refs_summary: String,
    pub(crate) gpu_cluster_counts: Vec<u32>,
    pub(crate) compute_workgroups: u32,
    pub(crate) row_pitch: u32,
    pub(crate) readback_bytes: usize,
    pub(crate) image: &'a ForwardPlusGpuImage,
    pub(crate) metrics: ForwardPlusGpuMetrics,
}

pub(super) fn stats(args: LightingStatsArgs<'_>) -> ForwardPlusGpuStats {
    let LightingStatsArgs {
        context,
        settings,
        format,
        assignment,
        ambient,
        light_count,
        gpu_refs,
        gpu_refs_summary,
        gpu_cluster_counts,
        compute_workgroups,
        row_pitch,
        readback_bytes,
        image,
        metrics,
    } = args;
    ForwardPlusGpuStats {
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        texture_format: format!("{format:?}"),
        width: settings.width,
        height: settings.height,
        shading_z_slice: settings.shading_z_slice,
        cluster_count: assignment.cluster_count(),
        light_count,
        directional_light_count: assignment.directional_count(),
        ambient_name: ambient.name.clone(),
        ambient_color: ambient.color,
        ambient_intensity: ambient.intensity,
        ambient_contribution: ambient.contribution(),
        max_lights_per_cluster: assignment.grid.max_lights_per_cluster,
        cpu_total_light_refs: assignment.total_light_refs(),
        gpu_total_light_refs: gpu_refs.iter().map(Vec::len).sum(),
        cpu_non_empty_clusters: assignment.non_empty_cluster_count(),
        gpu_non_empty_clusters: gpu_refs.iter().filter(|refs| !refs.is_empty()).count(),
        cpu_max_cluster_occupancy: assignment.max_cluster_occupancy(),
        gpu_max_cluster_occupancy: gpu_refs.iter().map(Vec::len).max().unwrap_or(0),
        cpu_refs_summary: assignment.refs_summary(),
        gpu_refs_summary,
        gpu_cluster_counts,
        compute_workgroups,
        draw_calls: 1,
        readback_bytes,
        row_pitch,
        image_checksum: image.checksum(),
        metrics,
    }
}

pub(super) fn refs_summary(refs: &[Vec<usize>]) -> String {
    refs.iter()
        .enumerate()
        .map(|(index, lights)| {
            let lights = lights
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            format!("{index}:[{lights}]")
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub(super) fn invalid_lighting_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::ClusteredLightingInvalidConfig {
            field: field.into(),
            reason: reason.into(),
        },
    )
}

fn light_record(
    position: Vec3,
    radius: f32,
    direction: Vec3,
    light: &RenderLight,
    spot_angles: [f32; 2],
) -> LightRecordGpu {
    LightRecordGpu {
        position_radius: [position.x, position.y, position.z, radius],
        direction_kind: [
            direction.x,
            direction.y,
            direction.z,
            light.kind_id() as f32,
        ],
        color_intensity: [light.color.x, light.color.y, light.color.z, light.intensity],
        spot_angles: [spot_angles[0], spot_angles[1], 0.0, 0.0],
    }
}

fn invalid_lighting<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_lighting_error(field, reason))
}

pub(super) fn readback_mismatch(
    field: impl Into<String>,
    expected: impl ToString,
    actual: impl ToString,
) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::ClusteredLightingGpuReadbackMismatch {
            field: field.into(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        },
    )
}

fn to_u32(field: &'static str, value: usize) -> Result<u32, EngineError> {
    u32::try_from(value).map_err(|_| invalid_lighting_error(field, "does not fit in u32"))
}
