use std::{fs, path::Path};

use image::{DynamicImage, GenericImageView, ImageBuffer, ImageFormat, ImageReader, Luma};
use litd_engine::EngineError;
use sha2::{Digest, Sha256};

use super::{
    HeightmapBitDepth, HeightmapByteOrder, HeightmapExportSpec, HeightmapFileFormat,
    HeightmapImportSpec, TerrainGrid, heightmap_error, invalid_heightmap, validate_height_range,
};
use crate::history::EditorDocument;

#[derive(Debug, Clone)]
pub(super) struct HeightmapSamples {
    pub(super) bit_depth: HeightmapBitDepth,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) values: Vec<u16>,
}

pub(super) fn read_heightmap_samples(
    spec: &HeightmapImportSpec,
    expected_width: u32,
    expected_height: u32,
) -> Result<HeightmapSamples, EngineError> {
    match spec.format {
        HeightmapFileFormat::Png => read_png_samples(&spec.path, expected_width, expected_height),
        HeightmapFileFormat::Raw => read_raw_samples(
            &spec.path,
            spec.raw_bit_depth
                .ok_or_else(|| heightmap_error("raw.bit_depth", "raw import requires bit depth"))?,
            spec.raw_byte_order,
            expected_width,
            expected_height,
        ),
    }
}

pub(super) fn samples_from_document(
    document: &EditorDocument,
    spec: &HeightmapExportSpec,
    grid: &TerrainGrid,
) -> Result<Vec<u16>, EngineError> {
    let mut samples = vec![0_u16; sample_count(grid.width, grid.height)?];
    for z in 0..grid.height {
        for x in 0..grid.width {
            let source_z = if spec.flip_vertical {
                grid.height - 1 - z
            } else {
                z
            };
            let source_id = grid
                .ids_by_coord
                .get(&(x as i32, source_z as i32))
                .ok_or_else(|| {
                    heightmap_error("grid.coord", format!("missing cell {x},{source_z}"))
                })?;
            let height = document
                .terrain_cells
                .get(source_id)
                .ok_or_else(|| heightmap_error("grid.cell", format!("missing cell {source_id}")))?
                .height;
            let index = sample_index(grid.width, x, z)?;
            samples[index] =
                height_to_sample(height, spec.bit_depth, spec.min_height, spec.max_height)?;
        }
    }
    Ok(samples)
}

pub(super) fn write_heightmap(
    path: &Path,
    format: HeightmapFileFormat,
    bit_depth: HeightmapBitDepth,
    byte_order: HeightmapByteOrder,
    width: u32,
    height: u32,
    samples: &[u16],
) -> Result<(), EngineError> {
    match format {
        HeightmapFileFormat::Png => write_png(path, bit_depth, width, height, samples),
        HeightmapFileFormat::Raw => write_raw(path, bit_depth, byte_order, samples),
    }
}

pub(super) fn sample_to_height(
    sample: u16,
    bit_depth: HeightmapBitDepth,
    min_height: f32,
    max_height: f32,
) -> Result<f32, EngineError> {
    let normalized = sample as f32 / bit_depth.max_sample() as f32;
    let height = min_height + normalized * (max_height - min_height);
    if !height.is_finite() {
        return invalid_heightmap("sample.height", "decoded sample produced non-finite height");
    }
    Ok(height)
}

pub(super) fn sample_count(width: u32, height: u32) -> Result<usize, EngineError> {
    width
        .checked_mul(height)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| heightmap_error("dimensions", "heightmap sample count overflowed"))
}

pub(super) fn sample_index(width: u32, x: u32, z: u32) -> Result<usize, EngineError> {
    z.checked_mul(width)
        .and_then(|row| row.checked_add(x))
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| heightmap_error("index", "heightmap sample index overflowed"))
}

pub(super) fn sha256_file(path: &Path) -> Result<String, EngineError> {
    let bytes = fs::read(path).map_err(|err| {
        heightmap_error(
            "file.read",
            format!("failed to read {}: {err}", path.display()),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_png_samples(
    path: &Path,
    expected_width: u32,
    expected_height: u32,
) -> Result<HeightmapSamples, EngineError> {
    let reader = ImageReader::open(path)
        .map_err(|err| {
            heightmap_error(
                "png.open",
                format!("failed to open {}: {err}", path.display()),
            )
        })?
        .with_guessed_format()
        .map_err(|err| heightmap_error("png.format", format!("failed to identify PNG: {err}")))?;
    if reader.format() != Some(ImageFormat::Png) {
        return invalid_heightmap("png.format", "heightmap import expected a PNG file");
    }
    let image = reader
        .decode()
        .map_err(|err| heightmap_error("png.decode", format!("failed to decode PNG: {err}")))?;
    let (width, height) = image.dimensions();
    validate_dimensions(width, height, expected_width, expected_height)?;
    match image {
        DynamicImage::ImageLuma8(buffer) => Ok(HeightmapSamples {
            bit_depth: HeightmapBitDepth::U8,
            width,
            height,
            values: buffer.into_raw().into_iter().map(u16::from).collect(),
        }),
        DynamicImage::ImageLuma16(buffer) => Ok(HeightmapSamples {
            bit_depth: HeightmapBitDepth::U16,
            width,
            height,
            values: buffer.into_raw(),
        }),
        other => invalid_heightmap(
            "png.color",
            format!(
                "PNG heightmaps must be 8-bit or 16-bit grayscale, got {:?}",
                other.color()
            ),
        ),
    }
}

fn read_raw_samples(
    path: &Path,
    bit_depth: HeightmapBitDepth,
    byte_order: HeightmapByteOrder,
    expected_width: u32,
    expected_height: u32,
) -> Result<HeightmapSamples, EngineError> {
    let bytes = fs::read(path).map_err(|err| {
        heightmap_error(
            "raw.open",
            format!("failed to read {}: {err}", path.display()),
        )
    })?;
    let sample_count = sample_count(expected_width, expected_height)?;
    let expected_bytes = sample_count
        .checked_mul(bit_depth.bytes_per_sample())
        .ok_or_else(|| heightmap_error("raw.size", "raw byte count overflow"))?;
    if bytes.len() != expected_bytes {
        return invalid_heightmap(
            "raw.size",
            format!(
                "raw heightmap byte size {} did not match expected {} for {}x{} {:?}",
                bytes.len(),
                expected_bytes,
                expected_width,
                expected_height,
                bit_depth
            ),
        );
    }
    let values = match bit_depth {
        HeightmapBitDepth::U8 => bytes.into_iter().map(u16::from).collect(),
        HeightmapBitDepth::U16 => bytes
            .chunks_exact(2)
            .map(|chunk| match byte_order {
                HeightmapByteOrder::LittleEndian => u16::from_le_bytes([chunk[0], chunk[1]]),
                HeightmapByteOrder::BigEndian => u16::from_be_bytes([chunk[0], chunk[1]]),
            })
            .collect(),
    };
    Ok(HeightmapSamples {
        bit_depth,
        width: expected_width,
        height: expected_height,
        values,
    })
}

fn write_png(
    path: &Path,
    bit_depth: HeightmapBitDepth,
    width: u32,
    height: u32,
    samples: &[u16],
) -> Result<(), EngineError> {
    match bit_depth {
        HeightmapBitDepth::U8 => {
            let bytes = samples
                .iter()
                .map(|sample| {
                    u8::try_from(*sample)
                        .map_err(|_| heightmap_error("png.sample", "u8 export sample exceeded 255"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let image = ImageBuffer::<Luma<u8>, Vec<u8>>::from_vec(width, height, bytes)
                .ok_or_else(|| heightmap_error("png.buffer", "failed to assemble u8 PNG buffer"))?;
            image
                .save_with_format(path, ImageFormat::Png)
                .map_err(|err| {
                    heightmap_error(
                        "png.write",
                        format!("failed to write {}: {err}", path.display()),
                    )
                })?;
        }
        HeightmapBitDepth::U16 => {
            let image =
                ImageBuffer::<Luma<u16>, Vec<u16>>::from_vec(width, height, samples.to_vec())
                    .ok_or_else(|| {
                        heightmap_error("png.buffer", "failed to assemble u16 PNG buffer")
                    })?;
            image
                .save_with_format(path, ImageFormat::Png)
                .map_err(|err| {
                    heightmap_error(
                        "png.write",
                        format!("failed to write {}: {err}", path.display()),
                    )
                })?;
        }
    }
    Ok(())
}

fn write_raw(
    path: &Path,
    bit_depth: HeightmapBitDepth,
    byte_order: HeightmapByteOrder,
    samples: &[u16],
) -> Result<(), EngineError> {
    let mut bytes = Vec::with_capacity(samples.len() * bit_depth.bytes_per_sample());
    match bit_depth {
        HeightmapBitDepth::U8 => {
            for sample in samples {
                bytes.push(
                    u8::try_from(*sample).map_err(|_| {
                        heightmap_error("raw.sample", "u8 export sample exceeded 255")
                    })?,
                );
            }
        }
        HeightmapBitDepth::U16 => {
            for sample in samples {
                let encoded = match byte_order {
                    HeightmapByteOrder::LittleEndian => sample.to_le_bytes(),
                    HeightmapByteOrder::BigEndian => sample.to_be_bytes(),
                };
                bytes.extend_from_slice(&encoded);
            }
        }
    }
    fs::write(path, bytes).map_err(|err| {
        heightmap_error(
            "raw.write",
            format!("failed to write {}: {err}", path.display()),
        )
    })
}

fn height_to_sample(
    height: f32,
    bit_depth: HeightmapBitDepth,
    min_height: f32,
    max_height: f32,
) -> Result<u16, EngineError> {
    validate_height_range(min_height, max_height)?;
    if !height.is_finite() {
        return invalid_heightmap("export.height", "terrain height must be finite");
    }
    if height < min_height || height > max_height {
        return invalid_heightmap(
            "export.range",
            format!("height {height} outside export range {min_height}..={max_height}"),
        );
    }
    let normalized = (height - min_height) / (max_height - min_height);
    Ok((normalized * bit_depth.max_sample() as f32).round() as u16)
}

fn validate_dimensions(
    width: u32,
    height: u32,
    expected_width: u32,
    expected_height: u32,
) -> Result<(), EngineError> {
    if width != expected_width || height != expected_height {
        return invalid_heightmap(
            "dimensions",
            format!(
                "heightmap dimensions {width}x{height} did not match world {expected_width}x{expected_height}"
            ),
        );
    }
    Ok(())
}

impl HeightmapBitDepth {
    fn max_sample(self) -> u16 {
        match self {
            Self::U8 => u8::MAX.into(),
            Self::U16 => u16::MAX,
        }
    }

    fn bytes_per_sample(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
        }
    }
}
