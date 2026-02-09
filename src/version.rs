//! Version and build information for Magellan
//!
//! Provides version string and build metadata (commit SHA, build date, rustc version).

/// Get the full version string including build metadata
///
/// Returns format: "magellan {version} ({commit} {date}) rustc {rustc_version}"
pub fn version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let commit = build_commit();
    let date = build_date();
    let rustc_version = rustc_version();

    format!("magellan {} ({} {}) rustc {}", version, commit, date, rustc_version)
}

/// Get the package version (e.g., "2.2.0")
pub fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Get the build commit SHA
///
/// Returns "unknown" if not built with commit info
pub fn build_commit() -> &'static str {
    option_env!("MAGELLAN_COMMIT_SHA").unwrap_or("unknown")
}

/// Get the build date
///
/// Returns "unknown" if not built with date info
pub fn build_date() -> &'static str {
    option_env!("MAGELLAN_BUILD_DATE").unwrap_or("unknown")
}

/// Get the Rust compiler version used for the build
///
/// Returns "unknown" if not built with rustc version info
pub fn rustc_version() -> &'static str {
    option_env!("MAGELLAN_RUSTC_VERSION").unwrap_or("unknown")
}
