//! C/C++ to LLVM IR compilation via clang
//!
//! Compiles C/C++ source files to LLVM IR text files (.ll) using clang.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::graph::external_tools::{tool_detector, tool_invoker};

/// Errors from clang compilation
#[derive(Debug, thiserror::Error)]
pub enum ClangCompilationError {
    #[error("Clang not found: {0}")]
    ClangNotFound(String),

    #[error("Compilation failed: {0}")]
    CompilationFailed(String),

    #[error("Unsupported file type: {extension}")]
    UnsupportedFileType { extension: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Source language detected from file extension
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    C,
    Cpp,
    ObjectiveC,
    ObjectiveCpp,
}

impl SourceLanguage {
    /// Get the clang language flag for this language
    pub fn clang_flag(&self) -> &'static str {
        match self {
            SourceLanguage::C => "-xc",
            SourceLanguage::Cpp => "-xc++",
            SourceLanguage::ObjectiveC => "-xobjective-c",
            SourceLanguage::ObjectiveCpp => "-xobjective-c++",
        }
    }

    /// Get the file extensions for this language
    pub fn extensions() -> &'static [&'static str] {
        &["c", "cpp", "cc", "cxx", "h", "hpp", "m", "mm"]
    }
}

/// Detect source language from file extension
pub fn detect_source_language(path: &Path) -> Result<SourceLanguage, ClangCompilationError> {
    let extension = path.extension().and_then(|e| e.to_str()).ok_or_else(|| {
        ClangCompilationError::UnsupportedFileType {
            extension: "<none>".to_string(),
        }
    })?;

    match extension.to_lowercase().as_str() {
        "c" => Ok(SourceLanguage::C),
        "cpp" | "cc" | "cxx" | "hpp" => Ok(SourceLanguage::Cpp),
        "m" => Ok(SourceLanguage::ObjectiveC),
        "mm" => Ok(SourceLanguage::ObjectiveCpp),
        _ => Err(ClangCompilationError::UnsupportedFileType {
            extension: extension.to_string(),
        }),
    }
}

/// Compile a C/C++ source file to LLVM IR
///
/// # Arguments
///
/// * `source_path` - Path to the C/C++ source file
/// * `output_path` - Path where the .ll file should be written
///
/// # Returns
///
/// Ok(()) if compilation succeeds
///
/// # Errors
///
/// Returns error if clang is not found or compilation fails
pub fn compile_to_llvm_ir(
    source_path: &Path,
    output_path: &Path,
) -> Result<(), ClangCompilationError> {
    // Find clang executable
    let clang_path = tool_detector::find_clang().map_err(|e| match e {
        tool_detector::ToolDetectionError::ToolNotFound { .. } => {
            ClangCompilationError::ClangNotFound("clang not found in PATH".to_string())
        }
        _ => ClangCompilationError::ClangNotFound(format!("clang detection failed: {}", e)),
    })?;

    // Detect source language
    let language = detect_source_language(source_path)?;

    // Create output directory if it doesn't exist
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ClangCompilationError::Io(e))?;
    }

    // Build clang command
    let output = Command::new(&clang_path)
        .arg(language.clang_flag()) // Set language explicitly
        .arg("-S") // Generate assembly
        .arg("-emit-llvm") // Emit LLVM IR
        .arg("-o") // Output file
        .arg(output_path)
        .arg(source_path)
        .output()
        .map_err(|e| ClangCompilationError::Io(e))?;

    // Check if compilation succeeded
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ClangCompilationError::CompilationFailed(stderr.to_string()));
    }

    // Verify output file was created
    if !output_path.exists() {
        return Err(ClangCompilationError::CompilationFailed(
            "LLVM IR file was not created".to_string(),
        ));
    }

    Ok(())
}

/// Compile a C/C++ source file to LLVM IR in a temporary directory
///
/// This is a convenience function that creates a temporary .ll file
/// and returns its path. The caller is responsible for cleaning it up.
///
/// # Arguments
///
/// * `source_path` - Path to the C/C++ source file
///
/// # Returns
///
/// Path to the temporary .ll file
///
/// # Errors
///
/// Returns error if clang is not found or compilation fails
pub fn compile_to_llvm_ir_temp(source_path: &Path) -> Result<PathBuf, ClangCompilationError> {
    // Create a temporary file with .ll extension
    let source_stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let temp_dir = std::env::temp_dir();
    let output_path = temp_dir.join(format!("{}_{}.ll", source_stem, std::process::id()));

    compile_to_llvm_ir(source_path, &output_path)?;

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_detect_source_language_c() {
        assert_eq!(
            detect_source_language(Path::new("test.c")).unwrap(),
            SourceLanguage::C
        );
    }

    #[test]
    fn test_detect_source_language_cpp() {
        assert_eq!(
            detect_source_language(Path::new("test.cpp")).unwrap(),
            SourceLanguage::Cpp
        );
        assert_eq!(
            detect_source_language(Path::new("test.cc")).unwrap(),
            SourceLanguage::Cpp
        );
        assert_eq!(
            detect_source_language(Path::new("test.cxx")).unwrap(),
            SourceLanguage::Cpp
        );
    }

    #[test]
    fn test_detect_source_language_header() {
        // .h files are not explicitly supported (could be C or C++ headers)
        assert!(detect_source_language(Path::new("test.h")).is_err());
        assert_eq!(
            detect_source_language(Path::new("test.hpp")).unwrap(),
            SourceLanguage::Cpp
        );
    }

    #[test]
    fn test_detect_source_language_unsupported() {
        assert!(detect_source_language(Path::new("test.rs")).is_err());
        assert!(detect_source_language(Path::new("test.py")).is_err());
    }

    #[test]
    fn test_clanguage_flag() {
        assert_eq!(SourceLanguage::C.clang_flag(), "-xc");
        assert_eq!(SourceLanguage::Cpp.clang_flag(), "-xc++");
        assert_eq!(SourceLanguage::ObjectiveC.clang_flag(), "-xobjective-c");
        assert_eq!(SourceLanguage::ObjectiveCpp.clang_flag(), "-xobjective-c++");
    }

    #[test]
    fn test_compile_simple_c_function() {
        // Skip this test if clang is not available
        if tool_detector::find_clang().is_err() {
            return;
        }

        // Create a simple C file
        let source = r#"
int foo(int x) {
    if (x > 0) {
        return x * 2;
    } else {
        return x + 1;
    }
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".c").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Compile to LLVM IR
        let output_path = compile_to_llvm_ir_temp(source_path);

        assert!(output_path.is_ok());

        // Verify output file exists and contains LLVM IR
        let output_path = output_path.unwrap();
        assert!(output_path.exists());

        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(
            content.contains("define i32 @foo") || content.contains("define dso_local i32 @foo")
        );
        // Verify we have a function definition (braces)
        assert!(content.contains("{") && content.contains("}"));

        // Clean up
        let _ = std::fs::remove_file(output_path);
    }

    #[test]
    fn test_compile_simple_cpp_function() {
        // Skip this test if clang is not available
        if tool_detector::find_clang().is_err() {
            return;
        }

        // Create a simple C++ file
        let source = r#"
extern "C" int bar(int x) {
    return x + 42;
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".cpp").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Compile to LLVM IR
        let output_path = compile_to_llvm_ir_temp(source_path);

        assert!(output_path.is_ok());

        // Verify output file exists
        let output_path = output_path.unwrap();
        assert!(output_path.exists());

        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(
            content.contains("define i32 @bar") || content.contains("define dso_local i32 @bar")
        );
        // Verify we have a function definition (braces)
        assert!(content.contains("{") && content.contains("}"));

        // Clean up
        let _ = std::fs::remove_file(output_path);
    }
}
