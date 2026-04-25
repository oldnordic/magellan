//! C/C++ CFG extraction via clang → LLVM IR
//!
//! This module compiles C/C++ source files to LLVM IR text files (.ll)
//! using clang, then parses the LLVM IR to extract CFG blocks.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

pub mod clang_invoker;
pub mod llvm_ir_parser;

use clang_invoker::{compile_to_llvm_ir_temp, ClangCompilationError};

/// Extract CFG from a C/C++ source file
///
/// This is the main entry point for C/C++ CFG extraction.
///
/// # Arguments
///
/// * `source_path` - Path to the C/C++ source file
///
/// # Returns
///
/// CFG with blocks and edges for all functions in the source file
///
/// # Errors
///
/// Returns error if:
/// - clang is not found
/// - compilation fails
/// - LLVM IR parsing fails
pub fn extract_cfg_from_cpp(
    source_path: &Path,
) -> Result<crate::graph::cfg_edges_extract::CfgWithEdges> {
    // Step 1: Compile to LLVM IR
    let ll_path =
        compile_to_llvm_ir_temp(source_path).context("Failed to compile C/C++ to LLVM IR")?;

    // Step 2: Read LLVM IR content
    let ll_content = std::fs::read_to_string(&ll_path).context("Failed to read LLVM IR file")?;

    // Step 3: Parse CFG from LLVM IR
    let function_cfgs = llvm_ir_parser::extract_cfg_from_llvm_ir(&ll_content)
        .context("Failed to parse CFG from LLVM IR")?;

    // Step 4: Merge all function CFGs into one
    let merged_cfg = merge_function_cfgs(function_cfgs);

    // Step 5: Clean up temporary .ll file
    let _ = std::fs::remove_file(&ll_path);

    Ok(merged_cfg)
}

/// Extract CFG from a C/C++ source file for a specific function
///
/// # Arguments
///
/// * `source_path` - Path to the C/C++ source file
/// * `function_name` - Name of the function to extract
///
/// # Returns
///
/// CFG with blocks and edges for the specified function
///
/// # Errors
///
/// Returns error if:
/// - clang is not found
/// - compilation fails
/// - LLVM IR parsing fails
/// - function not found
pub fn extract_cfg_for_function(
    source_path: &Path,
    function_name: &str,
) -> Result<crate::graph::cfg_edges_extract::CfgWithEdges> {
    // Step 1: Compile to LLVM IR
    let ll_path =
        compile_to_llvm_ir_temp(source_path).context("Failed to compile C/C++ to LLVM IR")?;

    // Step 2: Read LLVM IR content
    let ll_content = std::fs::read_to_string(&ll_path).context("Failed to read LLVM IR file")?;

    // Step 3: Parse CFG for specific function
    let cfg = llvm_ir_parser::extract_cfg_for_function(&ll_content, function_name)
        .context("Failed to parse CFG from LLVM IR")?;

    // Step 4: Clean up temporary .ll file
    let _ = std::fs::remove_file(&ll_path);

    Ok(cfg)
}

/// Merge multiple function CFGs into a single CFG
///
/// This is a simplified merge that just concatenates blocks and edges.
/// A more sophisticated merge would handle function calls and returns.
fn merge_function_cfgs(
    function_cfgs: HashMap<String, crate::graph::cfg_edges_extract::CfgWithEdges>,
) -> crate::graph::cfg_edges_extract::CfgWithEdges {
    use crate::graph::cfg_edges_extract::CfgWithEdges;

    let mut all_blocks = Vec::new();
    let mut all_edges = Vec::new();
    let mut block_id_offset = 0i64;

    for (_func_name, mut cfg) in function_cfgs {
        // Offset block IDs in edges to avoid collisions
        for edge in &mut cfg.edges {
            edge.source_idx += block_id_offset as usize;
            edge.target_idx += block_id_offset as usize;
        }

        // Update offset for next function
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

/// Check if clang is available
pub fn is_clang_available() -> bool {
    crate::graph::external_tools::tool_detector::is_tool_available("clang")
}

/// Get clang version information
pub fn get_clang_version() -> Option<String> {
    match crate::graph::external_tools::tool_detector::check_clang_version() {
        Ok(version) => Some(version),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_is_clang_available() {
        // This test just verifies the function doesn't panic
        let _ = is_clang_available();
    }

    #[test]
    fn test_extract_cfg_simple_c() {
        // Skip this test if clang is not available
        if !is_clang_available() {
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

int bar(int x) {
    return x + 42;
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".c").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Extract CFG
        let result = extract_cfg_from_cpp(source_path);

        if let Err(e) = &result {
            // Check if this is an expected error (e.g., clang not found)
            if e.to_string().contains("clang") {
                return; // Skip test if clang not available
            }
        }

        assert!(result.is_ok());

        let cfg = result.unwrap();
        assert!(!cfg.blocks.is_empty());
        assert!(!cfg.edges.is_empty());
    }

    #[test]
    fn test_extract_cfg_for_function() {
        // Skip this test if clang is not available
        if !is_clang_available() {
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

int bar(int x) {
    return x + 42;
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".c").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Extract CFG for foo function
        let result = extract_cfg_for_function(source_path, "foo");

        if let Err(e) = &result {
            // Check if this is an expected error (e.g., clang not found)
            if e.to_string().contains("clang") {
                return; // Skip test if clang not available
            }
        }

        assert!(result.is_ok());

        let cfg = result.unwrap();
        assert!(!cfg.blocks.is_empty());
        assert!(!cfg.edges.is_empty());
    }

    #[test]
    fn test_extract_cfg_function_not_found() {
        // Skip this test if clang is not available
        if !is_clang_available() {
            return;
        }

        // Create a simple C file
        let source = r#"
int foo(int x) {
    return x;
}
"#;

        let mut temp_file = NamedTempFile::with_suffix(".c").unwrap();
        temp_file.write_all(source.as_bytes()).unwrap();
        let source_path = temp_file.path();

        // Try to extract CFG for non-existent function
        let result = extract_cfg_for_function(source_path, "nonexistent");

        if let Err(e) = &result {
            // Check if this is an expected error (e.g., clang not found)
            if e.to_string().contains("clang") {
                return; // Skip test if clang not available
            }
        }

        assert!(result.is_err());
    }
}
