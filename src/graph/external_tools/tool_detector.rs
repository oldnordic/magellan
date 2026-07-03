//! Cross-platform external tool detection
//!
//! Finds clang and javac executables on Linux and Windows.
//! Searches PATH and common installation locations.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

/// Errors from tool detection
#[derive(Debug, thiserror::Error)]
pub enum ToolDetectionError {
    #[error("Tool not found: {tool}")]
    ToolNotFound { tool: String },

    #[error("Tool execution failed: {tool}")]
    ExecutionFailed { tool: String, reason: String },

    #[error("Version check failed: {tool}")]
    VersionCheckFailed { tool: String, reason: String },
}

/// Find clang executable on the system
pub fn find_clang() -> Result<PathBuf, ToolDetectionError> {
    let exe_name = get_executable_name("clang");

    // Try PATH first
    if let Ok(path) = which::which(&exe_name) {
        return Ok(path);
    }

    // Search platform-specific common locations
    #[cfg(unix)]
    let common_paths = search_unix_common_paths("clang");

    #[cfg(windows)]
    let common_paths = search_windows_common_paths("clang");

    #[cfg(not(any(unix, windows)))]
    let common_paths: Vec<PathBuf> = vec![];

    for path in common_paths {
        if path.exists() {
            return Ok(path);
        }
    }

    Err(ToolDetectionError::ToolNotFound {
        tool: "clang".to_string(),
    })
}

/// Find javac executable on the system
pub fn find_javac() -> Result<PathBuf, ToolDetectionError> {
    let exe_name = get_executable_name("javac");

    // Try PATH first
    if let Ok(path) = which::which(&exe_name) {
        return Ok(path);
    }

    // Search platform-specific common locations
    #[cfg(unix)]
    let common_paths = search_unix_common_paths("javac");

    #[cfg(windows)]
    let common_paths = search_windows_common_paths("javac");

    #[cfg(not(any(unix, windows)))]
    let common_paths: Vec<PathBuf> = vec![];

    for path in common_paths {
        if path.exists() {
            return Ok(path);
        }
    }

    Err(ToolDetectionError::ToolNotFound {
        tool: "javac".to_string(),
    })
}

/// Find the nightly rustc executable via rustup.
///
/// Magellan itself compiles on stable; nightly is only required at runtime for
/// MIR-based CFG extraction (`-Zunpretty=mir`). This locates the nightly rustc
/// binary without adding a nightly build dependency.
///
/// Primary strategy: ask `rustup` which rustc the nightly toolchain resolves to.
/// Fallback: scan `~/.rustup/toolchains/nightly-*/bin/rustc` directly.
pub fn find_rustc_nightly() -> Result<PathBuf, ToolDetectionError> {
    // Primary: rustup knows the exact toolchain path, including the host triple.
    let rustup_output = Command::new("rustup")
        .args(["which", "--toolchain", "nightly", "rustc"])
        .output();

    if let Ok(output) = rustup_output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let trimmed = stdout.trim();
            if !trimmed.is_empty() {
                let path = PathBuf::from(trimmed);
                if path.exists() {
                    return Ok(path);
                }
            }
        }
    }

    // Fallback: scan the rustup toolchains directory for a nightly-* toolchain.
    // Matches the glob `~/.rustup/toolchains/nightly-*/bin/rustc` without pulling
    // in an extra crate. Resolves the home directory the same way `rustup` does:
    // `$HOME` on Unix, `%USERPROFILE%` on Windows.
    let home = std::env::var_os(home_env_var());
    if let Some(home) = home {
        let toolchains_dir = PathBuf::from(home).join(".rustup").join("toolchains");
        if let Ok(entries) = std::fs::read_dir(&toolchains_dir) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let Some(name) = file_name.to_str() else {
                    continue;
                };
                if !name.starts_with("nightly-") {
                    continue;
                }
                let rustc_path = entry.path().join("bin").join("rustc");
                if rustc_path.exists() {
                    return Ok(rustc_path);
                }
            }
        }
    }

    Err(ToolDetectionError::ToolNotFound {
        tool: "rustc (nightly toolchain)".to_string(),
    })
}

/// Get the nightly rustc version string.
///
/// Runs `rustup run nightly rustc --version` and returns the captured stdout
/// (e.g. `"rustc 1.97.0-nightly (f964de49b 2026-05-07)"`).
pub fn check_rustc_nightly_version() -> Result<String, ToolDetectionError> {
    let output = Command::new("rustup")
        .args(["run", "nightly", "rustc", "--version"])
        .output()
        .map_err(|e| ToolDetectionError::ExecutionFailed {
            tool: "rustc (nightly toolchain)".to_string(),
            reason: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(ToolDetectionError::ExecutionFailed {
            tool: "rustc (nightly toolchain)".to_string(),
            reason: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let version =
        String::from_utf8(output.stdout).map_err(|e| ToolDetectionError::VersionCheckFailed {
            tool: "rustc (nightly toolchain)".to_string(),
            reason: e.to_string(),
        })?;

    Ok(version.trim().to_string())
}

/// Get clang version information
pub fn check_clang_version() -> Result<String, ToolDetectionError> {
    let clang_path = find_clang()?;

    let output = Command::new(&clang_path)
        .arg("--version")
        .output()
        .map_err(|e| ToolDetectionError::ExecutionFailed {
            tool: "clang".to_string(),
            reason: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(ToolDetectionError::ExecutionFailed {
            tool: "clang".to_string(),
            reason: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    String::from_utf8(output.stdout).map_err(|e| ToolDetectionError::VersionCheckFailed {
        tool: "clang".to_string(),
        reason: e.to_string(),
    })
}

/// Get javac version information
pub fn check_javac_version() -> Result<String, ToolDetectionError> {
    let javac_path = find_javac()?;

    let output = Command::new(&javac_path)
        .arg("-version")
        .output()
        .map_err(|e| ToolDetectionError::ExecutionFailed {
            tool: "javac".to_string(),
            reason: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(ToolDetectionError::ExecutionFailed {
            tool: "javac".to_string(),
            reason: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    String::from_utf8(output.stderr).map_err(|e| ToolDetectionError::VersionCheckFailed {
        tool: "javac".to_string(),
        reason: e.to_string(),
    })
}

/// Check if a specific tool is available
pub fn is_tool_available(tool_name: &str) -> bool {
    let exe_name = get_executable_name(tool_name);
    which::which(&exe_name).is_ok()
}

/// Get platform-specific executable name
#[cfg(unix)]
pub fn get_executable_name(name: &str) -> String {
    name.to_string()
}

#[cfg(windows)]
pub fn get_executable_name(name: &str) -> String {
    format!("{}.exe", name)
}

/// Name of the environment variable holding the user's home directory.
#[cfg(unix)]
fn home_env_var() -> &'static str {
    "HOME"
}

#[cfg(windows)]
fn home_env_var() -> &'static str {
    "USERPROFILE"
}

/// Search common Unix installation paths for a tool
#[cfg(unix)]
fn search_unix_common_paths(tool: &str) -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/bin/").join(tool),
        PathBuf::from("/usr/local/bin/").join(tool),
        PathBuf::from("/opt/llvm/bin/").join(tool),
        PathBuf::from("/opt/homebrew/bin/").join(tool),
        PathBuf::from("/opt/homebrew/opt/llvm/bin/").join(tool),
    ]
}

/// Search common Windows installation paths for a tool
#[cfg(windows)]
fn search_windows_common_paths(tool: &str) -> Vec<PathBuf> {
    let tool_exe = get_executable_name(tool);

    let mut paths = vec![
        // LLVM installer paths
        PathBuf::from("C:\\Program Files\\LLVM\\bin\\").join(&tool_exe),
        PathBuf::from("C:\\Program Files (x86)\\LLVM\\bin\\").join(&tool_exe),
    ];

    // Add common JDK installation paths
    // Note: We can't use glob patterns directly, so we check common locations
    let jdk_base_paths = vec![
        "C:\\Program Files\\Java\\",
        "C:\\Program Files (x86)\\Java\\",
        "C:\\Program Files\\Eclipse Adoptium\\",
        "C:\\Program Files\\Eclipse Adoptium\\jdk-",
    ];

    for base in jdk_base_paths {
        // Check for recent JDK versions (11-21)
        for version in 11..=21 {
            let path = PathBuf::from(format!("{}{}\\bin\\{}", base, version, tool_exe));
            paths.push(path);
        }
        // Try "latest" symlink
        let path = PathBuf::from(format!("{}latest\\bin\\{}", base, tool_exe));
        paths.push(path);
    }

    paths
}

/// Get platform-specific installation instructions for clang
pub fn get_clang_install_instructions() -> &'static str {
    if cfg!(unix) {
        r#"
Linux installation:
  Ubuntu/Debian: sudo apt install clang
  Fedora: sudo dnf install clang
  Arch: sudo pacman -S clang

macOS installation:
  brew install llvm
"#
    } else if cfg!(windows) {
        r#"
Windows installation:
  Download from: https://releases.llvm.org/download.html
  Or install via: winget install LLVM.LLVM

  Make sure to add LLVM to your PATH during installation.
"#
    } else {
        "Please install clang for your platform."
    }
}

/// Get platform-specific installation instructions for javac
pub fn get_javac_install_instructions() -> &'static str {
    if cfg!(unix) {
        r#"
Linux installation:
  Ubuntu/Debian: sudo apt install default-jdk
  Fedora: sudo dnf install java-devel
  Arch: sudo pacman -S jdk-openjdk

macOS installation:
  brew install openjdk
"#
    } else if cfg!(windows) {
        r#"
Windows installation:
  Download from: https://adoptium.net/ (Eclipse Temurin JDK)
  Or install via: winget install EclipseAdoptium.Temurin.17.JDK

  Make sure to add JDK to your PATH during installation.
"#
    } else {
        "Please install JDK for your platform."
    }
}

/// Get installation instructions for the nightly Rust toolchain.
///
/// Unlike clang/javac, nightly Rust is installed via `rustup` rather than a
/// system package manager, so the instructions are platform-agnostic.
pub fn get_rustc_nightly_install_instructions() -> &'static str {
    "Install nightly with: rustup toolchain install nightly"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_executable_name_unix() {
        #[cfg(unix)]
        assert_eq!(get_executable_name("clang"), "clang");

        #[cfg(windows)]
        assert_eq!(get_executable_name("clang"), "clang.exe");
    }

    #[test]
    fn test_is_tool_available() {
        // This test just verifies the function doesn't panic
        // Results depend on what's installed on the system
        let _ = is_tool_available("clang");
        let _ = is_tool_available("javac");
    }

    #[test]
    fn test_find_clang() {
        // Test may fail if clang not installed - that's ok
        // We're just testing the function works
        if let Ok(path) = find_clang() {
            assert!(path.exists());
        }
    }

    #[test]
    fn test_find_javac() {
        if let Ok(path) = find_javac() {
            assert!(path.exists());
        }
    }

    #[test]
    fn test_find_rustc_nightly() {
        // Nightly is installed on this machine (rustc 1.97.0-nightly), so
        // detection should succeed and resolve to an existing rustc binary.
        let path = find_rustc_nightly().expect("nightly rustc should be detectable");
        assert!(path.exists(), "detected nightly rustc path should exist");
        assert!(
            path.file_name()
                .is_some_and(|n| n == "rustc" || n == "rustc.exe"),
            "detected path should be a rustc binary: {}",
            path.display()
        );
    }

    #[test]
    fn test_check_rustc_nightly_version() {
        let version =
            check_rustc_nightly_version().expect("nightly rustc version should be obtainable");
        assert!(
            version.contains("nightly"),
            "version string should mention 'nightly': {}",
            version
        );
    }
}
