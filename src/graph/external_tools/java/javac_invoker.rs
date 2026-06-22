//! Java to bytecode compilation via javac
//!
//! Compiles Java source files to .class bytecode files using javac.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::graph::external_tools::{tool_detector, tool_invoker};

/// Errors from javac compilation
#[derive(Debug, thiserror::Error)]
pub enum JavacCompilationError {
    #[error("Javac not found: {0}")]
    JavacNotFound(String),

    #[error("Compilation failed: {0}")]
    CompilationFailed(String),

    #[error("Unsupported file type: {extension}")]
    UnsupportedFileType { extension: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Detect the Java source root by reading the package declaration and walking up the directory tree.
///
/// For `src/com/example/Main.java` with `package com.example;`, returns `src/`.
/// Returns `None` if no package declaration is found (default package) or if the
/// directory structure doesn't match the package depth.
fn detect_source_root(source_path: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(source_path).ok()?;
    let package = content
        .lines()
        .find(|l| l.trim_start().starts_with("package "))?
        .trim()
        .strip_prefix("package ")?
        .trim_end_matches(';')
        .trim()
        .to_string();

    if package.is_empty() {
        return None;
    }

    let depth = package.split('.').count();
    let mut root = source_path.parent()?;
    for _ in 0..depth {
        root = root.parent()?;
    }
    Some(root.to_path_buf())
}

/// Compile a Java source file to bytecode
///
/// # Arguments
///
/// * `source_path` - Path to the Java source file
/// * `output_dir` - Directory where .class files should be written
///
/// # Returns
///
/// Ok(()) if compilation succeeds
///
/// # Errors
///
/// Returns error if javac is not found or compilation fails
pub fn compile_to_class(
    source_path: &Path,
    output_dir: &Path,
) -> Result<(), JavacCompilationError> {
    // Find javac executable
    let javac_path = tool_detector::find_javac().map_err(|e| match e {
        tool_detector::ToolDetectionError::ToolNotFound { .. } => {
            JavacCompilationError::JavacNotFound("javac not found in PATH".to_string())
        }
        _ => JavacCompilationError::JavacNotFound(format!("javac detection failed: {}", e)),
    })?;

    // Verify file extension
    let extension = source_path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| JavacCompilationError::UnsupportedFileType {
            extension: "<none>".to_string(),
        })?;

    if extension.to_lowercase() != "java" {
        return Err(JavacCompilationError::UnsupportedFileType {
            extension: extension.to_string(),
        });
    }

    // Create output directory if it doesn't exist
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir)?;
    }

    // Detect source root from package declaration so javac can resolve cross-file imports.
    // e.g. com/example/Main.java with "package com.example;" → source root is two dirs up.
    let source_root = detect_source_root(source_path);

    // Build javac command
    let mut cmd = Command::new(&javac_path);
    cmd.arg("-d").arg(output_dir);
    if let Some(ref root) = source_root {
        // -sourcepath lets javac find and compile imported classes from source
        cmd.arg("-sourcepath").arg(root);
    }
    cmd.arg(source_path);
    let output = cmd.output().map_err(JavacCompilationError::Io)?;

    // Check if compilation succeeded
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(JavacCompilationError::CompilationFailed(stderr.to_string()));
    }

    Ok(())
}

/// Compile a Java source file to bytecode in a temporary directory
///
/// This is a convenience function that creates a temporary directory
/// and returns the path to the .class file. The caller is responsible
/// for cleaning up the temporary directory.
///
/// # Arguments
///
/// * `source_path` - Path to the Java source file
///
/// # Returns
///
/// Path to the compiled .class file
///
/// # Errors
///
/// Returns error if javac is not found or compilation fails
pub fn compile_to_class_temp(source_path: &Path) -> Result<PathBuf, JavacCompilationError> {
    let temp_dir =
        tempfile::tempdir().map_err(|e| JavacCompilationError::Io(std::io::Error::other(e)))?;

    compile_to_class(source_path, temp_dir.path())?;

    let class_files = find_class_files(temp_dir.path());

    if class_files.is_empty() {
        return Err(JavacCompilationError::CompilationFailed(
            "No .class files were generated".to_string(),
        ));
    }

    // When -sourcepath pulls in dependencies, multiple .class files are produced.
    // Select the one that corresponds to the source file we were asked to compile.
    let class_file = if class_files.len() == 1 {
        class_files[0].clone()
    } else {
        let class_name = extract_class_name(source_path).unwrap_or_default();
        let expected = format!("{}.class", class_name);
        class_files
            .iter()
            .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(expected.as_str()))
            .unwrap_or(&class_files[0])
            .clone()
    };

    // Persist the temp directory so the returned path remains valid.
    // The OS cleans up orphaned temp directories on reboot.
    let _ = temp_dir.keep();
    Ok(class_file)
}

/// Find all .class files in a directory recursively
fn find_class_files(dir: &Path) -> Vec<PathBuf> {
    let mut class_files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                class_files.extend(find_class_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("class") {
                class_files.push(path);
            }
        }
    }

    class_files
}

/// Extract class name from Java source file
///
/// This is a simple extraction that looks for `public class Name`
/// or `class Name` patterns. It doesn't handle all Java syntax.
pub fn extract_class_name(source_path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(source_path)?;

    // Look for class declaration
    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with("//") || line.starts_with("/*") {
            continue;
        }

        // Look for class declaration
        if line.contains("class ") {
            // Extract class name
            if let Some(class_pos) = line.find("class ") {
                let rest = &line[class_pos + 6..];
                let class_name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                    .collect();

                if !class_name.is_empty() {
                    return Ok(class_name);
                }
            }
        }
    }

    // Fallback: use file name without extension
    let file_stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            JavacCompilationError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid file name",
            ))
        })?;

    Ok(file_stem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_extract_class_name_simple() {
        let source = r#"
public class Foo {
    public static void main(String[] args) {
        System.out.println("Hello");
    }
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        let class_name = extract_class_name(source_path);
        assert!(class_name.is_ok());
        assert_eq!(class_name.unwrap(), "Foo");
    }

    #[test]
    fn test_extract_class_name_without_public() {
        let source = r#"
class Bar {
    public int value;
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        let class_name = extract_class_name(source_path);
        assert!(class_name.is_ok());
        assert_eq!(class_name.unwrap(), "Bar");
    }

    #[test]
    fn test_extract_class_name_fallback() {
        // Source with no class declaration - should use filename
        let source = "// Just a comment\n";

        let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();

        // Rename to have specific stem
        let test_path = temp_file.path().with_file_name("TestClass.java");
        std::fs::rename(temp_file.path(), &test_path).ok();

        let class_name = extract_class_name(&test_path);
        assert!(class_name.is_ok());
        assert_eq!(class_name.unwrap(), "TestClass");
    }

    #[test]
    fn test_compile_simple_java_class() {
        // Skip this test if javac is not available
        if tool_detector::find_javac().is_err() {
            return;
        }

        // Create a simple Java file
        let source = r#"
class Test {
    public static int foo(int x) {
        if (x > 0) {
            return x * 2;
        } else {
            return x + 1;
        }
    }
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Compile to bytecode
        let output_path = compile_to_class_temp(source_path);

        assert!(output_path.is_ok());

        // Verify .class file exists
        let class_path = output_path.unwrap();
        assert!(class_path.exists());
        assert_eq!(class_path.extension().unwrap(), "class");
    }

    #[test]
    fn test_compile_java_with_syntax_error() {
        // Skip this test if javac is not available
        if tool_detector::find_javac().is_err() {
            return;
        }

        // Create a Java file with syntax error
        let source = r#"
public class Broken {
    public static void main( {
        // Missing closing parenthesis
    }
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Compile should fail
        let result = compile_to_class_temp(source_path);
        assert!(result.is_err());
    }
}
