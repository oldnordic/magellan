//! Version and build information for Magellan
//!
//! Provides version string and build metadata (commit SHA, build date, rustc version).

/// Get the full version string including build metadata
///
/// Returns format: "magellan {version} ({commit} {date}) rustc {rustc_version} backends: {backends}"
pub fn version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let commit = build_commit();
    let date = build_date();
    let rustc_version = rustc_version();
    let backends = compiled_backends();

    format!(
        "magellan {} ({} {}) rustc {} backends: {}",
        version, commit, date, rustc_version, backends
    )
}

/// Get the list of compiled backend features
///
/// Returns a comma-separated list like "sqlite" or "sqlite,geometric"
fn compiled_backends() -> String {
    let mut backends = Vec::new();

    #[cfg(feature = "sqlite-backend")]
    backends.push("sqlite");

    #[cfg(feature = "geometric-backend")]
    backends.push("geometric");

    #[cfg(feature = "native-v3")]
    backends.push("native-v3");

    if backends.is_empty() {
        // Fallback - at least sqlite should be available
        backends.push("sqlite");
    }

    backends.join(",")
}

/// Get the package version (e.g., "2.2.0")
///
/// Public API for programmatic version access - may be used by external tools.
#[expect(dead_code)]
pub fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Get the package version (e.g., "2.2.0")

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
