//! #816 minimap generation + loading screen + map preview.
//!
//! The minimap is EDITOR authoring output baked into the package as DATA. Per
//! #1783 an RTS game ships a minimap; per #944 this is creator/authoring/loading
//! tooling and must never become a default RPG player HUD map (the payload records
//! `rpg_runtime_hud: false` explicitly). Generation deterministically box-downsamples
//! an authored source colour grid to the target minimap resolution and fingerprints
//! the packed pixels -- a frozen, reproducible artifact, not a live query.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Rgba8, WorldEnvironmentError, invalid, record::EnvironmentPayload, require_non_empty};

const MAX_MINIMAP_DIM: u32 = 512;

/// Which authored layer drives minimap colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinimapSource {
    /// Terrain tile albedo.
    TerrainAlbedo,
    /// Height-shaded relief.
    HeightRelief,
    /// Pathing/walkability classification.
    Pathing,
}

impl MinimapSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TerrainAlbedo => "terrain_albedo",
            Self::HeightRelief => "height_relief",
            Self::Pathing => "pathing",
        }
    }
}

/// Content-addressed reference to a preview/loading image blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewImageRef {
    /// SHA-256 (64 hex) of the opaque image blob (the only non-Calyx bytes allowed).
    pub blob_sha256: String,
    pub width_px: u32,
    pub height_px: u32,
}

impl PreviewImageRef {
    fn validate(&self, field: &str) -> Result<(), WorldEnvironmentError> {
        if self.blob_sha256.len() != 64
            || !self.blob_sha256.chars().all(|ch| ch.is_ascii_hexdigit())
        {
            return invalid(
                format!("{field}.blob_sha256"),
                "must be a 64-hex-char content-addressed blob digest",
            );
        }
        if self.width_px == 0 || self.height_px == 0 {
            return invalid(
                format!("{field}.dimensions"),
                "preview image must be non-empty",
            );
        }
        Ok(())
    }
}

/// Authored loading screen shown while the packaged map loads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadingScreen {
    pub title: String,
    pub subtitle: String,
    pub preview_image: PreviewImageRef,
}

impl LoadingScreen {
    fn validate(&self) -> Result<(), WorldEnvironmentError> {
        require_non_empty("minimap.loading_screen.title", &self.title)?;
        // subtitle may be empty; only reject control chars if present.
        if self.subtitle.chars().any(char::is_control) {
            return invalid(
                "minimap.loading_screen.subtitle",
                "must not contain control characters",
            );
        }
        self.preview_image
            .validate("minimap.loading_screen.preview_image")
    }
}

/// The #816 authoring aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinimapAuthoring {
    pub id: String,
    pub display_name: String,
    pub author: String,
    pub source: MinimapSource,
    pub resolution_px: u32,
    /// True when the target is an RTS package that ships the minimap as data
    /// (#1783). Editor-preview-only authoring sets this false.
    pub rts_shipped_as_data: bool,
    pub loading_screen: LoadingScreen,
}

impl MinimapAuthoring {
    pub fn validate(&self) -> Result<(), WorldEnvironmentError> {
        require_non_empty("minimap.id", &self.id)?;
        require_non_empty("minimap.display_name", &self.display_name)?;
        require_non_empty("minimap.author", &self.author)?;
        if self.resolution_px == 0 || self.resolution_px > MAX_MINIMAP_DIM {
            return invalid(
                "minimap.resolution_px",
                "resolution must be within 1..=512 pixels per side",
            );
        }
        self.loading_screen.validate()
    }

    pub fn to_payload(&self) -> Result<EnvironmentPayload, WorldEnvironmentError> {
        self.validate()?;
        let fields = json!({
            "loading_preview_blob_sha256": self.loading_screen.preview_image.blob_sha256,
            "loading_preview_height_px": self.loading_screen.preview_image.height_px,
            "loading_preview_width_px": self.loading_screen.preview_image.width_px,
            "loading_subtitle": self.loading_screen.subtitle,
            "loading_title": self.loading_screen.title,
            "minimap_resolution_px": self.resolution_px,
            "minimap_source": self.source.as_str(),
            // #944 doctrine: authored minimap is creator/loading data, never a
            // default RPG player HUD map.
            "rpg_runtime_hud": false,
            "rts_shipped_as_data": self.rts_shipped_as_data,
        });
        EnvironmentPayload::new("minimap_loading", &self.id, fields)
    }
}

/// A frozen, fingerprinted generated minimap raster (the baked package data).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedMinimap {
    pub resolution_px: u32,
    /// Packed 0xRRGGBBAA pixels, row-major, `resolution_px * resolution_px` long.
    pub pixels: Vec<u32>,
    pub fingerprint: String,
}

/// Deterministically generate a square minimap by box-averaging an authored source
/// colour grid down to `resolution_px`. Frozen + fingerprinted: identical input
/// always yields identical output, so the baked artifact is reproducible.
pub fn generate_minimap(
    source: &[Rgba8],
    source_width: u32,
    source_height: u32,
    resolution_px: u32,
) -> Result<GeneratedMinimap, WorldEnvironmentError> {
    if source_width == 0 || source_height == 0 {
        return invalid("minimap.source", "source grid must be non-empty");
    }
    if resolution_px == 0 || resolution_px > MAX_MINIMAP_DIM {
        return invalid(
            "minimap.resolution_px",
            "resolution must be within 1..=512 pixels per side",
        );
    }
    let expected = (source_width as usize) * (source_height as usize);
    if source.len() != expected {
        return invalid(
            "minimap.source",
            "source grid length must equal width*height",
        );
    }
    let mut pixels = Vec::with_capacity((resolution_px * resolution_px) as usize);
    for py in 0..resolution_px {
        // Half-open source span for this minimap row (integer box filter).
        let y0 = (py as u64 * source_height as u64 / resolution_px as u64) as u32;
        let y1 =
            (((py + 1) as u64 * source_height as u64 / resolution_px as u64) as u32).max(y0 + 1);
        for px in 0..resolution_px {
            let x0 = (px as u64 * source_width as u64 / resolution_px as u64) as u32;
            let x1 =
                (((px + 1) as u64 * source_width as u64 / resolution_px as u64) as u32).max(x0 + 1);
            pixels.push(
                box_average(source, source_width, source_height, x0, x1, y0, y1).packed_rgba(),
            );
        }
    }
    let fingerprint = minimap_fingerprint(resolution_px, &pixels);
    Ok(GeneratedMinimap {
        resolution_px,
        pixels,
        fingerprint,
    })
}

fn box_average(
    source: &[Rgba8],
    width: u32,
    height: u32,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
) -> Rgba8 {
    let (mut r, mut g, mut b, mut a, mut count) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for y in y0..y1.min(height) {
        for x in x0..x1.min(width) {
            let texel = source[(y as usize) * (width as usize) + (x as usize)];
            r += u64::from(texel.r);
            g += u64::from(texel.g);
            b += u64::from(texel.b);
            a += u64::from(texel.a);
            count += 1;
        }
    }
    if count == 0 {
        return Rgba8::new(0, 0, 0, 0);
    }
    Rgba8::new(
        (r / count) as u8,
        (g / count) as u8,
        (b / count) as u8,
        (a / count) as u8,
    )
}

fn minimap_fingerprint(resolution_px: u32, pixels: &[u32]) -> String {
    let mut bytes = Vec::with_capacity(4 + pixels.len() * 4);
    bytes.extend_from_slice(&resolution_px.to_be_bytes());
    for pixel in pixels {
        bytes.extend_from_slice(&pixel.to_be_bytes());
    }
    litd_hash::sha256_hex(&bytes)
}
