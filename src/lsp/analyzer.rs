//! LSP analyzer detection and execution
//!
//! Detects available LSP tools and runs them as CLI commands to extract
//! type signatures and documentation for symbols.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Supported LSP analyzers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalyzerKind {
    /// Rust Analyzer (rust-analyzer)
    RustAnalyzer,
    /// Java Language Server (jdtls)
    JDTLS,
    /// C/C++ Language Server (clangd)
    Clangd,
}

impl AnalyzerKind {
    /// Get the binary name for this analyzer
    pub fn binary_name(&self) -> &'static str {
        match self {
            AnalyzerKind::RustAnalyzer => "rust-analyzer",
            AnalyzerKind::JDTLS => "jdtls",
            AnalyzerKind::Clangd => "clangd",
        }
    }

    /// Get the language ID for this analyzer
    pub fn language_id(&self) -> &'static str {
        match self {
            AnalyzerKind::RustAnalyzer => "rust",
            AnalyzerKind::JDTLS => "java",
            AnalyzerKind::Clangd => "cpp",
        }
    }

    /// Check if this analyzer is available in PATH
    pub fn is_available(&self) -> bool {
        which::which(self.binary_name()).is_ok()
    }

    /// Get version info for this analyzer
    pub fn get_version(&self) -> Option<String> {
        let output = Command::new(self.binary_name())
            .arg("--version")
            .output()
            .ok()?;
        
        String::from_utf8(output.stdout).ok()
    }

    /// Analyze a file and extract symbol information
    pub fn analyze_file(&self, file_path: &Path, workspace_root: &Path) -> Result<AnalyzerResult> {
        match self {
            AnalyzerKind::RustAnalyzer => self.analyze_with_rust_analyzer(file_path, workspace_root),
            AnalyzerKind::JDTLS => self.analyze_with_jdtls(file_path, workspace_root),
            AnalyzerKind::Clangd => self.analyze_with_clangd(file_path, workspace_root),
        }
    }

    /// Analyze with rust-analyzer
    fn analyze_with_rust_analyzer(&self, file_path: &Path, workspace_root: &Path) -> Result<AnalyzerResult> {
        // Use rust-analyzer's analysis-stats command
        let output = Command::new("rust-analyzer")
            .args(["analysis-stats", "--load-output-dirs"])
            .arg(file_path)
            .current_dir(workspace_root)
            .output()
            .with_context(|| format!("Failed to run rust-analyzer on {:?}", file_path))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(AnalyzerResult {
            analyzer: *self,
            raw_output: format!("{}{}", stdout, stderr),
            success: output.status.success(),
        })
    }

    /// Analyze with jdtls
    fn analyze_with_jdtls(&self, file_path: &Path, workspace_root: &Path) -> Result<AnalyzerResult> {
        // jdtls is typically run as a server, but we can use it via CLI for checks
        let output = Command::new("jdtls")
            .args(["--check"])
            .arg(file_path)
            .current_dir(workspace_root)
            .output()
            .with_context(|| format!("Failed to run jdtls on {:?}", file_path))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(AnalyzerResult {
            analyzer: *self,
            raw_output: format!("{}{}", stdout, stderr),
            success: output.status.success(),
        })
    }

    /// Analyze with clangd
    fn analyze_with_clangd(&self, file_path: &Path, workspace_root: &Path) -> Result<AnalyzerResult> {
        // Use clangd's --check mode
        let output = Command::new("clangd")
            .args(["--check"])
            .arg(file_path)
            .current_dir(workspace_root)
            .output()
            .with_context(|| format!("Failed to run clangd on {:?}", file_path))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(AnalyzerResult {
            analyzer: *self,
            raw_output: format!("{}{}", stdout, stderr),
            success: output.status.success(),
        })
    }
}

/// Result from running an analyzer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerResult {
    /// Which analyzer was used
    pub analyzer: AnalyzerKind,
    /// Raw output from the analyzer
    pub raw_output: String,
    /// Whether the analyzer completed successfully
    pub success: bool,
}

/// Detect all available analyzers
pub fn detect_available_analyzers() -> Vec<AnalyzerKind> {
    let mut analyzers = Vec::new();
    
    for kind in [AnalyzerKind::RustAnalyzer, AnalyzerKind::JDTLS, AnalyzerKind::Clangd] {
        if kind.is_available() {
            analyzers.push(kind);
        }
    }
    
    analyzers
}

/// Get analyzer for a specific language
pub fn get_analyzer_for_language(language: &str) -> Option<AnalyzerKind> {
    match language {
        "rust" => Some(AnalyzerKind::RustAnalyzer),
        "java" => Some(AnalyzerKind::JDTLS),
        "c" | "cpp" | "c++" | "h" | "hpp" => Some(AnalyzerKind::Clangd),
        _ => None,
    }
}

/// Detect language from file path
pub fn detect_language_from_path(file_path: &Path) -> Option<&'static str> {
    let ext = file_path.extension().and_then(|e| e.to_str())?;
    
    match ext {
        "rs" => Some("rust"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyzer_binary_names() {
        assert_eq!(AnalyzerKind::RustAnalyzer.binary_name(), "rust-analyzer");
        assert_eq!(AnalyzerKind::JDTLS.binary_name(), "jdtls");
        assert_eq!(AnalyzerKind::Clangd.binary_name(), "clangd");
    }

    #[test]
    fn test_detect_available_analyzers() {
        let analyzers = detect_available_analyzers();
        // At least one analyzer should be available (or none if none installed)
        // This test just verifies the function doesn't panic
        assert!(analyzers.len() <= 3);
    }

    #[test]
    fn test_get_analyzer_for_language() {
        assert_eq!(get_analyzer_for_language("rust"), Some(AnalyzerKind::RustAnalyzer));
        assert_eq!(get_analyzer_for_language("java"), Some(AnalyzerKind::JDTLS));
        assert_eq!(get_analyzer_for_language("cpp"), Some(AnalyzerKind::Clangd));
        assert_eq!(get_analyzer_for_language("python"), None);
    }

    #[test]
    fn test_detect_language_from_path() {
        assert_eq!(detect_language_from_path(Path::new("test.rs")), Some("rust"));
        assert_eq!(detect_language_from_path(Path::new("Test.java")), Some("java"));
        assert_eq!(detect_language_from_path(Path::new("main.cpp")), Some("cpp"));
        assert_eq!(detect_language_from_path(Path::new("script.py")), None);
    }
}
