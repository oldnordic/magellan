// Platform detection and feature flags

#[cfg(windows)]
pub const IS_WINDOWS: bool = true;

#[cfg(not(windows))]
pub const IS_WINDOWS: bool = false;

#[cfg(unix)]
pub const IS_UNIX: bool = true;

#[cfg(not(unix))]
pub const IS_UNIX: bool = false;

/// Warn users about Windows limitations on first run
pub fn check_platform_support() {
    if IS_WINDOWS {
        eprintln!("=== Windows Support Notice ===");
        eprintln!("Windows support is analysis-only:");
        eprintln!("  - File watching may have reduced performance");
        eprintln!("  - No signal handling (Ctrl+C behavior varies)");
        eprintln!("  - No background process management");
        eprintln!();
        eprintln!("For production use on Windows, consider:");
        eprintln!("  - Using WSL2 with native Linux builds");
        eprintln!("  - Manual reindex instead of watch mode");
        eprintln!("==================================");
    }
}

/// Check if watch mode is supported on this platform
pub fn watch_mode_supported() -> bool {
    if IS_WINDOWS {
        eprintln!("Warning: Watch mode on Windows may have reduced performance.");
        eprintln!("         Use manual reindex for large codebases.");
        true
    } else {
        true
    }
}
