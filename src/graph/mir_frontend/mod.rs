//! MIR frontend for Rust CFG extraction using rustc CLI
//!
//! This module provides an alternative to AST-based CFG extraction by using
//! the Mid-level Intermediate Representation (MIR) from the Rust compiler.
//! MIR provides more accurate CFG information than AST-based reconstruction.
//!
//! # Feature Flag
//!
//! This module is only available when the `mir-frontend` feature is enabled:
//! ```bash
//! cargo build --features mir-frontend
//! ```
//!
//! # Usage
//!
//! ```rust
//! use magellan::graph::mir_frontend::extract_cfg_from_rust_source;
//!
//! let cfg = extract_cfg_from_rust_source(
//!     "function_name",
//!     source_code,
//! ).expect("MIR extraction failed");
//! ```
//!
//! # Advantages over AST
//!
//! - **Accurate basic blocks**: Real compiler blocks, not AST reconstruction
//! - **Desugared control flow**: Loops, async, match desugared to simple primitives
//! - **Type-checked**: Validated by compiler before extraction
//! - **Accurate terminators**: Real goto, switch, call, return edges
//!
//! # Limitations
//!
//! - Requires stable Rust compiler (no nightly needed)
//! - Slower than AST (requires full compilation)
//! - Only works for Rust code
//! - Uses rustc as subprocess (no library linking)

use crate::graph::schema::CfgBlock;
use crate::graph::cfg_edges_extract::{CfgEdge, CfgEdgeType};
use anyhow::{Result, Context};
use std::process::Command;
use std::path::Path;
use std::fs;
use std::io::BufRead;

/// MIR extraction result with CFG blocks and edges
#[derive(Debug, Clone)]
pub struct MirCfgResult {
    /// Function name
    pub function_name: String,
    /// CFG basic blocks
    pub blocks: Vec<CfgBlock>,
    /// CFG edges between blocks
    pub edges: Vec<CfgEdge>,
}

/// Extract CFG from Rust source code using MIR via rustc CLI
///
/// This function compiles the given Rust source code using rustc
/// with `--emit=mir` flag to dump MIR, then parses the MIR output.
///
/// # Arguments
///
/// * `function_name` - Fully qualified name of the function to extract
/// * `source_code` - Rust source code
///
/// # Returns
///
/// Result containing MirCfgResult with CFG blocks and edges
///
/// # Errors
///
/// Returns error if:
/// - Compilation fails
/// - Function not found
/// - MIR output parsing fails
pub fn extract_cfg_from_rust_source(
    function_name: &str,
    source_code: &str,
) -> Result<MirCfgResult> {
    // Create temporary directory for compilation
    let temp_dir = tempfile::tempdir()
        .context("Failed to create temporary directory for MIR compilation")?;

    let temp_file = temp_dir.path().join("temp_mir_test.rs");

    // Write source code to temporary file
    fs::write(&temp_file, source_code)
        .context("Failed to write source code to temporary file")?;

    // Invoke rustc with --emit=mir to dump MIR
    let mir_file = temp_dir.path().join("temp_mir_test.mir");

    let output = Command::new("rustc")
        .arg(&temp_file)
        .arg("--emit=mir")
        .arg("-o")
        .arg(&mir_file)
        .current_dir(temp_dir.path())
        .output()
        .context("Failed to invoke rustc for MIR extraction")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Rustc compilation failed: {}",
            stderr.trim()
        );
    }

    // Debug: print MIR file content if tests fail
    #[cfg(test)]
    {
        eprintln!("=== MIR file content for debugging ===");
        if let Ok(mir_content) = fs::read_to_string(&mir_file) {
            eprintln!("{}", mir_content);
        } else {
            eprintln!("Failed to read MIR file");
        }
        eprintln!("=== End MIR file content ===");
    }

    // Parse MIR output
    parse_mir_file(&mir_file, function_name)
}

/// Parse MIR file and extract CFG
fn parse_mir_file(mir_path: &Path, function_name: &str) -> Result<MirCfgResult> {
    let file = fs::File::open(mir_path)
        .context("Failed to open MIR file")?;
    let reader = std::io::BufReader::new(file);

    let mut in_target_function = false;

    // First pass: collect all lines and build block map
    let mut lines = Vec::new();
    let mut block_map = std::collections::HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        lines.push(line.to_string());
    }

    // Second pass: parse blocks and build block map
    for line in &lines {
        // Detect function start: "fn simple_function(_1: i32) -> i32 {"
        if line.starts_with("fn ") {
            // Extract function name: "fn simple_function(" -> "simple_function"
            let func_start = line.find("fn ").unwrap() + 3;
            let func_end = line.find('(').unwrap_or(line.len());
            let func_name = &line[func_start..func_end];

            // Skip main(), only look for target function
            in_target_function = func_name.trim() == function_name;
            if !in_target_function {
                continue;
            }

            block_map.clear();
        }

        if !in_target_function {
            continue;
        }

        // Parse basic blocks: "bb0: {"
        if line.starts_with("bb") && line.contains(": {") {
            // Extract block number: "bb0: {" -> 0
            let block_num_str = &line[2..].split(':').next().unwrap_or("0");
            let block_num: usize = block_num_str.parse().unwrap_or(0);

            if !block_map.contains_key(&block_num) {
                let block_idx = block_map.len();
                block_map.insert(block_num, block_idx);
            }
        }
    }

    // Third pass: create blocks and edges
    let mut blocks = Vec::new();
    let mut edges = Vec::new();
    let mut current_block: Option<usize> = None;
    let mut in_target_function = false;

    for line in &lines {
        // Detect function start
        if line.starts_with("fn ") {
            let func_start = line.find("fn ").unwrap() + 3;
            let func_end = line.find('(').unwrap_or(line.len());
            let func_name = &line[func_start..func_end];

            in_target_function = func_name.trim() == function_name;
            if !in_target_function {
                continue;
            }
        }

        // Exit function when we see the next function declaration (different from current one)
        if line.starts_with("fn ") && in_target_function {
            // Check if this is a different function
            let func_start = line.find("fn ").unwrap() + 3;
            let func_end = line.find('(').unwrap_or(line.len());
            let this_func = &line[func_start..func_end];

            if this_func.trim() != function_name {
                in_target_function = false;
                continue;
            }
        }

        // Exit function when we see the next function declaration (different from current one)
        if line.starts_with("fn ") && in_target_function {
            // Check if this is a different function
            let func_start = line.find("fn ").unwrap() + 3;
            let func_end = line.find('(').unwrap_or(line.len());
            let this_func = &line[func_start..func_end];

            if this_func.trim() != function_name {
                #[cfg(test)]
                eprintln!("Found next function '{}' while processing '{}', exiting", this_func.trim(), function_name);
                in_target_function = false;
                continue;
            }
        }

        // Mark block end
        if *line == "}" && current_block.is_some() {
            current_block = None;
            continue;
        }

        if !in_target_function {
            continue;
        }

        // Parse basic blocks
        if line.starts_with("bb") && line.contains(": {") {
            let block_num_str = &line[2..].split(':').next().unwrap_or("0");
            let block_num: usize = block_num_str.parse().unwrap_or(0);

            if let Some(&block_idx) = block_map.get(&block_num) {
                // Ensure block exists
                while blocks.len() <= block_idx {
                    blocks.push(CfgBlock {
                        function_id: 0,
                        kind: "basic_block".to_string(),
                        terminator: "unknown".to_string(),
                        byte_start: 0,
                        byte_end: 0,
                        start_line: 0,
                        start_col: 0,
                        end_line: 0,
                        end_col: 0,
                        cfg_hash: None,
                        statements: None,
                        cfg_condition: None,
                    });
                }
                current_block = Some(block_idx);
            }
        }

        let source_block = match current_block {
            Some(idx) => idx,
            None => continue,
        };

        // Parse goto: "goto -> bb5;"
        if line.contains("goto ->") {
            if let Some(target) = extract_single_goto_target(line) {
                if let Some(&target_idx) = block_map.get(&target) {
                    edges.push(CfgEdge {
                        source_idx: source_block,
                        target_idx,
                        edge_type: CfgEdgeType::Jump,
                    });
                }
            }
        }

        // Parse switchInt: "switchInt(move _2) -> [0: bb3, otherwise: bb1];"
        if line.contains("switchInt") {
            let targets = extract_switch_targets(line);
            for (i, target) in targets.iter().enumerate() {
                if let Some(&target_idx) = block_map.get(target) {
                    let edge_type = if i == 0 {
                        CfgEdgeType::ConditionalFalse
                    } else {
                        CfgEdgeType::ConditionalTrue
                    };
                    edges.push(CfgEdge {
                        source_idx: source_block,
                        target_idx,
                        edge_type,
                    });
                }
            }
        }

        // Parse return: "return;"
        if *line == "return;" {
            edges.push(CfgEdge {
                source_idx: source_block,
                target_idx: usize::MAX,
                edge_type: CfgEdgeType::Return,
            });
        }

        // Parse call with return: "simple_function(move _2) -> [return: bb1, unwind continue];"
        if line.contains("-> [return:") {
            let targets = extract_call_targets(line);
            for target in targets {
                if let Some(&target_idx) = block_map.get(&target) {
                    edges.push(CfgEdge {
                        source_idx: source_block,
                        target_idx,
                        edge_type: CfgEdgeType::Call,
                    });
                }
            }
        }
    }

    Ok(MirCfgResult {
        function_name: function_name.to_string(),
        blocks,
        edges,
    })
}

/// Extract single goto target from line like "goto -> bb5;"
fn extract_single_goto_target(line: &str) -> Option<usize> {
    let line = line.trim();
    if !line.contains("goto ->") {
        return None;
    }

    // Find "bbN" pattern after "->"
    let arrow_pos = line.find("->")?;
    let after_arrow = &line[arrow_pos + 2..];

    // Pattern: bb5; or bb5 followed by space/semicolon
    let bb_match = after_arrow.trim_start().strip_prefix("bb")?;
    let num_str: String = bb_match.chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num_str.parse().ok()
}

/// Extract switch targets from line like "switchInt(move _2) -> [0: bb3, otherwise: bb1];"
fn extract_switch_targets(line: &str) -> Vec<usize> {
    let mut targets = Vec::new();

    // Find all "bbN" patterns in brackets
    if let Some(bracket_start) = line.find('[') {
        let bracket_content = &line[bracket_start + 1..];
        if let Some(bracket_end) = bracket_content.find(']') {
            let content = &bracket_content[..bracket_end];

            // Split by comma and extract each bbN
            for part in content.split(',') {
                let part = part.trim();
                if let Some(bb_start) = part.find("bb") {
                    let after_bb = &part[bb_start + 2..];
                    let num_str: String = after_bb.chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if let Ok(num) = num_str.parse::<usize>() {
                        targets.push(num);
                    }
                }
            }
        }
    }

    targets
}

/// Extract call return targets from line like "simple_function(move _2) -> [return: bb1, unwind continue];"
fn extract_call_targets(line: &str) -> Vec<usize> {
    let mut targets = Vec::new();

    // Find "return: bbN" pattern
    if let Some(return_pos) = line.find("return:") {
        let after_return = &line[return_pos + 7..];
        if let Some(bb_start) = after_return.find("bb") {
            let after_bb = &after_return[bb_start + 2..];
            let num_str: String = after_bb.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(num) = num_str.parse::<usize>() {
                targets.push(num);
            }
        }
    }

    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_function_extraction() {
        let source = r#"
fn main() {
    let x = 5;
    simple_function(x);
}

pub fn simple_function(x: i32) -> i32 {
    if x > 0 {
        x + 1
    } else {
        x - 1
    }
}
"#;

        let result = extract_cfg_from_rust_source(
            "simple_function",
            source,
        );

        if let Err(ref e) = result {
            eprintln!("MIR extraction failed: {:?}", e);
        }

        assert!(result.is_ok(), "MIR extraction should succeed");
        let cfg = result.unwrap();
        assert_eq!(cfg.function_name, "simple_function");

        // Should have blocks for if/else (6 blocks in MIR: bb0-bb5)
        assert!(!cfg.blocks.is_empty(), "Should extract basic blocks");

        // Should have edges for control flow
        assert!(!cfg.edges.is_empty(), "Should extract control flow edges");
    }

    #[test]
    fn test_loop_function_extraction() {
        let source = r#"
fn main() {
    loop_function(10);
}

pub fn loop_function(n: i32) -> i32 {
    let mut sum = 0;
    let mut i = 0;
    while i < n {
        sum += i;
        i += 1;
    }
    sum
}
"#;

        let result = extract_cfg_from_rust_source(
            "loop_function",
            source,
        );

        if let Err(ref e) = result {
            eprintln!("MIR extraction failed: {:?}", e);
        }

        assert!(result.is_ok(), "MIR extraction should succeed");
        let cfg = result.unwrap();
        assert_eq!(cfg.function_name, "loop_function");

        eprintln!("Loop function extracted {} blocks and {} edges", cfg.blocks.len(), cfg.edges.len());

        // Should have blocks for while loop
        assert!(!cfg.blocks.is_empty(), "Should extract basic blocks");

        // Should have edges including back edges for the loop
        assert!(!cfg.edges.is_empty(), "Should extract control flow edges");
    }
}
