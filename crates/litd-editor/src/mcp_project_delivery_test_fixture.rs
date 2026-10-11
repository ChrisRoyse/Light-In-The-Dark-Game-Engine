use std::path::PathBuf;

pub(super) struct FixturePaths {
    pub(super) root: PathBuf,
    pub(super) state_path: PathBuf,
    pub(super) project_path: PathBuf,
    pub(super) package_path: PathBuf,
    pub(super) catalog_path: PathBuf,
    pub(super) thumbnail_path: PathBuf,
    pub(super) delivery_manifest_path: PathBuf,
    pub(super) blob_root_path: PathBuf,
    pub(super) install_manifest_path: PathBuf,
    pub(super) install_root_path: PathBuf,
}
