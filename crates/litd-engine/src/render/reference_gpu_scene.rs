use crate::{
    EngineError,
    math::{Mat4, Quat, Vec3, Vec4},
};

use super::ReferenceSceneSettings;

type Color = [f32; 4];
type Rect = [f32; 5];

const ZERO: Color = [0.0; 4];

pub(super) struct GpuSceneBuffers {
    pub vertex_bytes: Vec<u8>,
    pub instance_bytes: Vec<u8>,
    pub camera_bytes: Vec<u8>,
    pub quad_instance_count: u32,
    pub instance_count: u32,
}

#[derive(Debug, Clone, Copy)]
struct GpuInstance {
    transform: Mat4,
    base_color: Vec4,
    emissive: Vec4,
    mesh: MeshKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeshKind {
    Quad,
    Diamond,
}

pub(super) fn build_gpu_scene_buffers(
    settings: ReferenceSceneSettings,
) -> Result<GpuSceneBuffers, EngineError> {
    let instances = scene_instances(settings)?;
    Ok(GpuSceneBuffers {
        vertex_bytes: scene_vertex_bytes(),
        instance_bytes: scene_instance_bytes(&instances),
        camera_bytes: camera_bytes(settings),
        quad_instance_count: quad_instance_count(&instances),
        instance_count: instances.len() as u32,
    })
}

fn scene_vertex_bytes() -> Vec<u8> {
    let n = Vec3::Z;
    let vertices = [
        (Vec3::new(-0.5, -0.5, 0.0), n),
        (Vec3::new(0.5, -0.5, 0.0), n),
        (Vec3::new(0.5, 0.5, 0.0), n),
        (Vec3::new(-0.5, -0.5, 0.0), n),
        (Vec3::new(0.5, 0.5, 0.0), n),
        (Vec3::new(-0.5, 0.5, 0.0), n),
        (Vec3::new(0.0, 0.5, 0.0), n),
        (Vec3::new(0.5, 0.0, 0.0), n),
        (Vec3::new(0.0, -0.5, 0.0), n),
        (Vec3::new(0.0, 0.5, 0.0), n),
        (Vec3::new(0.0, -0.5, 0.0), n),
        (Vec3::new(-0.5, 0.0, 0.0), n),
    ];
    let mut bytes = Vec::with_capacity(vertices.len() * 24);
    for (position, normal) in vertices {
        push_vec3(&mut bytes, position);
        push_vec3(&mut bytes, normal);
    }
    bytes
}

fn scene_instances(settings: ReferenceSceneSettings) -> Result<Vec<GpuInstance>, EngineError> {
    let aspect = settings.width as f32 / settings.height as f32;
    let half_y = 9.0;
    let half_x = half_y * aspect;
    let z_world = -8.0;
    let z_ui = -1.0;
    let mut out = Vec::new();

    q(
        &mut out,
        [0.0, 0.0, half_x * 2.0, half_y * 2.0, z_world],
        [0.025, 0.025, 0.028, 1.0],
        ZERO,
    );
    q(
        &mut out,
        [0.0, 5.8, half_x * 2.0, 5.2, z_world + 0.05],
        [0.012, 0.045, 0.18, 0.82],
        [0.0, 0.02, 0.12, 0.0],
    );
    q(
        &mut out,
        [-12.4, 0.0, 2.8, 10.4, z_world + 0.05],
        [0.008, 0.032, 0.13, 0.78],
        [0.0, 0.01, 0.08, 0.0],
    );
    for i in -15..=15 {
        let x = i as f32 * 1.02;
        q(
            &mut out,
            [x, 5.8, 0.12, 5.0, z_world + 0.14],
            [0.0, 0.0, 0.010, 0.82],
            ZERO,
        );
        q(
            &mut out,
            [x + 0.48, 5.8, 0.08, 5.0, z_world + 0.13],
            [0.15, 0.18, 0.24, 0.46],
            ZERO,
        );
    }
    for j in 0..10 {
        let y = 3.35 + j as f32 * 0.46;
        q(
            &mut out,
            [0.0, y, half_x * 2.0, 0.11, z_world + 0.16],
            [0.0, 0.0, 0.010, 0.78],
            ZERO,
        );
        q(
            &mut out,
            [0.0, y + 0.22, half_x * 2.0, 0.07, z_world + 0.15],
            [0.16, 0.18, 0.24, 0.42],
            ZERO,
        );
    }
    q(
        &mut out,
        [-4.8, -0.7, 6.6, 4.2, z_world + 0.2],
        [0.30, 0.11, 0.04, 0.84],
        [1.80, 0.32, 0.03, 0.0],
    );
    q(
        &mut out,
        [4.2, 0.6, 6.0, 4.6, z_world + 0.3],
        [0.015, 0.065, 0.20, 0.82],
        [0.01, 0.14, 1.25, 0.0],
    );
    for i in -6..=6 {
        let x = i as f32 * 1.45;
        q(
            &mut out,
            [x, -0.1, 0.12, 5.4, z_world + 0.6],
            [0.70, 0.66, 0.50, 0.62],
            ZERO,
        );
        q(
            &mut out,
            [x + 0.7, -0.1, 0.12, 5.4, z_world + 0.5],
            [0.005, 0.006, 0.020, 0.84],
            ZERO,
        );
    }
    for ix in -6..=6 {
        for iy in -4..=4 {
            let warm_tile = (ix + iy) % 2 == 0;
            let color = if warm_tile {
                [0.26, 0.17, 0.09, 0.72]
            } else {
                [0.045, 0.045, 0.050, 0.82]
            };
            q(
                &mut out,
                [
                    ix as f32 * 0.88,
                    iy as f32 * 0.52 - 0.1,
                    0.78,
                    0.44,
                    z_world + 0.55,
                ],
                color,
                ZERO,
            );
        }
    }
    for j in -4..=4 {
        let y = j as f32 * 0.66;
        q(
            &mut out,
            [0.0, y, 13.0, 0.16, z_world + 0.65],
            [0.005, 0.006, 0.014, 0.72],
            ZERO,
        );
    }
    for [x, y, w, h] in [
        [-7.2, 2.4, 0.8, 4.1],
        [-5.9, 3.0, 0.7, 3.0],
        [6.0, 2.8, 0.8, 3.5],
        [7.5, 2.1, 0.7, 4.6],
    ] {
        q(
            &mut out,
            [x, y, w, h, z_world + 0.8],
            [0.17, 0.18, 0.20, 1.0],
            ZERO,
        );
        q(
            &mut out,
            [x - w * 0.22, y + 0.15, w * 0.12, h * 0.92, z_world + 0.7],
            [0.44, 0.48, 0.51, 0.58],
            ZERO,
        );
    }
    for (rect, color, emissive) in [
        (
            [-3.8, -0.4, 0.86, 0.86, z_world + 1.2],
            [0.88, 0.20, 0.07, 1.0],
            [2.8, 0.52, 0.08, 0.0],
        ),
        (
            [3.4, 0.7, 0.95, 0.95, z_world + 1.2],
            [0.08, 0.50, 0.96, 1.0],
            [0.08, 0.80, 2.4, 0.0],
        ),
        (
            [0.0, -0.2, 0.82, 0.82, z_world + 1.2],
            [0.90, 0.62, 0.22, 1.0],
            [0.90, 0.48, 0.08, 0.0],
        ),
    ] {
        q(&mut out, rect, color, emissive);
    }
    for [x, y] in [[-1.7, -0.4], [-0.9, 0.7], [1.4, 0.6], [2.0, -0.5]] {
        q(
            &mut out,
            [x, y, 0.58, 0.48, z_world + 1.1],
            [0.20, 0.07, 0.08, 1.0],
            [0.42, 0.04, 0.03, 0.0],
        );
    }
    q(
        &mut out,
        [0.0, 7.4, 8.0, 0.82, z_ui],
        [0.03, 0.02, 0.025, 0.94],
        ZERO,
    );
    q(
        &mut out,
        [0.0, 7.4, 7.3, 0.25, z_ui + 0.05],
        [0.62, 0.05, 0.05, 1.0],
        [0.85, 0.05, 0.02, 0.0],
    );
    q(
        &mut out,
        [0.0, -7.6, 12.8, 2.1, z_ui],
        [0.025, 0.025, 0.035, 0.96],
        ZERO,
    );
    for rect in [
        [0.0, -6.55, 12.8, 0.14, z_ui + 0.14],
        [0.0, -8.65, 12.8, 0.14, z_ui + 0.14],
        [-6.4, -7.6, 0.14, 2.1, z_ui + 0.14],
        [6.4, -7.6, 0.14, 2.1, z_ui + 0.14],
    ] {
        q(
            &mut out,
            rect,
            [0.94, 0.48, 0.025, 1.0],
            [0.45, 0.18, 0.0, 0.0],
        );
    }
    q(
        &mut out,
        [-5.6, -7.6, 1.1, 1.1, z_ui + 0.10],
        [0.62, 0.02, 0.02, 1.0],
        [0.65, 0.02, 0.02, 0.0],
    );
    q(
        &mut out,
        [5.6, -7.6, 1.1, 1.1, z_ui + 0.10],
        [0.02, 0.21, 0.70, 1.0],
        [0.02, 0.18, 0.95, 0.0],
    );
    for i in 0..10 {
        let x = -3.9 + i as f32 * 0.78;
        q(
            &mut out,
            [x, -7.6, 0.58, 0.78, z_ui + 0.08],
            [0.05, 0.055, 0.07, 1.0],
            ZERO,
        );
        q(
            &mut out,
            [x, -7.25, 0.46, 0.08, z_ui + 0.12],
            [0.94, 0.48, 0.025, 1.0],
            [0.45, 0.18, 0.0, 0.0],
        );
    }
    q(
        &mut out,
        [11.7, 5.3, 3.0, 3.0, z_ui],
        [0.02, 0.03, 0.035, 0.92],
        ZERO,
    );
    q(
        &mut out,
        [-12.5, -3.8, 4.6, 2.2, z_ui],
        [0.02, 0.025, 0.035, 0.84],
        ZERO,
    );

    let quad_count = out.len();
    d(
        &mut out,
        [0.0, -0.3, 12.5, 5.7, z_world + 0.4],
        [0.30, 0.31, 0.29, 1.0],
        ZERO,
    );
    d(
        &mut out,
        [11.7, 5.3, 2.35, 2.0, z_ui + 0.08],
        [0.19, 0.33, 0.21, 0.96],
        [0.02, 0.16, 0.02, 0.0],
    );
    d(
        &mut out,
        [11.7, 5.3, 3.2, 3.2, z_ui + 0.12],
        [0.94, 0.48, 0.025, 1.0],
        [0.38, 0.14, 0.0, 0.0],
    );
    d(
        &mut out,
        [-4.2, -7.6, 1.4, 1.4, z_ui + 0.15],
        [0.94, 0.48, 0.025, 1.0],
        [0.45, 0.18, 0.0, 0.0],
    );
    d(
        &mut out,
        [4.2, -7.6, 1.4, 1.4, z_ui + 0.15],
        [0.94, 0.48, 0.025, 1.0],
        [0.45, 0.18, 0.0, 0.0],
    );
    validate_mesh_groups(&out, quad_count)?;
    Ok(out)
}

fn validate_mesh_groups(instances: &[GpuInstance], quad_count: usize) -> Result<(), EngineError> {
    let quads_ok = instances[..quad_count]
        .iter()
        .all(|it| it.mesh == MeshKind::Quad);
    let diamonds_ok = instances[quad_count..]
        .iter()
        .all(|it| it.mesh == MeshKind::Diamond);
    if quads_ok && diamonds_ok {
        Ok(())
    } else {
        Err(gpu_scene_error(
            "instance_layout",
            "quad and diamond instances must be grouped by mesh for two draw calls",
        ))
    }
}

fn scene_instance_bytes(instances: &[GpuInstance]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(instances.len() * 96);
    for instance in instances {
        push_mat4(&mut bytes, instance.transform);
        push_vec4(&mut bytes, instance.base_color);
        push_vec4(&mut bytes, instance.emissive);
    }
    bytes
}

fn quad_instance_count(instances: &[GpuInstance]) -> u32 {
    instances
        .iter()
        .take_while(|it| it.mesh == MeshKind::Quad)
        .count() as u32
}

fn camera_bytes(settings: ReferenceSceneSettings) -> Vec<u8> {
    let aspect = settings.width as f32 / settings.height as f32;
    let half_y = 9.0;
    let half_x = half_y * aspect;
    let view_proj = Mat4::orthographic_rh(-half_x, half_x, -half_y, half_y, 0.1, 20.0);
    let mut bytes = Vec::with_capacity(128);
    push_mat4(&mut bytes, view_proj);
    push_vec4(&mut bytes, Vec4::new(0.28, 0.48, 0.83, 0.0).normalize());
    push_vec4(&mut bytes, Vec4::new(1.00, 0.76, 0.48, 0.95));
    push_vec4(&mut bytes, Vec4::new(-3.6, -0.4, -5.6, 9.0));
    push_vec4(&mut bytes, Vec4::new(1.0, 0.30, 0.08, 1.6));
    bytes
}

fn q(out: &mut Vec<GpuInstance>, rect: Rect, color: Color, emissive: Color) {
    out.push(instance(MeshKind::Quad, rect, color, emissive));
}

fn d(out: &mut Vec<GpuInstance>, rect: Rect, color: Color, emissive: Color) {
    out.push(instance(MeshKind::Diamond, rect, color, emissive));
}

fn instance(
    mesh: MeshKind,
    [x, y, width, height, z]: Rect,
    color: Color,
    emissive: Color,
) -> GpuInstance {
    GpuInstance {
        transform: Mat4::from_scale_rotation_translation(
            Vec3::new(width, height, 1.0),
            Quat::IDENTITY,
            Vec3::new(x, y, z),
        ),
        base_color: Vec4::from_array(color),
        emissive: Vec4::from_array(emissive),
        mesh,
    }
}

fn push_mat4(bytes: &mut Vec<u8>, value: Mat4) {
    for component in value.to_cols_array() {
        push_f32(bytes, component);
    }
}

fn push_vec3(bytes: &mut Vec<u8>, value: Vec3) {
    for component in value.to_array() {
        push_f32(bytes, component);
    }
}

fn push_vec4(bytes: &mut Vec<u8>, value: Vec4) {
    for component in value.to_array() {
        push_f32(bytes, component);
    }
}

fn push_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn gpu_scene_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::ReferenceSceneInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
