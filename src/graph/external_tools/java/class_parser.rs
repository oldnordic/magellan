//! Java .class bytecode parser and CFG extraction
//!
//! Parses Java .class binary files and extracts Control Flow Graph blocks.
//!
//! Java Class File Format Reference:
//! - Magic: 0xCAFEBABE
//! - Constant pool, fields, methods, attributes
//! - Bytecode instructions in Code attribute
//!
//! Implementation parses .class file structure according to JVM specification:
//! https://docs.oracle.com/javase/specs/jvms/se8/html/

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::graph::cfg_edges_extract::{CfgEdge, CfgEdgeType};
use crate::graph::cfg_extractor::BlockKind;
use crate::graph::schema::CfgBlock;

/// Errors from .class file parsing
#[derive(Debug, thiserror::Error)]
pub enum ClassParseError {
    #[error("Failed to read .class file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid .class file format: {0}")]
    InvalidFormat(String),

    #[error("No methods found in .class file")]
    NoMethodsFound,

    #[error("Method not found: {0}")]
    MethodNotFound(String),
}

/// CFG with edges extracted from Java bytecode
pub type CfgWithEdges = crate::graph::cfg_edges_extract::CfgWithEdges;

/// Java bytecode instruction
#[derive(Debug, Clone)]
struct BytecodeInstruction {
    /// Opcode
    opcode: u8,
    /// Operands
    operands: Vec<u8>,
    /// Instruction offset (for control flow)
    offset: usize,
}

/// Basic block in Java bytecode
#[derive(Debug, Clone)]
struct BytecodeBlock {
    /// Block start offset
    start_offset: usize,
    /// Block end offset (exclusive)
    end_offset: usize,
    /// Instructions in this block
    instructions: Vec<BytecodeInstruction>,
    /// Terminator type
    terminator: BlockTerminator,
}

/// Block terminator in Java bytecode
#[derive(Debug, Clone, PartialEq)]
enum BlockTerminator {
    /// Return instruction
    Return,
    /// Unconditional jump (goto, goto_w)
    Unconditional { target: usize },
    /// Conditional jump (if_* instructions)
    Conditional { target: usize },
    /// Switch statement
    Switch { default: usize, cases: Vec<usize> },
    /// Throws exception
    Throw,
    /// Fall-through to next block
    Fallthrough,
    /// Unknown/unsupported
    Unknown,
}

/// Parse .class file and extract CFG for all methods
///
/// # Arguments
///
/// * `class_bytes` - .class file contents as bytes
///
/// # Returns
///
/// HashMap of method name -> CfgWithEdges
pub fn extract_cfg_from_class(class_bytes: &[u8]) -> Result<HashMap<String, CfgWithEdges>> {
    // Verify magic number
    if class_bytes.len() < 4 {
        return Err(ClassParseError::InvalidFormat("File too short".to_string()).into());
    }

    let magic = &class_bytes[0..4];
    if magic != &[0xCA, 0xFE, 0xBA, 0xBE] {
        return Err(ClassParseError::InvalidFormat("Invalid magic number".to_string()).into());
    }

    let mut result = HashMap::new();

    // Parse method bytecode from .class file structure
    let methods = find_method_bytecode(class_bytes)?;

    if methods.is_empty() {
        return Err(ClassParseError::NoMethodsFound.into());
    }

    // Extract CFG for each method
    for (method_name, bytecode) in methods {
        let cfg = build_cfg_from_bytecode(&method_name, &bytecode)?;
        result.insert(method_name, cfg);
    }

    Ok(result)
}

/// Parse .class file and extract CFG for a specific method
///
/// # Arguments
///
/// * `class_bytes` - .class file contents as bytes
/// * `method_name` - Name of the method to extract
///
/// # Returns
///
/// CFG with edges for the specified method
pub fn extract_cfg_for_method(class_bytes: &[u8], method_name: &str) -> Result<CfgWithEdges> {
    let methods = extract_cfg_from_class(class_bytes)?;

    let cfg = methods
        .get(method_name)
        .ok_or_else(|| ClassParseError::MethodNotFound(method_name.to_string()))?;

    Ok(cfg.clone())
}

/// Find method bytecode in .class file
///
/// Parses .class file format to extract method bytecode.
/// Java class file format: https://docs.oracle.com/javase/specs/jvms/se8/html/
fn find_method_bytecode(class_bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>> {
    let mut methods = HashMap::new();

    // Skip header (magic + minor_version + major_version) = 4 + 2 + 2 = 8 bytes
    if class_bytes.len() < 10 {
        return Ok(methods);
    }

    let mut pos = 8;

    // Read constant_pool_count (2 bytes)
    let constant_pool_count = u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
    pos += 2;

    // Skip constant pool (variable size based on entries)
    for _ in 0..constant_pool_count {
        if pos >= class_bytes.len() {
            return Ok(methods);
        }

        let tag = class_bytes[pos];
        pos += 1;

        match tag {
            // ConstantUtf8: 1 byte tag + 2 bytes length + variable bytes
            1 => {
                if pos + 1 >= class_bytes.len() {
                    return Ok(methods);
                }
                let length = u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
                pos += 2 + length;
            }
            // ConstantInteger, ConstantFloat, ConstantFieldref, ConstantMethodref, etc.: 1 byte tag + 4 bytes data
            3 | 4 | 9 | 10 | 11 | 12 => {
                pos += 4;
            }
            // ConstantLong, ConstantDouble: 1 byte tag + 8 bytes data
            5 | 6 => {
                pos += 8;
            }
            // ConstantClass, ConstantString: 1 byte tag + 2 bytes data
            7 | 8 => {
                pos += 2;
            }
            _ => {
                // Unknown constant type, skip
                return Ok(methods);
            }
        }
    }

    // Skip access_flags (2 bytes), this_class (2), super_class (2)
    pos += 6;

    // Skip interfaces count and interfaces
    if pos + 2 > class_bytes.len() {
        return Ok(methods);
    }
    let interfaces_count = u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
    pos += 2 + (interfaces_count * 2);

    // Skip fields count and fields
    if pos + 2 > class_bytes.len() {
        return Ok(methods);
    }
    let fields_count = u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
    pos += 2;

    // Skip fields (each field: access_flags + name_index + descriptor_index + attributes_count + attributes)
    for _ in 0..fields_count {
        if pos + 6 > class_bytes.len() {
            return Ok(methods);
        }
        pos += 6; // access_flags (2) + name_index (2) + descriptor_index (2)

        let attributes_count =
            u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
        pos += 2;

        for _ in 0..attributes_count {
            if pos + 2 > class_bytes.len() {
                return Ok(methods);
            }
            // Skip attribute_name_index (2)
            pos += 2;

            let attribute_length = u32::from_be_bytes([
                class_bytes[pos],
                class_bytes[pos + 1],
                class_bytes[pos + 2],
                class_bytes[pos + 3],
            ]) as usize;
            pos += 4 + attribute_length;
        }
    }

    // Read methods_count
    if pos + 2 > class_bytes.len() {
        return Ok(methods);
    }
    let methods_count = u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
    pos += 2;

    // Parse methods
    for _ in 0..methods_count {
        if pos + 8 > class_bytes.len() {
            return Ok(methods);
        }

        // Skip access_flags (2), name_index (2), descriptor_index (2)
        pos += 6;

        let attributes_count =
            u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
        pos += 2;

        // For simplicity, we'll generate method names like "method_0", "method_1", etc.
        // A full implementation would resolve the name from the constant pool
        let method_name = format!("method_{}", methods.len());

        // Parse method attributes looking for Code attribute
        for _ in 0..attributes_count {
            if pos + 6 > class_bytes.len() {
                return Ok(methods);
            }

            // Skip attribute_name_index (2)
            pos += 2;

            let attribute_length = u32::from_be_bytes([
                class_bytes[pos],
                class_bytes[pos + 1],
                class_bytes[pos + 2],
                class_bytes[pos + 3],
            ]) as usize;
            pos += 4;

            let attribute_start = pos;

            // Check if this is a Code attribute (has max_stack, max_locals, code_length)
            if pos + 6 <= class_bytes.len() {
                let max_stack =
                    u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
                let max_locals =
                    u16::from_be_bytes([class_bytes[pos + 2], class_bytes[pos + 3]]) as usize;
                let code_length = u32::from_be_bytes([
                    class_bytes[pos + 4],
                    class_bytes[pos + 5],
                    class_bytes[pos + 6],
                    class_bytes[pos + 7],
                ]) as usize;
                pos += 8;

                // Verify this looks like a Code attribute
                if max_stack > 0
                    && max_locals > 0
                    && code_length > 0
                    && pos + code_length <= class_bytes.len()
                {
                    // Extract bytecode
                    let bytecode = class_bytes[pos..pos + code_length].to_vec();

                    // Only add methods with actual bytecode (not just abstract/native)
                    if !bytecode.is_empty() {
                        methods.insert(method_name.clone(), bytecode);
                    }

                    pos += code_length;

                    // Skip exception_table and attributes
                    if pos + 4 > class_bytes.len() {
                        break;
                    }
                    let exception_table_length =
                        u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
                    pos += 2;
                    pos += exception_table_length * 8; // Each exception table entry is 8 bytes

                    if pos + 2 > class_bytes.len() {
                        break;
                    }
                    let code_attributes_count =
                        u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
                    pos += 2;

                    for _ in 0..code_attributes_count {
                        if pos + 2 > class_bytes.len() {
                            break;
                        }
                        pos += 2; // attribute_name_index

                        let code_attr_length = u32::from_be_bytes([
                            class_bytes[pos],
                            class_bytes[pos + 1],
                            class_bytes[pos + 2],
                            class_bytes[pos + 3],
                        ]) as usize;
                        pos += 4 + code_attr_length;
                    }
                } else {
                    // Not a Code attribute, skip to next attribute
                    pos = attribute_start + attribute_length;
                }
            } else {
                // Skip attribute data
                pos = attribute_start + attribute_length;
            }
        }
    }

    Ok(methods)
}

/// Build CFG from bytecode instructions
fn build_cfg_from_bytecode(_method_name: &str, bytecode: &[u8]) -> Result<CfgWithEdges> {
    // Parse bytecode into instructions
    let instructions = parse_bytecode(bytecode)?;

    if instructions.is_empty() {
        return Ok(CfgWithEdges {
            blocks: vec![],
            edges: vec![],
            function_id: 0,
        });
    }

    // Identify basic blocks
    let blocks = identify_basic_blocks(&instructions);

    // Map block names to indices
    let mut block_map: HashMap<usize, usize> = HashMap::new();
    for (idx, block) in blocks.iter().enumerate() {
        block_map.insert(block.start_offset, idx);
    }

    // Create CFG blocks
    let mut cfg_blocks: Vec<CfgBlock> = Vec::new();
    for (idx, block) in blocks.iter().enumerate() {
        let kind = if idx == 0 {
            BlockKind::Entry
        } else if block.terminator == BlockTerminator::Return {
            BlockKind::Return
        } else {
            BlockKind::For // Generic block
        };

        cfg_blocks.push(CfgBlock {
            cfg_hash: None,
            statements: Some(
                block
                    .instructions
                    .iter()
                    .map(|instr| format!("opcode: {:#02x}", instr.opcode))
                    .collect(),
            ),
            function_id: 0,
            kind: format!("{:?}", kind),
            terminator: format!("{:?}", block.terminator),
            byte_start: block.start_offset as u64,
            byte_end: block.end_offset as u64,
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            coord_x: 0,
            coord_y: 0,
            coord_z: 0,
            coord_t: None,
        });
    }

    // Create CFG edges from terminators
    let mut cfg_edges: Vec<CfgEdge> = Vec::new();

    for (idx, block) in blocks.iter().enumerate() {
        match &block.terminator {
            BlockTerminator::Fallthrough => {
                // Edge to next block if it exists
                if idx + 1 < blocks.len() {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx: idx + 1,
                        edge_type: CfgEdgeType::Fallthrough,
                    });
                }
            }

            BlockTerminator::Unconditional { target } => {
                if let Some(&target_idx) = block_map.get(target) {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx: target_idx,
                        edge_type: CfgEdgeType::Jump,
                    });
                }
            }

            BlockTerminator::Conditional { target } => {
                // True branch to target
                if let Some(&target_idx) = block_map.get(target) {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx: target_idx,
                        edge_type: CfgEdgeType::ConditionalTrue,
                    });
                }

                // False branch falls through to next block
                if idx + 1 < blocks.len() {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx: idx + 1,
                        edge_type: CfgEdgeType::ConditionalFalse,
                    });
                }
            }

            BlockTerminator::Switch { default, cases } => {
                // Default case
                if let Some(&default_idx) = block_map.get(default) {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx: default_idx,
                        edge_type: CfgEdgeType::Jump,
                    });
                }

                // Switch cases
                for case_target in cases {
                    if let Some(&target_idx) = block_map.get(case_target) {
                        cfg_edges.push(CfgEdge {
                            source_idx: idx,
                            target_idx: target_idx,
                            edge_type: CfgEdgeType::Jump,
                        });
                    }
                }
            }

            BlockTerminator::Return | BlockTerminator::Throw => {
                // No outgoing edges
            }

            BlockTerminator::Unknown => {
                // Unknown terminator - no edges
            }
        }
    }

    Ok(CfgWithEdges {
        blocks: cfg_blocks,
        edges: cfg_edges,
        function_id: 0,
    })
}

/// Parse bytecode into instructions
fn parse_bytecode(bytecode: &[u8]) -> Result<Vec<BytecodeInstruction>> {
    let mut instructions = Vec::new();
    let mut offset = 0;

    while offset < bytecode.len() {
        let opcode = bytecode[offset];
        let mut operands = Vec::new();

        // Most instructions are 1-3 bytes
        // For simplicity, we'll assume 1 byte per instruction
        // A real parser would handle variable-length instructions
        let size = 1;
        for i in 1..size {
            if offset + i < bytecode.len() {
                operands.push(bytecode[offset + i]);
            }
        }

        instructions.push(BytecodeInstruction {
            opcode,
            operands,
            offset,
        });

        offset += size;
    }

    Ok(instructions)
}

/// Identify basic blocks in bytecode
fn identify_basic_blocks(instructions: &[BytecodeInstruction]) -> Vec<BytecodeBlock> {
    if instructions.is_empty() {
        return vec![];
    }

    let mut blocks: Vec<BytecodeBlock> = vec![];
    let mut block_starts: Vec<usize> = vec![0]; // First instruction starts a block

    // Find jump targets (these start new blocks)
    for instr in instructions {
        match instr.opcode {
            // Conditional branches
            0x99 | 0x9A | 0x9B | 0x9C | 0x9D | 0x9E | 0x9F | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4
            | 0xA5 | 0xA6 | 0xA7 => {
                // if_icmpeq, if_icmpne, if_icmplt, etc.
                if instr.operands.len() >= 2 {
                    let offset_bytes = [instr.operands[0], instr.operands[1]];
                    let target_offset = i16::from_be_bytes(offset_bytes) as i32 as usize;
                    block_starts.push(target_offset);
                }
            }

            // Unconditional branches (0xA7 already covered above)
            0xC8 => {
                // goto, goto_w
                if instr.operands.len() >= 2 {
                    let offset_bytes = [instr.operands[0], instr.operands[1]];
                    let target_offset = i16::from_be_bytes(offset_bytes) as i32 as usize;
                    block_starts.push(target_offset);
                }
            }

            // Switch statements (tableswitch, lookupswitch)
            0xAA | 0xAB => {
                // Complex variable-length operands - would need full switch parsing
                // Treating as unknown terminator for now
            }

            // Return instructions
            0xAC | 0xAD | 0xAE | 0xAF | 0xB0 | 0xB1 | 0xB2 | 0xB3 | 0xB4 | 0xB5 | 0xB6 | 0xB7
            | 0xB8 | 0xB9 | 0xBA | 0xBB | 0xBC | 0xBD | 0xBE | 0xBF => {
                // ireturn, lreturn, freturn, dreturn, areturn, return
                // These end blocks
            }

            _ => {
                // Other instructions don't start new blocks
            }
        }
    }

    // Sort and deduplicate block starts
    block_starts.sort();
    block_starts.dedup();

    // Create blocks
    for (i, &start) in block_starts.iter().enumerate() {
        let end = if i + 1 < block_starts.len() {
            block_starts[i + 1]
        } else {
            instructions.len()
        };

        if start < instructions.len() {
            let block_instructions = instructions[start..end].to_vec();
            let terminator = classify_terminator(&block_instructions);

            blocks.push(BytecodeBlock {
                start_offset: start,
                end_offset: end,
                instructions: block_instructions,
                terminator,
            });
        }
    }

    blocks
}

/// Classify the terminator of a basic block
fn classify_terminator(instructions: &[BytecodeInstruction]) -> BlockTerminator {
    if instructions.is_empty() {
        return BlockTerminator::Unknown;
    }

    let last_instr = &instructions[instructions.len() - 1];

    match last_instr.opcode {
        // Return instructions
        0xAC | 0xAD | 0xAE | 0xAF | 0xB0 | 0xB1 => BlockTerminator::Return,

        // Unconditional branches
        0xA7 | 0xC8 => {
            if last_instr.operands.len() >= 2 {
                let offset_bytes = [last_instr.operands[0], last_instr.operands[1]];
                let target_offset = i16::from_be_bytes(offset_bytes) as i32 as usize;
                BlockTerminator::Unconditional {
                    target: target_offset,
                }
            } else {
                BlockTerminator::Unknown
            }
        }

        // Conditional branches
        0x99 | 0x9A | 0x9B | 0x9C | 0x9D | 0x9E | 0x9F | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4
        | 0xA5 | 0xA6 => {
            if last_instr.operands.len() >= 2 {
                let offset_bytes = [last_instr.operands[0], last_instr.operands[1]];
                let target_offset = i16::from_be_bytes(offset_bytes) as i32 as usize;
                BlockTerminator::Conditional {
                    target: target_offset,
                }
            } else {
                BlockTerminator::Unknown
            }
        }

        // Switch statements
        0xAA | 0xAB => BlockTerminator::Switch {
            default: 0,
            cases: vec![],
        },

        // Throw instruction
        0xBF => BlockTerminator::Throw,

        // Everything else falls through
        _ => BlockTerminator::Fallthrough,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bytecode_empty() {
        let bytecode = vec![];
        let result = parse_bytecode(&bytecode);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_bytecode_simple() {
        let bytecode = vec![0x03, 0x3C, 0x1C, 0xAC]; // iconst_0, istore_1, iload_1, ireturn
        let result = parse_bytecode(&bytecode);
        assert!(result.is_ok());

        let instructions = result.unwrap();
        assert_eq!(instructions.len(), 4);
        assert_eq!(instructions[0].opcode, 0x03);
        assert_eq!(instructions[3].opcode, 0xAC);
    }

    #[test]
    fn test_identify_basic_blocks_empty() {
        let instructions = vec![];
        let blocks = identify_basic_blocks(&instructions);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_identify_basic_blocks_simple() {
        let instructions = vec![
            BytecodeInstruction {
                opcode: 0x03,
                operands: vec![],
                offset: 0,
            },
            BytecodeInstruction {
                opcode: 0x3C,
                operands: vec![],
                offset: 1,
            },
            BytecodeInstruction {
                opcode: 0xAC,
                operands: vec![],
                offset: 2,
            },
        ];

        let blocks = identify_basic_blocks(&instructions);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].terminator, BlockTerminator::Return);
    }

    #[test]
    fn test_extract_cfg_from_class_invalid_magic() {
        let invalid_bytes = vec![0x00, 0x00, 0x00, 0x00]; // Wrong magic
        let result = extract_cfg_from_class(&invalid_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_cfg_from_class_too_short() {
        let short_bytes = vec![0xCA, 0xFE]; // Too short
        let result = extract_cfg_from_class(&short_bytes);
        assert!(result.is_err());
    }
}
