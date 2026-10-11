fn main() {
    require_windows_target("litd-engine");
}

fn require_windows_target(package: &str) {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        panic!(
            "LITD_WINDOWS_PLATFORM_REQUIRED: {package} supports native Windows only; requested target_os={target_os}"
        );
    }
}
