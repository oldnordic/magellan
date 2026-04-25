//! Cross-platform external tool detection
//!
//! Finds clang and javac executables on Linux and Windows.
//! Searches PATH and common installation locations.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
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
        let result = find_clang();
        if result.is_ok() {
            let path = result.unwrap();
            assert!(path.exists());
        }
    }

    #[test]
    fn test_find_javac() {
        // Test may fail if javac not installed - that's ok
        // We're just testing the function works
        let result = find_javac();
        if result.is_ok() {
            let path = result.unwrap();
            assert!(path.exists());
        }
    }
}
