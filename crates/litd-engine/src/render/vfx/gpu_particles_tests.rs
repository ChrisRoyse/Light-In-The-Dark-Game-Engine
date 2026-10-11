use super::*;

#[test]
fn gpu_particle_shader_declares_vertex_and_fragment_entrypoints() {
    assert!(VFX_GPU_PARTICLE_WGSL.contains("litd_vfx_particle_vs"));
    assert!(VFX_GPU_PARTICLE_WGSL.contains("litd_vfx_particle_fs"));
    assert!(VFX_GPU_PARTICLE_WGSL.contains("smoothstep"));
}

#[test]
fn gpu_particle_bmp_writer_persists_signature_dimensions_and_pixels() {
    let image = VfxGpuParticleImage::from_pixels(
        2,
        2,
        vec![
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 255, 255],
        ],
    );
    let bytes = image.bmp_bytes();

    assert_eq!(&bytes[0..2], b"BM");
    assert_eq!(u32::from_le_bytes(bytes[18..22].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(bytes[22..26].try_into().unwrap()), 2);
    assert_ne!(image.checksum(), "0000000000000000");
}
