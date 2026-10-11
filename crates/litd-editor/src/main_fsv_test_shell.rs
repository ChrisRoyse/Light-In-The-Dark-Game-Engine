#[derive(Debug, Clone)]
pub struct EditorShellRuntimeEvidence {
    pub frontend_kind: &'static str,
    pub layout_units: u64,
    pub active_mode: &'static str,
    pub active_inspector_title: &'static str,
}

pub fn format_shell_stats(_shell: &EditorShellRuntimeEvidence) -> String {
    String::new()
}
