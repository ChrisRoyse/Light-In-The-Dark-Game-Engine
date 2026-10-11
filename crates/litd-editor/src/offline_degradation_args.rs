use super::*;

pub(super) struct ReadbackArgs<'a, T0: Into<String>> {
    pub(super) probe: &'a CalyxAvailabilityProbe,
    pub(super) home: Option<PathBuf>,
    pub(super) home_exists: bool,
    pub(super) home_is_dir: bool,
    pub(super) home_readable: bool,
    pub(super) endpoint: Option<CalyxEndpointReadback>,
    pub(super) tool_count: usize,
    pub(super) required_tools_present: Vec<String>,
    pub(super) reason_code: &'a str,
    pub(super) reason: T0,
}
