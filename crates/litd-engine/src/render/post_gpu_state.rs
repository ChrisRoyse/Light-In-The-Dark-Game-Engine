use super::super::{HdrFormat, PostProcessSettings, TonemapOperator};
use super::{PostGpuInputImage, PostGradeGpu, PostSettingsGpu};

pub(super) fn source_bytes(input: &PostGpuInputImage, format: HdrFormat) -> Vec<u8> {
    let row_pitch = source_row_pitch(input.width, format) as usize;
    let row_bytes = input.width as usize * source_bytes_per_pixel(format) as usize;
    let mut bytes = vec![0_u8; row_pitch * input.height as usize];
    for y in 0..input.height as usize {
        let row = &mut bytes[y * row_pitch..y * row_pitch + row_bytes];
        for (x, pixel) in input.pixels[y * input.width as usize..(y + 1) * input.width as usize]
            .iter()
            .enumerate()
        {
            write_source_pixel(row, x, *pixel, format);
        }
    }
    bytes
}

pub(super) fn settings_gpu(
    settings: PostProcessSettings,
    width: u32,
    height: u32,
) -> PostSettingsGpu {
    PostSettingsGpu {
        exposure_ev: settings.exposure_ev,
        bloom_threshold: settings.bloom.threshold,
        bloom_knee: settings.bloom.knee,
        bloom_intensity: settings.bloom.intensity,
        lut_strength: settings.color_grade.lut_strength,
        saturation: settings.color_grade.saturation,
        contrast: settings.color_grade.contrast,
        output_gamma: settings.output_gamma,
        tonemap_operator: tonemap_operator(settings.tonemap),
        bloom_enabled: u32::from(settings.bloom.enabled),
        width,
        height,
    }
}

pub(super) fn grade_gpu(settings: PostProcessSettings) -> PostGradeGpu {
    PostGradeGpu {
        lift: settings.color_grade.lift.extend(0.0).to_array(),
        gamma: settings.color_grade.gamma.extend(0.0).to_array(),
        gain: settings.color_grade.gain.extend(0.0).to_array(),
    }
}

pub(super) fn source_format(format: HdrFormat) -> wgpu::TextureFormat {
    match format {
        HdrFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        HdrFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
    }
}

pub(super) fn source_row_pitch(width: u32, format: HdrFormat) -> u32 {
    align_to(
        width * source_bytes_per_pixel(format),
        wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
    )
}

fn write_source_pixel(row: &mut [u8], x: usize, pixel: [f32; 4], format: HdrFormat) {
    match format {
        HdrFormat::Rgba16Float => {
            let offset = x * 8;
            for (channel, value) in pixel.iter().enumerate() {
                row[offset + channel * 2..offset + channel * 2 + 2]
                    .copy_from_slice(&f32_to_f16_bits(*value).to_le_bytes());
            }
        }
        HdrFormat::Rgba32Float => {
            let offset = x * 16;
            for (channel, value) in pixel.iter().enumerate() {
                row[offset + channel * 4..offset + channel * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
    }
}

fn tonemap_operator(operator: TonemapOperator) -> u32 {
    match operator {
        TonemapOperator::Aces => 0,
        TonemapOperator::Reinhard => 1,
        TonemapOperator::None => 2,
    }
}

fn source_bytes_per_pixel(format: HdrFormat) -> u32 {
    match format {
        HdrFormat::Rgba16Float => 8,
        HdrFormat::Rgba32Float => 16,
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

fn f32_to_f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7f_ffff;
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let mantissa = (mant | 0x80_0000) >> (1 - exp);
        return sign | (((mantissa + 0x1000) >> 13) as u16);
    }
    if exp >= 31 {
        return sign | 0x7c00;
    }
    sign | ((exp as u16) << 10) | (((mant + 0x1000) >> 13) as u16)
}
