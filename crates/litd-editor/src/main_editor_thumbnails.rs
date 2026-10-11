use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
};

use egui::{Color32, Rect, Stroke, StrokeKind, TextureHandle, TextureOptions};
use image::GenericImageView;
use litd_editor::asset_vault::AssetVaultRecord;
use litd_engine::EngineError;

use crate::main_editor_assets::{AssetIcon, draw_asset_icon};

const PREVIEW_NAMES: &[&str] = &[
    "Preview.png",
    "Preview.jpg",
    "Preview.jpeg",
    "preview.png",
    "preview.jpg",
    "contents.png",
    "Contents.png",
];

#[derive(Debug, Clone)]
pub struct AssetThumbnail {
    path: PathBuf,
    relative_path: String,
}

#[derive(Clone, Default)]
pub struct ThumbnailCache {
    textures: BTreeMap<PathBuf, TextureHandle>,
    failed_paths: BTreeSet<PathBuf>,
}

impl fmt::Debug for ThumbnailCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ThumbnailCache")
            .field("loaded_count", &self.textures.len())
            .field("error_count", &self.failed_paths.len())
            .finish()
    }
}

impl AssetThumbnail {
    pub fn resolve(record: &AssetVaultRecord) -> Result<Option<Self>, EngineError> {
        let vault_root = vault_root_for_record(record)?;
        if is_image_path(&record.absolute_path) {
            return thumbnail_from_path(&vault_root, &record.absolute_path).map(Some);
        }
        let mut current = record.absolute_path.parent();
        while let Some(directory) = current {
            if !directory.starts_with(&vault_root) {
                break;
            }
            for name in PREVIEW_NAMES {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    return thumbnail_from_path(&vault_root, &candidate).map(Some);
                }
            }
            current = directory.parent();
        }
        Ok(None)
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ThumbnailCache {
    pub fn preload_first<'a, I>(&mut self, ctx: &egui::Context, thumbnails: I, limit: usize)
    where
        I: IntoIterator<Item = &'a AssetThumbnail>,
    {
        for thumbnail in thumbnails {
            if self.textures.len() >= limit {
                break;
            }
            if self.textures.contains_key(&thumbnail.path)
                || self.failed_paths.contains(&thumbnail.path)
            {
                continue;
            }
            if let Err(reason) = self.texture_for(ctx, thumbnail) {
                tracing::error!(
                    path = %thumbnail.path.display(),
                    error = reason,
                    "asset thumbnail preload failed"
                );
            }
        }
    }

    pub fn draw_thumbnail_or_error(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: Rect,
        thumbnail: &AssetThumbnail,
        fallback_icon: AssetIcon,
        selected: bool,
    ) {
        match self.texture_for(ctx, thumbnail) {
            Ok(texture) => draw_texture_tile(painter, rect, texture, selected),
            Err(reason) => {
                tracing::error!(
                    path = %thumbnail.path.display(),
                    error = reason,
                    "asset thumbnail decode failed"
                );
                draw_asset_icon(painter, rect, fallback_icon, selected);
                draw_error_overlay(painter, rect);
            }
        }
    }

    pub fn loaded_count(&self) -> usize {
        self.textures.len()
    }

    pub fn error_count(&self) -> usize {
        self.failed_paths.len()
    }

    fn texture_for(
        &mut self,
        ctx: &egui::Context,
        thumbnail: &AssetThumbnail,
    ) -> Result<&TextureHandle, String> {
        if self.failed_paths.contains(&thumbnail.path) {
            return Err("thumbnail previously failed to decode".to_string());
        }
        if !self.textures.contains_key(&thumbnail.path) {
            let image = load_color_image(&thumbnail.path).inspect_err(|_reason| {
                self.failed_paths.insert(thumbnail.path.clone());
            })?;
            let name = format!("asset-thumb:{}", thumbnail.relative_path);
            let texture = ctx.load_texture(name, image, TextureOptions::LINEAR);
            self.textures.insert(thumbnail.path.clone(), texture);
        }
        self.textures
            .get(&thumbnail.path)
            .ok_or_else(|| "thumbnail texture cache insert was not readable".to_string())
    }
}

fn thumbnail_from_path(vault_root: &Path, path: &Path) -> Result<AssetThumbnail, EngineError> {
    if !path.starts_with(vault_root) {
        return thumbnail_error(
            "path",
            format!(
                "thumbnail {} escapes vault root {}",
                path.display(),
                vault_root.display()
            ),
        );
    }
    let metadata = fs::metadata(path).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_thumbnail.path".to_string(),
        reason: format!("failed to stat {}: {error}", path.display()),
    })?;
    if metadata.len() == 0 {
        return thumbnail_error("bytes", format!("thumbnail {} is empty", path.display()));
    }
    let relative_path = path
        .strip_prefix(vault_root)
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_thumbnail.relative_path".to_string(),
            reason: format!("failed to relativize {}: {error}", path.display()),
        })?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(AssetThumbnail {
        path: path.to_path_buf(),
        relative_path,
    })
}

fn vault_root_for_record(record: &AssetVaultRecord) -> Result<PathBuf, EngineError> {
    let relative = Path::new(&record.relative_path);
    let mut root = record.absolute_path.clone();
    for _ in relative.components() {
        if !root.pop() {
            return thumbnail_error(
                "vault_root",
                format!(
                    "record path {} does not contain relative path {}",
                    record.absolute_path.display(),
                    record.relative_path
                ),
            );
        }
    }
    Ok(root)
}

fn load_color_image(path: &Path) -> Result<egui::ColorImage, String> {
    let image = image::ImageReader::open(path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?
        .with_guessed_format()
        .map_err(|error| format!("failed to guess format for {}: {error}", path.display()))?
        .decode()
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err(format!(
            "decoded thumbnail {} has zero extent",
            path.display()
        ));
    }
    let rgba = image.to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        rgba.as_raw(),
    ))
}

fn draw_texture_tile(painter: &egui::Painter, rect: Rect, texture: &TextureHandle, selected: bool) {
    painter.rect_filled(rect, 4, Color32::from_rgb(12, 16, 22));
    let image_rect = rect.shrink(4.0);
    painter.image(
        texture.id(),
        image_rect,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    painter.rect_stroke(
        rect,
        4,
        Stroke::new(
            1.0,
            if selected {
                Color32::from_rgb(222, 179, 98)
            } else {
                Color32::from_rgb(78, 70, 52)
            },
        ),
        StrokeKind::Inside,
    );
}

fn draw_error_overlay(painter: &egui::Painter, rect: Rect) {
    painter.line_segment(
        [rect.left_top(), rect.right_bottom()],
        Stroke::new(2.0, Color32::from_rgb(244, 96, 43)),
    );
}

fn is_image_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    matches!(extension.as_deref(), Some("png" | "jpg" | "jpeg"))
}

fn thumbnail_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_thumbnail.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use litd_editor::asset_vault::AssetVaultCatalog;

    const TEST_WARM_LIMIT: usize = 24;

    #[test]
    fn preload_first_loads_bounded_real_asset_thumbnails() {
        let thumbnails = first_unique_real_thumbnails(TEST_WARM_LIMIT);
        let ctx = egui::Context::default();
        let mut cache = ThumbnailCache::default();

        cache.preload_first(&ctx, thumbnails.iter(), 12);
        assert_eq!(cache.loaded_count(), 12);
        assert_eq!(cache.error_count(), 0);

        cache.preload_first(&ctx, thumbnails.iter(), TEST_WARM_LIMIT);
        assert_eq!(cache.loaded_count(), TEST_WARM_LIMIT);
        assert_eq!(cache.error_count(), 0);
    }

    fn first_unique_real_thumbnails(limit: usize) -> Vec<AssetThumbnail> {
        let catalog = AssetVaultCatalog::load_default_cached()
            .expect("real asset vault catalog must load for thumbnail warm test");
        let mut thumbnails = Vec::new();
        let mut paths = BTreeSet::new();
        for record in catalog.editor_palette_records(catalog.usable_record_count()) {
            let Some(thumbnail) =
                AssetThumbnail::resolve(record).expect("real asset thumbnail must resolve")
            else {
                continue;
            };
            if paths.insert(thumbnail.path.clone()) {
                thumbnails.push(thumbnail);
            }
            if thumbnails.len() == limit {
                break;
            }
        }
        assert_eq!(
            thumbnails.len(),
            limit,
            "real asset vault must provide {limit} unique thumbnails"
        );
        thumbnails
    }
}
