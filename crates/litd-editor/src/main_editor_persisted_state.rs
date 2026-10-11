use super::*;

impl EditorShellRuntime {
    /// Read the persisted shell-state JSON bytes back from the Calyx Aster vault that
    /// backs this runtime. The shell state migrated from a flat JSON file to an Aster
    /// vault directory; tests must read it through the vault API, not `fs::read_to_string`
    /// on the directory (which fails with PermissionDenied). Source-of-truth reader for
    /// editor UI FSV tests.
    pub(crate) fn persisted_shell_state_bytes(&self) -> Vec<u8> {
        litd_editor::editor_state_store::EditorStateStore::new(std::path::PathBuf::from(
            self.shell_state.path_display(),
        ))
        .read_shell_state()
        .expect("read shell state from Aster vault")
        .expect("shell state must exist in the Aster vault")
    }

    pub(crate) fn persisted_shell_state_json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.persisted_shell_state_bytes())
            .expect("persisted shell state must parse as JSON")
    }
}
