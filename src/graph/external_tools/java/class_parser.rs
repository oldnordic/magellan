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
    /// Absolute byte offset of this instruction within the method Code array
    byte_offset: usize,
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
    if magic != [0xCA, 0xFE, 0xBA, 0xBE] {
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

    // Parse constant pool, collecting Utf8 entries so method names can be resolved.
    // Indices run 1..constant_pool_count-1 (count-1 real entries).
    // Long (5) and Double (6) occupy two slots.
    let mut utf8_pool: HashMap<usize, String> = HashMap::new();
    let mut cp_idx = 1;
    while cp_idx < constant_pool_count {
        if pos >= class_bytes.len() {
            return Ok(methods);
        }

        let tag = class_bytes[pos];
        pos += 1;

        match tag {
            // Utf8: 2-byte length + bytes
            1 => {
                if pos + 1 >= class_bytes.len() {
                    return Ok(methods);
                }
                let length = u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
                pos += 2;
                if pos + length > class_bytes.len() {
                    return Ok(methods);
                }
                if let Ok(s) = std::str::from_utf8(&class_bytes[pos..pos + length]) {
                    utf8_pool.insert(cp_idx, s.to_owned());
                }
                pos += length;
                cp_idx += 1;
            }
            // Integer, Float, Fieldref, Methodref, InterfaceMethodref, NameAndType: 4 bytes
            3 | 4 | 9 | 10 | 11 | 12 => {
                pos += 4;
                cp_idx += 1;
            }
            // Long, Double: 8 bytes and consume TWO slots
            5 | 6 => {
                pos += 8;
                cp_idx += 2;
            }
            // Class, String: 2 bytes
            7 | 8 => {
                pos += 2;
                cp_idx += 1;
            }
            // MethodHandle: 3 bytes
            15 => {
                pos += 3;
                cp_idx += 1;
            }
            // MethodType: 2 bytes
            16 => {
                pos += 2;
                cp_idx += 1;
            }
            // Dynamic, InvokeDynamic: 4 bytes
            17 | 18 => {
                pos += 4;
                cp_idx += 1;
            }
            _ => {
                // Unknown tag — constant pool parse failed, return empty
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

        // access_flags (2)
        pos += 2;
        // name_index (2) — look up actual method name from constant pool
        let name_index = u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
        pos += 2;
        // descriptor_index (2)
        pos += 2;

        let method_name = utf8_pool
            .get(&name_index)
            .cloned()
            .unwrap_or_else(|| format!("method_{}", methods.len()));

        let attributes_count =
            u16::from_be_bytes([class_bytes[pos], class_bytes[pos + 1]]) as usize;
        pos += 2;

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
            cfg_condition: None,
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
                        target_idx,
                        edge_type: CfgEdgeType::Jump,
                    });
                }
            }

            BlockTerminator::Conditional { target } => {
                // True branch to target
                if let Some(&target_idx) = block_map.get(target) {
                    cfg_edges.push(CfgEdge {
                        source_idx: idx,
                        target_idx,
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
                            target_idx,
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

/// Return the total byte size of a JVM bytecode instruction starting at `pos` in `bytecode`.
///
/// Implements JVMS §6.5. Variable-length instructions (tableswitch, lookupswitch, wide)
/// are computed from the operands. Returns 1 on unknown opcodes to avoid infinite loops.
fn instruction_size(bytecode: &[u8], pos: usize) -> usize {
    if pos >= bytecode.len() {
        return 1;
    }
    match bytecode[pos] {
        // 1 byte: nop, aconst_null, iconst_*, lconst_*, fconst_*, dconst_*
        0x00..=0x0f => 1,
        // bipush
        0x10 => 2,
        // sipush
        0x11 => 3,
        // ldc
        0x12 => 2,
        // ldc_w, ldc2_w
        0x13..=0x14 => 3,
        // iload, lload, fload, dload, aload (with index operand)
        0x15..=0x19 => 2,
        // iload_0..dconst_1 range of no-operand loads/stores/arith
        0x1a..=0x35 => 1,
        // istore, lstore, fstore, dstore, astore (with index)
        0x36..=0x3a => 2,
        // istore_0..sastore (no operand)
        0x3b..=0x56 => 1,
        // pop..lxor, i2l..dcmpg (no operand)
        0x57..=0x98 => 1,
        // ifeq..jsr: opcode + 2-byte branch offset
        0x99..=0xa8 => 3,
        // ret: opcode + 1-byte index
        0xa9 => 2,
        // tableswitch: variable, 4-byte-aligned
        0xaa => {
            let base = pos + 1;
            let pad = (4 - (base % 4)) % 4;
            let ts = base + pad;
            if ts + 11 >= bytecode.len() { return 1; }
            let low  = i32::from_be_bytes([bytecode[ts+4], bytecode[ts+5], bytecode[ts+6], bytecode[ts+7]]);
            let high = i32::from_be_bytes([bytecode[ts+8], bytecode[ts+9], bytecode[ts+10], bytecode[ts+11]]);
            1 + pad + 12 + (high - low + 1).max(0) as usize * 4
        }
        // lookupswitch: variable, 4-byte-aligned
        0xab => {
            let base = pos + 1;
            let pad = (4 - (base % 4)) % 4;
            let ts = base + pad;
            if ts + 7 >= bytecode.len() { return 1; }
            let npairs = i32::from_be_bytes([bytecode[ts+4], bytecode[ts+5], bytecode[ts+6], bytecode[ts+7]]).max(0) as usize;
            1 + pad + 8 + npairs * 8
        }
        // ireturn..return (no operand)
        0xac..=0xb1 => 1,
        // getstatic, putstatic, getfield, putfield, invokevirtual, invokespecial, invokestatic
        0xb2..=0xb8 => 3,
        // invokeinterface: opcode + index(2) + count(1) + 0(1) = 5
        0xb9 => 5,
        // invokedynamic: opcode + index(2) + 0(1) + 0(1) = 5
        0xba => 5,
        // new, anewarray (2-byte index)
        0xbb | 0xbd => 3,
        // newarray (1-byte type)
        0xbc => 2,
        // arraylength, athrow (no operand)
        0xbe..=0xbf => 1,
        // checkcast, instanceof (2-byte index)
        0xc0..=0xc1 => 3,
        // monitorenter, monitorexit (no operand)
        0xc2..=0xc3 => 1,
        // wide prefix
        0xc4 => {
            if pos + 1 >= bytecode.len() { return 2; }
            match bytecode[pos + 1] {
                0x15..=0x19 | 0x36..=0x3a | 0xa9 => 4,
                0x84 => 6,
                _ => 2,
            }
        }
        // multianewarray: opcode + index(2) + dims(1) = 4
        0xc5 => 4,
        // ifnull, ifnonnull (2-byte offset)
        0xc6..=0xc7 => 3,
        // goto_w, jsr_w (4-byte offset)
        0xc8..=0xc9 => 5,
        _ => 1,
    }
}

/// Parse bytecode into instructions, recording the absolute byte offset of each.
fn parse_bytecode(bytecode: &[u8]) -> Result<Vec<BytecodeInstruction>> {
    let mut instructions = Vec::new();
    let mut offset = 0;

    while offset < bytecode.len() {
        let opcode = bytecode[offset];
        let size = instruction_size(bytecode, offset);
        let operand_end = (offset + size).min(bytecode.len());
        let operands = bytecode[offset + 1..operand_end].to_vec();

        instructions.push(BytecodeInstruction { opcode, operands, byte_offset: offset });

        offset += size;
    }

    Ok(instructions)
}

/// Identify basic blocks in bytecode.
///
/// Works in byte-offset space. Branch targets and block boundaries are all absolute
/// byte offsets within the method Code array (matching what `BytecodeInstruction::byte_offset`
/// records). This keeps the model consistent when instruction sizes vary.
fn identify_basic_blocks(instructions: &[BytecodeInstruction]) -> Vec<BytecodeBlock> {
    if instructions.is_empty() {
        return vec![];
    }

    // Block leader set: byte offsets where blocks start.
    // Offset 0 is always a leader.
    let mut leaders: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    leaders.insert(0);

    for instr in instructions {
        let instr_start = instr.byte_offset;
        // Instruction ends at the start of the next instruction.
        // We approximate the instruction end as instr_start + 1 + operands.len().
        let instr_end = instr_start + 1 + instr.operands.len();

        match instr.opcode {
            // Conditional branches: ifeq..if_acmpne, ifnull, ifnonnull (2-byte signed offset)
            0x99..=0xa6 | 0xc6..=0xc7 if instr.operands.len() >= 2 => {
                let rel = i16::from_be_bytes([instr.operands[0], instr.operands[1]]) as isize;
                let target = (instr_start as isize + rel) as usize;
                leaders.insert(target);
                leaders.insert(instr_end); // fall-through also starts a block
            }
            // goto (2-byte signed offset)
            0xa7 if instr.operands.len() >= 2 => {
                let rel = i16::from_be_bytes([instr.operands[0], instr.operands[1]]) as isize;
                let target = (instr_start as isize + rel) as usize;
                leaders.insert(target);
                // No fall-through after unconditional branch
            }
            // goto_w (4-byte signed offset)
            0xc8 if instr.operands.len() >= 4 => {
                let rel = i32::from_be_bytes([
                    instr.operands[0], instr.operands[1], instr.operands[2], instr.operands[3],
                ]) as isize;
                let target = (instr_start as isize + rel) as usize;
                leaders.insert(target);
            }
            // jsr / jsr_w: treat jump target as leader
            0xa8 if instr.operands.len() >= 2 => {
                let rel = i16::from_be_bytes([instr.operands[0], instr.operands[1]]) as isize;
                leaders.insert((instr_start as isize + rel) as usize);
                leaders.insert(instr_end);
            }
            _ => {}
        }
    }

    // Build offset → instruction-slice lookup
    let leaders_vec: Vec<usize> = leaders.into_iter().collect();

    // Map byte offset → index in `instructions`
    let offset_to_idx: std::collections::HashMap<usize, usize> = instructions
        .iter()
        .enumerate()
        .map(|(i, instr)| (instr.byte_offset, i))
        .collect();

    let mut blocks: Vec<BytecodeBlock> = Vec::new();
    for (li, &leader_off) in leaders_vec.iter().enumerate() {
        let next_leader_off = leaders_vec.get(li + 1).copied();

        let start_idx = match offset_to_idx.get(&leader_off) {
            Some(&i) => i,
            None => continue, // leader points past end of method
        };

        // Collect instructions until next leader or end
        let block_instrs: Vec<BytecodeInstruction> = instructions[start_idx..]
            .iter()
            .take_while(|instr| {
                if let Some(next) = next_leader_off {
                    instr.byte_offset < next
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        if block_instrs.is_empty() {
            continue;
        }

        let end_off = next_leader_off
            .unwrap_or_else(|| instructions.last().map(|i| i.byte_offset + 1).unwrap_or(leader_off + 1));

        let terminator = classify_terminator(&block_instrs);
        blocks.push(BytecodeBlock {
            start_offset: leader_off,
            end_offset: end_off,
            instructions: block_instrs,
            terminator,
        });
    }

    blocks
}

/// Classify the terminator of a basic block.
///
/// Branch targets are absolute byte offsets (relative branch offset + instruction byte offset).
fn classify_terminator(instructions: &[BytecodeInstruction]) -> BlockTerminator {
    if instructions.is_empty() {
        return BlockTerminator::Unknown;
    }

    let last = &instructions[instructions.len() - 1];

    match last.opcode {
        0xac..=0xb1 => BlockTerminator::Return,

        // goto (2-byte offset)
        0xa7 if last.operands.len() >= 2 => {
            let rel = i16::from_be_bytes([last.operands[0], last.operands[1]]) as isize;
            BlockTerminator::Unconditional { target: (last.byte_offset as isize + rel) as usize }
        }
        // goto_w (4-byte offset)
        0xc8 if last.operands.len() >= 4 => {
            let rel = i32::from_be_bytes([
                last.operands[0], last.operands[1], last.operands[2], last.operands[3],
            ]) as isize;
            BlockTerminator::Unconditional { target: (last.byte_offset as isize + rel) as usize }
        }

        // ifeq..if_acmpne (2-byte offset), ifnull, ifnonnull
        0x99..=0xa6 | 0xc6..=0xc7 if last.operands.len() >= 2 => {
            let rel = i16::from_be_bytes([last.operands[0], last.operands[1]]) as isize;
            BlockTerminator::Conditional { target: (last.byte_offset as isize + rel) as usize }
        }

        0xaa | 0xab => BlockTerminator::Switch { default: 0, cases: vec![] },

        0xbf => BlockTerminator::Throw,

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
        // iconst_0 (0), istore_1 (1), iload_1 (2), ireturn (3) — linear, 1 block
        let instructions = vec![
            BytecodeInstruction { opcode: 0x03, operands: vec![], byte_offset: 0 },
            BytecodeInstruction { opcode: 0x3c, operands: vec![], byte_offset: 1 },
            BytecodeInstruction { opcode: 0xac, operands: vec![], byte_offset: 2 },
        ];

        let blocks = identify_basic_blocks(&instructions);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].terminator, BlockTerminator::Return);
    }

    #[test]
    fn test_identify_basic_blocks_branch() {
        // ifeq +4 at offset 0 (3-byte instr) → targets offset 4
        // fall-through at offset 3: ireturn
        // taken at offset 4: iconst_1, ireturn
        let instructions = vec![
            BytecodeInstruction { opcode: 0x99, operands: vec![0x00, 0x04], byte_offset: 0 }, // ifeq +4
            BytecodeInstruction { opcode: 0xac, operands: vec![], byte_offset: 3 },           // ireturn (fall-through)
            BytecodeInstruction { opcode: 0x04, operands: vec![], byte_offset: 4 },           // iconst_1 (taken)
            BytecodeInstruction { opcode: 0xac, operands: vec![], byte_offset: 5 },           // ireturn
        ];

        let blocks = identify_basic_blocks(&instructions);
        // Should have 3 blocks: [0..3], [3..4], [4..6]
        assert_eq!(blocks.len(), 3, "expected 3 blocks, got: {:#?}", blocks);
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
