struct EditorWorldUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,
    fog_color: vec4<f32>,
    params: vec4<f32>,
    atmosphere: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> world: EditorWorldUniform;
@group(0) @binding(1)
var base_color_texture: texture_2d<f32>;
@group(0) @binding(2)
var world_sampler: sampler;
@group(0) @binding(3)
var normal_texture: texture_2d<f32>;
@group(0) @binding(4)
var metallic_roughness_texture: texture_2d<f32>;

struct ReplayFogUniform {
    // enabled, width, height, reserved
    config: vec4<u32>,
};

@group(1) @binding(0)
var replay_fog_texture: texture_2d<u32>;
@group(1) @binding(1)
var<uniform> replay_fog: ReplayFogUniform;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) color_emissive: vec4<f32>,
    @location(5) material_pbr: vec4<f32>,
};

struct FoliageInstanceIn {
    @location(6) root_height: vec4<f32>,
    @location(7) yaw_width: vec4<f32>,
    @location(8) color_emissive: vec4<f32>,
    @location(9) cull_wind: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) color_emissive: vec4<f32>,
    @location(5) material_pbr: vec4<f32>,
    @location(6) foliage_fade: f32,
};

@vertex
fn litd_editor_world_vs(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = world.view_proj * vec4<f32>(input.position, 1.0);
    out.world_position = input.position;
    out.normal = normalize(input.normal);
    out.tangent = vec4<f32>(normalize(input.tangent.xyz), input.tangent.w);
    out.uv = input.uv;
    out.color_emissive = input.color_emissive;
    out.material_pbr = input.material_pbr;
    out.foliage_fade = 1.0;
    return out;
}

@vertex
fn litd_editor_foliage_vs(input: VertexIn, instance: FoliageInstanceIn) -> VertexOut {
    let side = vec3<f32>(instance.yaw_width.x, 0.0, instance.yaw_width.y);
    let forward = vec3<f32>(-instance.yaw_width.y, 0.0, instance.yaw_width.x);
    let root = instance.root_height.xyz;
    let height = instance.root_height.w;
    let width = instance.yaw_width.z;
    let local = input.position;
    let world_position = root
        + side * (local.x * width)
        + forward * (local.z * width)
        + vec3<f32>(0.0, local.y * height, 0.0);
    let local_normal = normalize(input.normal);
    let world_normal = normalize(
        side * local_normal.x +
        vec3<f32>(0.0, local_normal.y, 0.0) +
        forward * local_normal.z
    );
    let local_tangent = normalize(input.tangent.xyz);
    let world_tangent = normalize(
        side * local_tangent.x +
        vec3<f32>(0.0, local_tangent.y, 0.0) +
        forward * local_tangent.z
    );
    let distance = length(world.camera_pos.xyz - root);
    let fade = 1.0 - smoothstep(instance.cull_wind.x, instance.cull_wind.y, distance);
    var out: VertexOut;
    out.clip_position = world.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.normal = world_normal;
    out.tangent = vec4<f32>(world_tangent, input.tangent.w);
    out.uv = input.uv;
    out.color_emissive = vec4<f32>(
        input.color_emissive.rgb * instance.color_emissive.rgb,
        input.color_emissive.a + instance.color_emissive.a
    );
    out.material_pbr = input.material_pbr;
    out.foliage_fade = clamp(fade, 0.0, 1.0);
    return out;
}

@fragment
fn litd_editor_world_fs(input: VertexOut) -> @location(0) vec4<f32> {
    let lit = litd_editor_world_lighting(input);
    let fogged = litd_editor_replay_fog_color(lit, input.world_position);
    return vec4<f32>(max(fogged, vec3<f32>(0.0)), 1.0);
}

@fragment
fn litd_editor_world_foliage_fs(input: VertexOut) -> @location(0) vec4<f32> {
    if input.foliage_fade <= 0.01 {
        discard;
    }
    let lit = litd_editor_world_lighting(input);
    let fogged = litd_editor_replay_fog_color(lit, input.world_position);
    return vec4<f32>(max(fogged, vec3<f32>(0.0)), input.foliage_fade);
}

@fragment
fn litd_editor_world_emissive_fs(input: VertexOut) -> @location(0) vec4<f32> {
    let sampled_base = textureSample(base_color_texture, world_sampler, input.uv);
    let alpha_cutoff = input.material_pbr.w;
    if alpha_cutoff < -0.5 {
        discard;
    }
    if alpha_cutoff > 0.0 && sampled_base.a < alpha_cutoff {
        discard;
    }
    let emissive_strength = clamp(input.color_emissive.a, 0.0, 8.0);
    if emissive_strength <= 0.04 {
        discard;
    }
    let geometric_normal = normalize(input.normal);
    let to_camera = normalize(world.camera_pos.xyz - input.world_position);
    let rim = pow(1.0 - clamp(dot(geometric_normal, to_camera), 0.0, 1.0), 2.0);
    let additive_alpha = clamp(smoothstep(0.05, 3.5, emissive_strength), 0.0, 1.0);
    let fog_visibility = litd_editor_replay_fog_emissive(input.world_position);
    if fog_visibility <= 0.0 {
        discard;
    }
    let glow = input.color_emissive.rgb * emissive_strength * (1.25 + rim * 2.10);
    return vec4<f32>(
        glow * additive_alpha * fog_visibility,
        additive_alpha * fog_visibility,
    );
}

fn litd_editor_replay_fog_state(world_position: vec3<f32>) -> u32 {
    if replay_fog.config.x == 0u {
        return 2u;
    }
    let cell = vec2<i32>(
        i32(floor(world_position.x)),
        i32(floor(world_position.z)),
    );
    let width = i32(replay_fog.config.y);
    let height = i32(replay_fog.config.z);
    if cell.x < 0 || cell.y < 0 || cell.x >= width || cell.y >= height {
        return 0u;
    }
    return textureLoad(replay_fog_texture, cell, 0).r;
}

fn litd_editor_replay_fog_color(color: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    let state = litd_editor_replay_fog_state(world_position);
    if state == 2u {
        return color;
    }
    if state == 1u {
        let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
        let remembered = mix(vec3<f32>(luma), color, 0.24);
        return remembered * 0.45 + vec3<f32>(0.004, 0.009, 0.018);
    }
    return vec3<f32>(0.002, 0.004, 0.008);
}

fn litd_editor_replay_fog_emissive(world_position: vec3<f32>) -> f32 {
    let state = litd_editor_replay_fog_state(world_position);
    if state == 2u {
        return 1.0;
    }
    if state == 1u {
        return 0.05;
    }
    return 0.0;
}

fn litd_editor_world_atmosphere_fog(
    lit: vec3<f32>,
    world_position: vec3<f32>,
    distance: f32,
    to_light: vec3<f32>,
) -> vec3<f32> {
    let fog_start = world.params.x;
    let fog_range = max(world.params.y, 0.001);
    let fog_max = world.params.z;
    let distance_factor = max((distance - fog_start) / fog_range, 0.0);
    let distance_fog = 1.0 - exp(-distance_factor * distance_factor * 2.4);

    let height_reference = world.atmosphere.x;
    let height_range = max(world.atmosphere.y, 0.001);
    let height_strength = world.atmosphere.z;
    let aerial_strength = world.atmosphere.w;
    let low_area = clamp((height_reference - world_position.y) / height_range, 0.0, 1.0);
    let height_fog = low_area * low_area * height_strength;

    let fog = clamp(max(distance_fog, height_fog), 0.0, fog_max);
    let view_direction = normalize(world.camera_pos.xyz - world_position);
    let sun_alignment = pow(max(dot(view_direction, to_light), 0.0), 8.0);
    let cool_inscatter = vec3<f32>(0.025, 0.075, 0.145) * aerial_strength;
    let warm_inscatter = vec3<f32>(0.220, 0.120, 0.055) * sun_alignment * aerial_strength;
    let scatter = world.fog_color.rgb + cool_inscatter + warm_inscatter;
    return mix(lit, scatter, fog);
}

fn litd_editor_world_lighting(input: VertexOut) -> vec3<f32> {
    let geometric_normal = normalize(input.normal);
    let tangent = normalize(input.tangent.xyz - geometric_normal * dot(geometric_normal, input.tangent.xyz));
    let bitangent = normalize(cross(geometric_normal, tangent) * input.tangent.w);
    let sampled_normal = textureSample(normal_texture, world_sampler, input.uv).xyz * 2.0 - vec3<f32>(1.0);
    let normal_strength = clamp(input.material_pbr.z, 0.0, 4.0);
    let tangent_space_normal = normalize(vec3<f32>(
        sampled_normal.xy * normal_strength,
        sampled_normal.z
    ));
    let normal = normalize(
        tangent * tangent_space_normal.x +
        bitangent * tangent_space_normal.y +
        geometric_normal * tangent_space_normal.z
    );
    let to_light = normalize(-world.light_dir.xyz);
    let to_camera = normalize(world.camera_pos.xyz - input.world_position);
    let ndotl = max(dot(normal, to_light), 0.0);
    let half_vector = normalize(to_light + to_camera);
    let metallic_roughness = textureSample(metallic_roughness_texture, world_sampler, input.uv).rgb;
    let roughness = clamp(metallic_roughness.g * input.material_pbr.y, 0.045, 1.0);
    let metallic = clamp(metallic_roughness.b * input.material_pbr.x, 0.0, 1.0);
    let specular_power = mix(96.0, 12.0, roughness);
    let specular = pow(max(dot(normal, half_vector), 0.0), specular_power)
        * mix(0.42, 0.92, metallic)
        * (1.0 - roughness * 0.72);
    let rim = pow(1.0 - clamp(dot(normal, to_camera), 0.0, 1.0), 2.0) * 0.32;
    let sampled_base = textureSample(base_color_texture, world_sampler, input.uv);
    let alpha_cutoff = input.material_pbr.w;
    if alpha_cutoff > 0.0 && sampled_base.a < alpha_cutoff {
        discard;
    }
    let base = sampled_base.rgb * input.color_emissive.rgb;
    let emissive_strength = clamp(input.color_emissive.a, 0.0, 8.0);
    let emissive_color = input.color_emissive.rgb * emissive_strength;
    let diffuse = base * (1.0 - metallic * 0.55);
    let light_power = world.params.w;
    let sky_ambient = vec3<f32>(0.055, 0.078, 0.112) *
        (0.45 + clamp(normal.y, 0.0, 1.0) * 0.30);
    let ground_bounce = vec3<f32>(0.045, 0.025, 0.018) *
        clamp(-normal.y, 0.0, 1.0) * 0.18;
    var lit = diffuse * (0.08 + ndotl * 0.95 * light_power)
        + diffuse * (sky_ambient + ground_bounce)
        + vec3<f32>(specular * (0.58 + metallic * 0.24) * light_power);
    let energy_surface = smoothstep(1.0, 3.4, emissive_strength);
    lit = mix(lit, base * 0.08 + vec3<f32>(specular) * 0.25, energy_surface * 0.86);
    lit += vec3<f32>(0.045, 0.120, 0.220) * rim;

    let distance = length(world.camera_pos.xyz - input.world_position);
    lit = litd_editor_world_atmosphere_fog(lit, input.world_position, distance, to_light);
    let bloom_hint = smoothstep(0.35, 2.5, emissive_strength);
    lit += emissive_color * (0.72 + emissive_strength * 0.16);
    lit += emissive_color * bloom_hint * (0.12 + rim * 0.34);
    return lit;
}
