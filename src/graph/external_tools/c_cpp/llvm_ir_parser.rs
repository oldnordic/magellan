//! LLVM IR text format parser and CFG extraction
//!
//! Parses LLVM IR .ll files and extracts Control Flow Graph blocks.

use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::graph::cfg_edges_extract::{CfgEdge, CfgEdgeType};
use crate::graph::cfg_extractor::BlockKind;
use crate::graph::schema::CfgBlock;

/// Errors from LLVM IR parsing
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Failed to read LLVM IR file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid LLVM IR syntax: {0}")]
    InvalidSyntax(String),

    #[error("No functions found in LLVM IR")]
    NoFunctionsFound,

    #[error("Function not found: {0}")]
    FunctionNotFound(String),
}

/// CFG with edges extracted from LLVM IR
pub type CfgWithEdges = crate::graph::cfg_edges_extract::CfgWithEdges;

/// Basic block in LLVM IR
#[derive(Debug, Clone)]
struct LlvmBlock {
    /// Block name (label)
    name: String,
    /// Terminator instruction
    terminator: Terminator,
    /// Line number in source (for debugging)
    line: usize,
}

/// Terminator instruction in LLVM IR
#[derive(Debug, Clone, PartialEq)]
enum Terminator {
    /// Unconditional branch: `br label %dest`
    Unconditional { dest: String },
    /// Conditional branch: `br i1 %cond, label %true, label %false`
    Conditional {
        cond: String,
        true_dest: String,
        false_dest: String,
    },
    /// Return: `ret [type] [value]`
    Return,
    /// Switch: `switch [int_type] [value], label %default, [targets]`
    Switch {
        value: String,
        default_dest: String,
        cases: Vec<(String, String)>, // (value, dest)
    },
    /// Unreachable
    Unreachable,
    /// Unknown/unsupported terminator
    Unknown(String),
}

/// Parse LLVM IR file and extract CFG for all functions
pub fn extract_cfg_from_llvm_ir(ll_content: &str) -> Result<HashMap<String, CfgWithEdges>> {
    let mut result = HashMap::new();

    // Simple parsing - look for function definitions
    let lines: Vec<String> = ll_content.lines().map(|s| s.to_string()).collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        // Look for function definitions: `define ... @name(...) {`
        if line.starts_with("define") {
            if let Ok(func_name) = extract_function_name(line) {
                // Parse basic blocks until closing brace
                let mut blocks: Vec<LlvmBlock> = Vec::new();
                let mut brace_count = 1; // Already counted opening brace
                i += 1;

                while i < lines.len() && brace_count > 0 {
                    let func_line = lines[i].trim();
                    brace_count += func_line.matches('{').count() as i32;
                    brace_count -= func_line.matches('}').count() as i32;

                    // Look for labels: `label_name:` (may have comments after)
                    // Strip comments first
                    let line_without_comment = if let Some(pos) = func_line.find(';') {
                        &func_line[..pos]
                    } else {
                        func_line
                    }
                    .trim();

                    if line_without_comment.ends_with(':') {
                        let label_name = line_without_comment[..line_without_comment.len() - 1]
                            .trim()
                            .to_string();
                        // Accept both numeric and text labels
                        blocks.push(LlvmBlock {
                            name: label_name,
                            terminator: Terminator::Unknown("no terminator".to_string()),
                            line: i,
                        });
                    }

                    // Look for terminators in current block
                    if !blocks.is_empty() && brace_count == 1 {
                        if let Some(terminator) = parse_terminator(func_line) {
                            blocks
                                .last_mut()
                                .expect("invariant: blocks is not empty, checked above")
                                .terminator = terminator;
                        }
                    }

                    i += 1;
                }

                // Build CFG from blocks
                if blocks.is_empty() {
                    // Function has no control flow - create a single implicit block
                    // This happens for simple functions that just return a value
                    let implicit_block = CfgBlock {
                        cfg_hash: None,
                        statements: Some(vec!["implicit block (no control flow)".to_string()]),
                        function_id: 0,
                        kind: "Entry".to_string(),
                        terminator: "Return".to_string(),
                        byte_start: 0,
                        byte_end: 0,
                        start_line: 0,
                        start_col: 0,
                        end_line: 0,
                        end_col: 0,
                        cfg_condition: None,
                    };

                    result.insert(
                        func_name,
                        CfgWithEdges {
                            blocks: vec![implicit_block],
                            edges: vec![],
                            function_id: 0,
                        },
                    );
                } else {
                    let cfg = build_cfg_from_blocks(&func_name, &blocks)?;
                    result.insert(func_name, cfg);
                }
                continue;
            }
        }
        i += 1;
    }

    if result.is_empty() {
        return Err(ParseError::NoFunctionsFound.into());
    }

    Ok(result)
}

/// Parse LLVM IR file and extract CFG for a specific function
pub fn extract_cfg_for_function(ll_content: &str, function_name: &str) -> Result<CfgWithEdges> {
    let functions = extract_cfg_from_llvm_ir(ll_content)?;

    let cfg = functions
        .get(function_name)
        .ok_or_else(|| ParseError::FunctionNotFound(function_name.to_string()))?;

    Ok(cfg.clone())
}

/// Extract function name from LLVM IR line
fn extract_function_name(line: &str) -> Result<String> {
    if let Some(at_pos) = line.find('@') {
        let rest = &line[at_pos + 1..];
        if let Some(paren_pos) = rest.find('(') {
            Ok(rest[..paren_pos].to_string())
        } else {
            Err(ParseError::InvalidSyntax("No '(' in function definition".to_string()).into())
        }
    } else {
        Err(ParseError::InvalidSyntax("No '@' in function definition".to_string()).into())
    }
}

/// Parse a terminator instruction from LLVM IR
fn parse_terminator(line: &str) -> Option<Terminator> {
    let line = line.trim();

    // Unconditional branch: `br label %dest`
    if line.starts_with("br label") {
        if let Some(dest) = extract_label(line) {
            return Some(Terminator::Unconditional { dest });
        }
    }

    // Conditional branch: `br i1 %cond, label %true, label %false`
    if line.starts_with("br i1") {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 3 {
            let true_dest = extract_label(parts[1]).unwrap_or_else(|| "unknown".to_string());
            let false_dest = extract_label(parts[2]).unwrap_or_else(|| "unknown".to_string());
            return Some(Terminator::Conditional {
                cond: "%cond".to_string(),
                true_dest,
                false_dest,
            });
        }
    }

    // Return: `ret ...`
    if line.starts_with("ret") {
        return Some(Terminator::Return);
    }

    // Switch: `switch ...`
    if line.starts_with("switch") {
        if let Some(default_pos) = line.find("label %") {
            let rest = &line[default_pos + 7..];
            if let Some(end) = rest.find(',') {
                let default_dest = rest[..end].trim().to_string();
                return Some(Terminator::Switch {
                    value: "%value".to_string(),
                    default_dest,
                    cases: vec![],
                });
            }
        }
    }

    // Unreachable
    if line.starts_with("unreachable") {
        return Some(Terminator::Unreachable);
    }

    None
}

/// Extract a label from LLVM IR line
fn extract_label(line: &str) -> Option<String> {
    if let Some(pos) = line.find('%') {
        let rest = &line[pos + 1..];
        let label: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        if !label.is_empty() {
            Some(label)
        } else {
            None
        }
    } else {
        None
    }
}

/// Build CFG from LLVM basic blocks
fn build_cfg_from_blocks(_func_name: &str, blocks: &[LlvmBlock]) -> Result<CfgWithEdges> {
    if blocks.is_empty() {
        return Ok(CfgWithEdges {
            blocks: vec![],
            edges: vec![],
            function_id: 0,
        });
    }

    // Map block names to indices
    let mut block_map: HashMap<String, usize> = HashMap::new();
    for (idx, block) in blocks.iter().enumerate() {
        block_map.insert(block.name.clone(), idx);
    }

    // Create CFG blocks using correct schema
    let mut cfg_blocks: Vec<CfgBlock> = Vec::new();
    for (idx, block) in blocks.iter().enumerate() {
        let kind = if idx == 0 {
            BlockKind::Entry
        } else if block.terminator == Terminator::Return {
            BlockKind::Return
        } else {
            BlockKind::For // Generic block
        };

        cfg_blocks.push(CfgBlock {
            cfg_hash: None,
            statements: None,
            function_id: 0,
            kind: format!("{:?}", kind),
            terminator: format!("{:?}", block.terminator),
            byte_start: 0,
            byte_end: 0,
            start_line: block.line as u64,
            start_col: 0,
            end_line: block.line as u64,
            end_col: 0,
            cfg_condition: None,
        });
    }

    // Create CFG edges from terminators
    let mut cfg_edges: Vec<CfgEdge> = Vec::new();

    for (idx, block) in blocks.iter().enumerate() {
        match &block.terminator {
            Terminator::Unconditional { dest } => {
                if let Some(&dest_idx) = block_map.get(dest) {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx: dest_idx,
                        edge_type: CfgEdgeType::Jump,
                    });
                }
            }

            Terminator::Conditional {
                true_dest,
                false_dest,
                ..
            } => {
                if let Some(&dest_idx) = block_map.get(true_dest) {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx: dest_idx,
                        edge_type: CfgEdgeType::ConditionalTrue,
                    });
                }

                if let Some(&dest_idx) = block_map.get(false_dest) {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx: dest_idx,
                        edge_type: CfgEdgeType::ConditionalFalse,
                    });
                }
            }

            Terminator::Switch {
                default_dest,
                cases,
                ..
            } => {
                if let Some(&dest_idx) = block_map.get(default_dest) {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx: dest_idx,
                        edge_type: CfgEdgeType::Jump,
                    });
                }

                for (_value, dest) in cases {
                    if let Some(&dest_idx) = block_map.get(dest) {
                        cfg_edges.push(CfgEdge {
                            source_idx: idx,
                            target_idx: dest_idx,
                            edge_type: CfgEdgeType::Jump,
                        });
                    }
                }
            }

            Terminator::Return | Terminator::Unreachable => {
                // No outgoing edges
            }

            Terminator::Unknown(_) => {
                // Unknown terminator - skip edge creation
            }
        }
    }

    Ok(CfgWithEdges {
        blocks: cfg_blocks,
        edges: cfg_edges,
        function_id: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_label() {
        assert_eq!(extract_label("br label %entry").unwrap(), "entry");
        assert_eq!(extract_label("br label %if.then").unwrap(), "if.then");
    }

    #[test]
    fn test_parse_terminator_unconditional() {
        let term = parse_terminator("br label %entry");
        assert_eq!(
            term.unwrap(),
            Terminator::Unconditional {
                dest: "entry".to_string()
            }
        );
    }

    #[test]
    fn test_parse_terminator_return() {
        let term = parse_terminator("ret i32 %0");
        assert_eq!(term.unwrap(), Terminator::Return);
    }

    #[test]
    fn test_parse_terminator_unreachable() {
        let term = parse_terminator("unreachable");
        assert_eq!(term.unwrap(), Terminator::Unreachable);
    }

    #[test]
    fn test_extract_cfg_simple_function() {
        let ll_ir = r#"
define i32 @foo(i32 %x) {
entry:
  %cmp = icmp sgt i32 %x, 0
  br i1 %cmp, label %if.then, label %if.else

if.then:
  %mul = mul nsw i32 %x, 2
  ret i32 %mul

if.else:
  %add = add nsw i32 %x, 1
  ret i32 %add
}
"#;

        let result = extract_cfg_for_function(ll_ir, "foo");
        assert!(result.is_ok());

        let cfg = result.unwrap();
        assert_eq!(cfg.blocks.len(), 3); // entry, if.then, if.else

        // Check edges from entry (source_idx == 0)
        let entry_edges: Vec<_> = cfg.edges.iter().filter(|e| e.source_idx == 0).collect();

        assert_eq!(entry_edges.len(), 2); // Two branches
    }

    #[test]
    fn test_extract_cfg_no_functions() {
        let ll_ir = "This is not valid LLVM IR";

        let result = extract_cfg_from_llvm_ir(ll_ir);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_cfg_function_not_found() {
        let ll_ir = r#"
define i32 @bar(i32 %x) {
entry:
  ret i32 %x
}
"#;

        let result = extract_cfg_for_function(ll_ir, "foo");
        assert!(result.is_err());
    }
}
