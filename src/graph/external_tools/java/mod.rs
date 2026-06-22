//! Java CFG extraction via javac → .class bytecode
//!
//! This module compiles Java source files to .class bytecode files
//! using javac, then parses the bytecode to extract CFG blocks.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

pub mod class_parser;
pub mod javac_invoker;

use javac_invoker::{compile_to_class_temp, JavacCompilationError};

/// Extract per-method CFGs from a Java source file.
///
/// Returns a map from unqualified method name → `CfgWithEdges`, matching
/// the same structure returned by the C/C++ LLVM extractor so ops.rs can
/// use an identical insertion loop.
///
/// # Errors
///
/// Returns error if javac is not available, compilation fails, or bytecode
/// parsing fails. The caller should log and skip, not propagate.
pub fn extract_cfgs_from_java(
    source_path: &Path,
) -> Result<HashMap<String, crate::graph::cfg_edges_extract::CfgWithEdges>> {
    // Step 1: Compile to .class bytecode
    let class_path =
        compile_to_class_temp(source_path).context("Failed to compile Java to bytecode")?;

    // Step 2: Read .class file bytes
    let class_bytes = std::fs::read(&class_path).context("Failed to read .class file")?;

    // Step 3: Parse per-method CFGs from bytecode
    let method_cfgs = class_parser::extract_cfg_from_class(&class_bytes)
        .context("Failed to parse CFG from bytecode")?;

    // Step 4: Clean up temporary .class file
    let _ = std::fs::remove_file(&class_path);

    Ok(method_cfgs)
}

/// Extract CFG from a Java source file (merged blob — legacy, kept for tests)
pub fn extract_cfg_from_java(
    source_path: &Path,
) -> Result<crate::graph::cfg_edges_extract::CfgWithEdges> {
    let method_cfgs = extract_cfgs_from_java(source_path)?;
    Ok(merge_method_cfgs(method_cfgs))
}

/// Extract CFG from a Java source file for a specific method
///
/// # Arguments
///
/// * `source_path` - Path to the Java source file
/// * `method_name` - Name of the method to extract
///
/// # Returns
///
/// CFG with blocks and edges for the specified method
///
/// # Errors
///
/// Returns error if:
/// - javac is not found
/// - compilation fails
/// - .class parsing fails
/// - method not found
pub fn extract_cfg_for_method(
    source_path: &Path,
    method_name: &str,
) -> Result<crate::graph::cfg_edges_extract::CfgWithEdges> {
    // Step 1: Compile to .class bytecode
    let class_path =
        compile_to_class_temp(source_path).context("Failed to compile Java to bytecode")?;

    // Step 2: Read .class file bytes
    let class_bytes = std::fs::read(&class_path).context("Failed to read .class file")?;

    // Step 3: Parse CFG for specific method
    let cfg = class_parser::extract_cfg_for_method(&class_bytes, method_name)
        .context("Failed to parse CFG from bytecode")?;

    // Step 4: Clean up temporary .class file
    let _ = std::fs::remove_file(&class_path);

    Ok(cfg)
}

/// Merge multiple method CFGs into a single CFG
///
/// This is a simplified merge that just concatenates blocks and edges.
/// A more sophisticated merge would handle method calls and returns.
fn merge_method_cfgs(
    method_cfgs: HashMap<String, crate::graph::cfg_edges_extract::CfgWithEdges>,
) -> crate::graph::cfg_edges_extract::CfgWithEdges {
    use crate::graph::cfg_edges_extract::CfgWithEdges;

    let mut all_blocks = Vec::new();
    let mut all_edges = Vec::new();
    let mut block_id_offset = 0i64;

    for (_method_name, mut cfg) in method_cfgs {
        // Offset block IDs in edges to avoid collisions
        for edge in &mut cfg.edges {
            edge.source_idx += block_id_offset as usize;
            edge.target_idx += block_id_offset as usize;
        }

        // Update offset for next method
        block_id_offset += cfg.blocks.len() as i64;

        // Merge blocks and edges
        all_blocks.extend(cfg.blocks);
        all_edges.extend(cfg.edges);
    }

    CfgWithEdges {
        blocks: all_blocks,
        edges: all_edges,
        function_id: 0,
    }
}

/// Check if javac is available
pub fn is_javac_available() -> bool {
    crate::graph::external_tools::tool_detector::is_tool_available("javac")
}

/// Get javac version information
pub fn get_javac_version() -> Option<String> {
    crate::graph::external_tools::tool_detector::check_javac_version().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_is_javac_available() {
        // This test just verifies the function doesn't panic
        let _ = is_javac_available();
    }

    #[test]
    fn test_extract_cfg_simple_java() {
        // Skip this test if javac is not available
        if !is_javac_available() {
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

    public static int bar(int x) {
        return x + 42;
    }
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Extract CFG
        let result = extract_cfg_from_java(source_path);

        if let Err(e) = &result {
            // Check if this is an expected error (e.g., javac not found)
            if e.to_string().contains("javac") {
                return; // Skip test if javac not available
            }
        }

        assert!(result.is_ok());

        let cfg = result.unwrap();
        assert!(!cfg.blocks.is_empty());
    }

    #[test]
    fn test_extract_cfg_for_method() {
        // Skip this test if javac is not available
        if !is_javac_available() {
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

    public static int bar(int x) {
        return x + 42;
    }
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Extract CFG for foo method
        let result = extract_cfg_for_method(source_path, "foo");

        if let Err(e) = &result {
            // Check if this is an expected error (e.g., javac not found)
            if e.to_string().contains("javac") {
                return; // Skip test if javac not available
            }
        }

        assert!(result.is_ok());

        let cfg = result.unwrap();
        assert!(!cfg.blocks.is_empty());
    }

    #[test]
    fn test_extract_cfg_method_not_found() {
        // Skip this test if javac is not available
        if !is_javac_available() {
            return;
        }

        // Create a simple Java file
        let source = r#"
class Test {
    public static int foo(int x) {
        return x;
    }
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Try to extract CFG for non-existent method
        let result = extract_cfg_for_method(source_path, "nonexistent");

        if let Err(e) = &result {
            // Check if this is an expected error (e.g., javac not found)
            if e.to_string().contains("javac") {
                return; // Skip test if javac not available
            }
        }

        assert!(result.is_err());
    }
}
