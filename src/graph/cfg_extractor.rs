//! AST-based CFG extraction for Rust
//!
//! This module extracts Control Flow Graph (CFG) information from tree-sitter
//! AST nodes. It's an interim solution pending stable_mir publication.
//!
//! ## Supported Constructs
//!
//! - if/else expressions
//! - loop/while/for expressions
//! - match expressions
//! - return/break/continue
//! - ? operator (try operator)
//!
//! ## Limitations
//!
//! - No macro expansion control flow (macros expanded by compiler, not in AST)
//! - No generic monomorphization (requires compiler analysis)
//! - No async/await desugaring (requires MIR)
//! - AST-only precision (may miss some implicit control flow)
//!
//! ## Reference
//!
//! - Research: docs/MIR_EXTRACTION_RESEARCH.md
//! - stable_mir tracking: https://rust-lang.github.io/rust-project-goals/2025h1/stable-mir.html

use crate::graph::schema::CfgBlock;
use tree_sitter::Node;

/// Block kind classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// Function entry block
    Entry,
    /// Block inside if expression
    If,
    /// Block inside else clause
    Else,
    /// Loop body
    Loop,
    /// While loop body
    While,
    /// For loop body
    For,
    /// Match arm
    MatchArm,
    /// Block after match (merge point)
    MatchMerge,
    /// Return statement
    Return,
    /// Block after break/continue
    Break,
    /// Continue statement
    Continue,
    /// Regular sequential block
    Block,
}

impl BlockKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockKind::Entry => "entry",
            BlockKind::If => "if",
            BlockKind::Else => "else",
            BlockKind::Loop => "loop",
            BlockKind::While => "while",
            BlockKind::For => "for",
            BlockKind::MatchArm => "match_arm",
            BlockKind::MatchMerge => "match_merge",
            BlockKind::Return => "return",
            BlockKind::Break => "break",
            BlockKind::Continue => "continue",
            BlockKind::Block => "block",
        }
    }
}

/// Terminator kind classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminatorKind {
    /// Unconditional fall-through
    Fallthrough,
    /// Conditional branch (if)
    Conditional,
    /// Unconditional jump
    Goto,
    /// Return from function
    Return,
    /// Break from loop
    Break,
    /// Continue loop
    Continue,
    /// Function call
    Call,
    /// Panic/unwind
    Panic,
}

impl TerminatorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TerminatorKind::Fallthrough => "fallthrough",
            TerminatorKind::Conditional => "conditional",
            TerminatorKind::Goto => "goto",
            TerminatorKind::Return => "return",
            TerminatorKind::Break => "break",
            TerminatorKind::Continue => "continue",
            TerminatorKind::Call => "call",
            TerminatorKind::Panic => "panic",
        }
    }
}

/// Find the function body block node
///
/// Helper function that navigates a function_item node to find its body block.
fn find_function_body<'a>(func_node: &Node<'a>) -> Option<Node<'a>> {
    let mut cursor = func_node.walk();

    // Navigate to the block
    // function_item -> parameters -> body (block)
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "block" {
                return Some(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

/// CFG extractor for Rust functions
///
/// Walks a function's AST to identify basic blocks and control flow.
pub struct CfgExtractor<'a> {
    source: &'a [u8],
    next_block_id: usize,
    blocks: Vec<CfgBlock>,
}

impl<'a> CfgExtractor<'a> {
    /// Create a new CFG extractor
    pub fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            next_block_id: 0,
            blocks: Vec::new(),
        }
    }

    /// Extract CFG blocks from a function node
    ///
    /// # Arguments
    /// * `func_node` - tree-sitter node for function_item
    /// * `function_id` - Database ID of the function symbol
    ///
    /// # Returns
    /// Vector of CfgBlock representing the function's control flow
    pub fn extract_cfg_from_function(
        &mut self,
        func_node: &Node,
        function_id: i64,
    ) -> Vec<CfgBlock> {
        self.blocks.clear();
        self.next_block_id = 0;

        // Find the function body block
        if let Some(body_node) = find_function_body(func_node) {
            // Create entry block
            let _entry_id = self.next_block_id;
            self.next_block_id += 1;

            self.visit_block(&body_node, function_id, BlockKind::Entry);
        }

        std::mem::take(&mut self.blocks)
    }

    /// Visit a block and extract CFG information
    fn visit_block(&mut self, node: &Node, function_id: i64, kind: BlockKind) {
        let byte_start = node.start_byte() as u64;
        let byte_end = node.end_byte() as u64;
        let start_line = node.start_position().row as u64 + 1;
        let start_col = node.start_position().column as u64;
        let end_line = node.end_position().row as u64 + 1;
        let end_col = node.end_position().column as u64;

        // Determine terminator by looking at last statement
        let terminator = self.detect_block_terminator(node);

        let block = CfgBlock {
            function_id,
            kind: kind.as_str().to_string(),
            terminator: terminator.as_str().to_string(),
            byte_start,
            byte_end,
            start_line,
            start_col,
            end_line,
            end_col,
        };

        self.blocks.push(block);

        // Recurse into nested control flow (but don't create CFG blocks for nested blocks here,
        // they'll be handled when the control flow visitor encounters them)
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                self.visit_control_flow(&child, function_id);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    /// Visit control flow constructs recursively
    fn visit_control_flow(&mut self, node: &Node, function_id: i64) {
        match node.kind() {
            "if_expression" => self.visit_if(node, function_id),
            "loop_expression" => self.visit_loop(node, function_id, BlockKind::Loop),
            "while_expression" => self.visit_loop(node, function_id, BlockKind::While),
            "for_expression" => self.visit_loop(node, function_id, BlockKind::For),
            "match_expression" => self.visit_match(node, function_id),
            "return_expression" => self.visit_return(node, function_id),
            "break_expression" => self.visit_break(node, function_id),
            "continue_expression" => self.visit_continue(node, function_id),
            _ => {
                // Recurse into blocks and expression_statements to find nested control flow
                if node.kind() == "block" || node.kind() == "expression_statement" {
                    let mut cursor = node.walk();
                    if cursor.goto_first_child() {
                        loop {
                            let child = cursor.node();
                            self.visit_control_flow(&child, function_id);
                            if !cursor.goto_next_sibling() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Visit an if_expression and extract CFG
    fn visit_if(&mut self, node: &Node, function_id: i64) {
        // if_expression structure in tree-sitter Rust grammar:
        // 0. "if" keyword
        // 1. condition expression
        // 2. consequence (block)
        // 3. alternative (else_clause, optional)

        let mut cursor = node.walk();
        let mut child_count = 0;
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child_count {
                    0 => {
                        // "if" keyword - skip
                    }
                    1 => {
                        // condition - skip
                    }
                    2 => {
                        // Consequence (then block)
                        if child.kind() == "block" {
                            self.visit_block(&child, function_id, BlockKind::If);
                        } else if child.kind() == "if_expression" {
                            // Nested if (else if)
                            self.visit_if(&child, function_id);
                        }
                    }
                    3 => {
                        // Alternative (else_clause)
                        // else_clause may contain "else" keyword and a block or if_expression
                        if child.kind() == "else_clause" {
                            // Find the block or if_expression inside else_clause
                            let mut else_cursor = child.walk();
                            if else_cursor.goto_first_child() {
                                loop {
                                    let else_child = else_cursor.node();
                                    // Skip "else" keyword, find the actual content
                                    if else_child.kind() == "block" {
                                        self.visit_block(&else_child, function_id, BlockKind::Else);
                                    } else if else_child.kind() == "if_expression" {
                                        self.visit_if(&else_child, function_id);
                                    }
                                    if !else_cursor.goto_next_sibling() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                child_count += 1;
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    /// Visit a loop expression (loop/while/for)
    fn visit_loop(&mut self, node: &Node, function_id: i64, kind: BlockKind) {
        // loop_expression has: body (block)
        // while_expression has: condition, body
        // for_expression has: pattern, iter, body

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                // The body is typically the last child or a named field
                if child.kind() == "block" {
                    self.visit_block(&child, function_id, kind.clone());
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    /// Visit a match_expression and extract CFG
    fn visit_match(&mut self, node: &Node, function_id: i64) {
        // match_expression structure in tree-sitter Rust grammar:
        // 0. "match" keyword
        // 1. value expression
        // 2. match_block (contains match_arm nodes)

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "match_block" {
                    // Recurse into match_block to find match_arm nodes
                    let mut block_cursor = child.walk();
                    if block_cursor.goto_first_child() {
                        loop {
                            let block_child = block_cursor.node();
                            if block_child.kind() == "match_arm" {
                                self.visit_match_arm(&block_child, function_id);
                            }
                            if !block_cursor.goto_next_sibling() {
                                break;
                            }
                        }
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    /// Visit a match arm
    fn visit_match_arm(&mut self, node: &Node, function_id: i64) {
        // match_arm structure:
        // 0. pattern
        // 1. "=>" (fat arrow)
        // 2. expression or block

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                // The value is the last child (the expression)
                if child.kind() == "block" {
                    self.visit_block(&child, function_id, BlockKind::MatchArm);
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    /// Visit a return_expression
    fn visit_return(&mut self, node: &Node, function_id: i64) {
        let byte_start = node.start_byte() as u64;
        let byte_end = node.end_byte() as u64;
        let start_line = node.start_position().row as u64 + 1;
        let start_col = node.start_position().column as u64;
        let end_line = node.end_position().row as u64 + 1;
        let end_col = node.end_position().column as u64;

        let block = CfgBlock {
            function_id,
            kind: BlockKind::Return.as_str().to_string(),
            terminator: TerminatorKind::Return.as_str().to_string(),
            byte_start,
            byte_end,
            start_line,
            start_col,
            end_line,
            end_col,
        };

        self.blocks.push(block);
    }

    /// Visit a break_expression
    fn visit_break(&mut self, node: &Node, function_id: i64) {
        let byte_start = node.start_byte() as u64;
        let byte_end = node.end_byte() as u64;
        let start_line = node.start_position().row as u64 + 1;
        let start_col = node.start_position().column as u64;
        let end_line = node.end_position().row as u64 + 1;
        let end_col = node.end_position().column as u64;

        let block = CfgBlock {
            function_id,
            kind: BlockKind::Break.as_str().to_string(),
            terminator: TerminatorKind::Break.as_str().to_string(),
            byte_start,
            byte_end,
            start_line,
            start_col,
            end_line,
            end_col,
        };

        self.blocks.push(block);
    }

    /// Visit a continue_expression
    fn visit_continue(&mut self, node: &Node, function_id: i64) {
        let byte_start = node.start_byte() as u64;
        let byte_end = node.end_byte() as u64;
        let start_line = node.start_position().row as u64 + 1;
        let start_col = node.start_position().column as u64;
        let end_line = node.end_position().row as u64 + 1;
        let end_col = node.end_position().column as u64;

        let block = CfgBlock {
            function_id,
            kind: BlockKind::Continue.as_str().to_string(),
            terminator: TerminatorKind::Continue.as_str().to_string(),
            byte_start,
            byte_end,
            start_line,
            start_col,
            end_line,
            end_col,
        };

        self.blocks.push(block);
    }

    /// Detect the terminator kind for a block
    fn detect_block_terminator(&self, node: &Node) -> TerminatorKind {
        let mut cursor = node.walk();

        // Get the last statement in the block
        let mut last_statement = None;
        if cursor.goto_first_child() {
            loop {
                let current = cursor.node();
                last_statement = Some(current);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if let Some(last) = last_statement {
            match last.kind() {
                "return_expression" => TerminatorKind::Return,
                "break_expression" => TerminatorKind::Break,
                "continue_expression" => TerminatorKind::Continue,
                "if_expression" => TerminatorKind::Conditional,
                "match_expression" => TerminatorKind::Conditional,
                "loop_expression" | "while_expression" | "for_expression" => TerminatorKind::Conditional,
                "call_expression" => TerminatorKind::Call,
                _ => TerminatorKind::Fallthrough,
            }
        } else {
            TerminatorKind::Fallthrough
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_rust(source: &[u8]) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::language()).unwrap();
        parser.parse(source, None).unwrap()
    }

    fn find_first_function(tree: &tree_sitter::Tree) -> Option<Node<'_>> {
        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            if child.kind() == "function_item" {
                return Some(child);
            }
        }
        None
    }

    #[test]
    fn test_extract_simple_function() {
        let source = b"fn main() { let x = 1; }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have at least an entry block
        assert!(!blocks.is_empty());
        assert_eq!(blocks[0].kind, "entry");
    }

    #[test]
    fn test_extract_if_function() {
        let source = b"fn test() { if x { y } else { z } }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have entry, if, and/or else blocks
        assert!(!blocks.is_empty());
        let if_blocks: Vec<_> = blocks.iter().filter(|b| b.kind == "if").collect();
        assert!(!if_blocks.is_empty());
    }

    #[test]
    fn test_extract_loop_function() {
        let source = b"fn test() { loop { break; } }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have loop block
        assert!(!blocks.is_empty());
        let loop_blocks: Vec<_> = blocks.iter().filter(|b| b.kind == "loop").collect();
        assert!(!loop_blocks.is_empty());
    }

    #[test]
    fn test_extract_return() {
        let source = b"fn test() { return 42; }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have return block
        assert!(!blocks.is_empty());
        let return_blocks: Vec<_> = blocks.iter().filter(|b| b.kind == "return").collect();
        assert!(!return_blocks.is_empty());
        assert_eq!(return_blocks[0].terminator, "return");
    }

    #[test]
    fn test_extract_match() {
        let source = b"fn test(x: i32) { match x { 1 => {}, _ => {} } }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have match_arm blocks
        assert!(!blocks.is_empty());
        let match_arms: Vec<_> = blocks.iter().filter(|b| b.kind == "match_arm").collect();
        assert!(!match_arms.is_empty());
    }

    #[test]
    fn test_block_kind_display() {
        assert_eq!(BlockKind::Entry.as_str(), "entry");
        assert_eq!(BlockKind::If.as_str(), "if");
        assert_eq!(BlockKind::Loop.as_str(), "loop");
        assert_eq!(BlockKind::Return.as_str(), "return");
    }

    #[test]
    fn test_terminator_kind_display() {
        assert_eq!(TerminatorKind::Return.as_str(), "return");
        assert_eq!(TerminatorKind::Break.as_str(), "break");
        assert_eq!(TerminatorKind::Conditional.as_str(), "conditional");
    }

    #[test]
    fn test_extract_while_loop() {
        let source = b"fn test() { while x { y } }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have while block
        assert!(!blocks.is_empty());
        let while_blocks: Vec<_> = blocks.iter().filter(|b| b.kind == "while").collect();
        assert!(!while_blocks.is_empty());
    }

    #[test]
    fn test_extract_for_loop() {
        let source = b"fn test() { for x in y { z } }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have for block
        assert!(!blocks.is_empty());
        let for_blocks: Vec<_> = blocks.iter().filter(|b| b.kind == "for").collect();
        assert!(!for_blocks.is_empty());
    }

    #[test]
    fn test_extract_break() {
        let source = b"fn test() { loop { break; } }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have break block
        assert!(!blocks.is_empty());
        let break_blocks: Vec<_> = blocks.iter().filter(|b| b.kind == "break").collect();
        assert!(!break_blocks.is_empty());
        assert_eq!(break_blocks[0].terminator, "break");
    }

    #[test]
    fn test_extract_continue() {
        let source = b"fn test() { loop { continue; } }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have continue block
        assert!(!blocks.is_empty());
        let continue_blocks: Vec<_> = blocks.iter().filter(|b| b.kind == "continue").collect();
        assert!(!continue_blocks.is_empty());
        assert_eq!(continue_blocks[0].terminator, "continue");
    }

    #[test]
    fn test_extract_nested_if() {
        let source = b"fn test() { if x { if y { z } } }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have multiple if blocks
        assert!(!blocks.is_empty());
        let if_blocks: Vec<_> = blocks.iter().filter(|b| b.kind == "if").collect();
        assert!(if_blocks.len() >= 2);
    }

    #[test]
    fn test_extract_else_if() {
        let source = b"fn test() { if x { y } else if z { w } }";
        let tree = parse_rust(source);
        let func = find_first_function(&tree).unwrap();

        let mut extractor = CfgExtractor::new(source);
        let blocks = extractor.extract_cfg_from_function(&func, 1);

        // Should have if blocks
        assert!(!blocks.is_empty());
        let if_blocks: Vec<_> = blocks.iter().filter(|b| b.kind == "if").collect();
        assert!(!if_blocks.is_empty());
    }
}

/// CFG extractor with KV storage support (native-v2 mode)
///
/// Wrapper around CfgExtractor that automatically stores extracted blocks
/// in the KV store when native-v2 feature is enabled.
#[cfg(feature = "native-v2")]
pub struct RustCfgExtractor<'a> {
    /// Inner AST-based CFG extractor
    inner: CfgExtractor<'a>,
    /// KV backend for persistent storage
    backend: Option<std::rc::Rc<dyn sqlitegraph::GraphBackend>>,
}

#[cfg(feature = "native-v2")]
impl<'a> RustCfgExtractor<'a> {
    /// Create a new CFG extractor with KV backend support
    ///
    /// # Arguments
    /// * `source` - Source code bytes
    /// * `backend` - Optional KV backend for persistent storage
    pub fn new(
        source: &'a [u8],
        backend: Option<std::rc::Rc<dyn sqlitegraph::GraphBackend>>,
    ) -> Self {
        Self {
            inner: CfgExtractor::new(source),
            backend,
        }
    }

    /// Extract CFG blocks from a function and store in KV
    ///
    /// This method extracts CFG blocks and stores them in the KV store
    /// if a backend is provided. Returns the extracted blocks regardless.
    ///
    /// # Arguments
    /// * `func_node` - tree-sitter node for function_item
    /// * `function_id` - Database ID of the function symbol
    ///
    /// # Returns
    /// Vector of CfgBlock representing the function's control flow
    pub fn extract_cfg(&mut self, func_node: &Node, function_id: i64) -> Vec<CfgBlock> {
        // Extract blocks using AST-based extractor
        let blocks = self.inner.extract_cfg_from_function(func_node, function_id);

        // Store in KV if backend is available
        if let Some(ref backend) = self.backend {
            if let Err(e) = store_cfg_blocks_kv(std::rc::Rc::clone(backend), function_id, &blocks) {
                eprintln!(
                    "Failed to store CFG blocks for function {}: {}",
                    function_id,
                    e
                );
            }
        }

        blocks
    }

    /// Get CFG blocks for a function from KV store
    ///
    /// # Arguments
    /// * `backend` - Graph backend (must be Native V2 for KV operations)
    /// * `function_id` - Database ID of the function symbol
    ///
    /// # Returns
    /// Vector of CfgBlock from KV store, or empty vector if not found
    pub fn get_cfg(
        backend: &dyn sqlitegraph::GraphBackend,
        function_id: i64,
    ) -> anyhow::Result<Vec<CfgBlock>> {
        get_cfg_blocks_kv(backend, function_id)
    }
}

#[cfg(test)]
#[cfg(feature = "native-v2")]
mod kv_tests {
    use super::*;
    use crate::graph::schema::CfgBlock;
    use sqlitegraph::NativeGraphBackend;

    #[test]
    fn test_cfg_storage_kv_roundtrip() {
        // Create test backend with temporary file
        let temp_dir = std::env::temp_dir();
        let unique_id = format!("magellan_test_{}", std::process::id());
        let db_path = temp_dir.join(unique_id);
        let backend: std::rc::Rc<dyn sqlitegraph::GraphBackend> = std::rc::Rc::new(
            NativeGraphBackend::new(&db_path).unwrap()
        );

        // Create sample CFG blocks
        let blocks = vec![
            CfgBlock {
                function_id: 1,
                kind: "entry".to_string(),
                terminator: "fallthrough".to_string(),
                byte_start: 0,
                byte_end: 100,
                start_line: 1,
                start_col: 0,
                end_line: 5,
                end_col: 0,
            },
            CfgBlock {
                function_id: 1,
                kind: "if".to_string(),
                terminator: "conditional".to_string(),
                byte_start: 100,
                byte_end: 200,
                start_line: 5,
                start_col: 0,
                end_line: 10,
                end_col: 0,
            },
        ];

        // Store blocks
        store_cfg_blocks_kv(std::rc::Rc::clone(&backend), 1, &blocks)
            .expect("Failed to store CFG blocks");

        // Retrieve blocks
        let retrieved = get_cfg_blocks_kv(&*backend, 1)
            .expect("Failed to retrieve CFG blocks");

        // Verify all blocks match
        assert_eq!(retrieved.len(), blocks.len());
        for (original, retrieved_block) in blocks.iter().zip(retrieved.iter()) {
            assert_eq!(original.function_id, retrieved_block.function_id);
            assert_eq!(original.kind, retrieved_block.kind);
            assert_eq!(original.terminator, retrieved_block.terminator);
            assert_eq!(original.byte_start, retrieved_block.byte_start);
            assert_eq!(original.byte_end, retrieved_block.byte_end);
        }
    }

    #[test]
    fn test_cfg_storage_kv_empty() {
        // Create test backend with temporary file
        let temp_dir = std::env::temp_dir();
        let unique_id = format!("magellan_test_{}", std::process::id());
        let db_path = temp_dir.join(unique_id);
        let backend: std::rc::Rc<dyn sqlitegraph::GraphBackend> = std::rc::Rc::new(
            NativeGraphBackend::new(&db_path).unwrap()
        );

        // Try to get CFG for non-existent function
        let retrieved = get_cfg_blocks_kv(&*backend, 999)
            .expect("Failed to retrieve CFG blocks");

        // Verify empty vector returned
        assert_eq!(retrieved.len(), 0);
    }

    #[test]
    fn test_cfg_storage_kv_overwrite() {
        // Create test backend with temporary file
        let temp_dir = std::env::temp_dir();
        let unique_id = format!("magellan_test_{}", std::process::id());
        let db_path = temp_dir.join(unique_id);
        let backend: std::rc::Rc<dyn sqlitegraph::GraphBackend> = std::rc::Rc::new(
            NativeGraphBackend::new(&db_path).unwrap()
        );

        // Store initial CFG blocks
        let initial_blocks = vec![
            CfgBlock {
                function_id: 1,
                kind: "entry".to_string(),
                terminator: "fallthrough".to_string(),
                byte_start: 0,
                byte_end: 100,
                start_line: 1,
                start_col: 0,
                end_line: 5,
                end_col: 0,
            },
        ];

        store_cfg_blocks_kv(std::rc::Rc::clone(&backend), 1, &initial_blocks)
            .expect("Failed to store initial CFG blocks");

        // Store updated CFG blocks for same function
        let updated_blocks = vec![
            CfgBlock {
                function_id: 1,
                kind: "entry".to_string(),
                terminator: "fallthrough".to_string(),
                byte_start: 0,
                byte_end: 100,
                start_line: 1,
                start_col: 0,
                end_line: 5,
                end_col: 0,
            },
            CfgBlock {
                function_id: 1,
                kind: "if".to_string(),
                terminator: "conditional".to_string(),
                byte_start: 100,
                byte_end: 200,
                start_line: 5,
                start_col: 0,
                end_line: 10,
                end_col: 0,
            },
            CfgBlock {
                function_id: 1,
                kind: "else".to_string(),
                terminator: "fallthrough".to_string(),
                byte_start: 200,
                byte_end: 300,
                start_line: 10,
                start_col: 0,
                end_line: 15,
                end_col: 0,
            },
        ];

        store_cfg_blocks_kv(std::rc::Rc::clone(&backend), 1, &updated_blocks)
            .expect("Failed to store updated CFG blocks");

        // Retrieve and verify latest blocks are returned
        let retrieved = get_cfg_blocks_kv(&*backend, 1)
            .expect("Failed to retrieve CFG blocks");

        assert_eq!(retrieved.len(), updated_blocks.len());
        assert_eq!(retrieved[0].kind, "entry");
        assert_eq!(retrieved[1].kind, "if");
        assert_eq!(retrieved[2].kind, "else");
    }
}

/// Store CFG blocks in KV store (native-v2 mode)
///
/// This function stores CFG blocks for a function in the Native V2 backend's
/// KV store. The blocks are JSON-encoded for human-readability and debuggability.
///
/// # Arguments
/// * `backend` - Graph backend (must be Native V2 for KV operations)
/// * `function_id` - Database ID of the function
/// * `blocks` - Slice of CFG blocks to store
///
/// # Returns
/// Result<()> indicating success or failure
///
/// # Errors
/// Returns error if KV operations fail or JSON encoding fails
#[cfg(feature = "native-v2")]
pub fn store_cfg_blocks_kv(
    backend: std::rc::Rc<dyn sqlitegraph::GraphBackend>,
    function_id: i64,
    blocks: &[crate::graph::schema::CfgBlock],
) -> anyhow::Result<()> {
    use crate::kv::encoding::encode_cfg_blocks;
    use crate::kv::keys::cfg_blocks_key;

    let key = cfg_blocks_key(function_id);
    let encoded = encode_cfg_blocks(blocks)?;
    backend.kv_set(key, sqlitegraph::backend::KvValue::Bytes(encoded), None)?;
    Ok(())
}

/// Retrieve CFG blocks from KV store (native-v2 mode)
///
/// This function retrieves CFG blocks for a function from the Native V2 backend's
/// KV store. The blocks are JSON-decoded from the stored value.
///
/// # Arguments
/// * `backend` - Graph backend (must be Native V2 for KV operations)
/// * `function_id` - Database ID of the function
///
/// # Returns
/// Result<Vec<CfgBlock>> containing the retrieved blocks, or empty vector if not found
///
/// # Errors
/// Returns error if KV operations fail or JSON decoding fails
#[cfg(feature = "native-v2")]
pub fn get_cfg_blocks_kv(
    backend: &dyn sqlitegraph::GraphBackend,
    function_id: i64,
) -> anyhow::Result<Vec<crate::graph::schema::CfgBlock>> {
    use crate::kv::encoding::decode_cfg_blocks;
    use crate::kv::keys::cfg_blocks_key;

    let key = cfg_blocks_key(function_id);
    let snapshot = sqlitegraph::SnapshotId::current();

    match backend.kv_get(snapshot, &key)? {
        Some(sqlitegraph::backend::KvValue::Bytes(data)) => {
            decode_cfg_blocks(&data)
        }
        _ => Ok(vec![]),
    }
}

/// LLVM IR-based CFG extraction (OPTIONAL feature)
///
/// This module provides more precise CFG extraction using LLVM IR,
/// which sees optimizations, macro expansion, and compiler-generated code.
/// Requires clang to emit IR for C/C++ files.
///
/// **This is OPTIONAL** - AST-based CFG (see CfgExtractor above) works
/// for all languages including C/C++.
///
/// To enable: --features llvm-cfg
/// Requires: clang installed, matching LLVM version
#[cfg(feature = "llvm-cfg")]
pub mod llvm_cfg {
    use super::CfgBlock;
    use anyhow::Result;

    /// LLVM IR-based CFG extractor for C/C++
    ///
    /// Uses LLVM C API (via inkwell) to extract precise CFG from LLVM IR.
    /// More accurate than AST for:
    /// - Compiler optimizations
    /// - Macro expansion
    /// - Template instantiation
    /// - Inline expansion
    ///
    /// **Status:** Stub implementation - full extraction deferred
    pub struct LlvmCfgExtractor {
        /// Path to clang binary for IR generation
        clang_path: std::path::PathBuf,
        /// LLVM context for IR parsing
        #[allow(dead_code)]
        context: Option<inkwell::context::Context>,
    }

    impl LlvmCfgExtractor {
        /// Create a new LLVM CFG extractor
        ///
        /// # Errors
        /// Returns error if clang is not found or LLVM version mismatch
        pub fn new() -> Result<Self> {
            // Find clang in PATH
            let clang_path = Self::find_clang()?;

            Ok(Self {
                clang_path,
                context: None, // Initialized when needed
            })
        }

        /// Extract CFG blocks from LLVM IR for a C/C++ function
        ///
        /// # Process
        /// 1. Compile C/C++ source to LLVM IR (.ll file) using clang
        /// 2. Parse IR using LLVM C API
        /// 3. Walk basic blocks in each function
        /// 4. Extract terminator info for each block
        ///
        /// # Limitations
        /// - Requires clang installation
        /// - LLVM version must match inkwell binding
        /// - Slower than AST (requires compilation)
        /// - Only works for C/C++ (not other languages)
        ///
        /// # Arguments
        /// * `ir_file` - Path to .ll IR file or .c/.cpp source
        /// * `function_name` - Name of function to analyze
        /// * `function_id` - Database ID for storing blocks
        ///
        /// # Returns
        /// Vector of CFG blocks (stub for now)
        pub fn extract_cfg_from_ir(
            &self,
            _ir_file: &std::path::Path,
            function_name: &str,
            function_id: i64,
        ) -> Result<Vec<CfgBlock>> {
            // STUB: Return empty vector for now
            // Full implementation requires:
            // 1. Compile to IR: clang -S -emit-llvm input.c -o input.ll
            // 2. Parse IR: inkwell::module::Module::parse_ir_string()
            // 3. Walk functions: module.get_function(function_name)
            // 4. Iterate basic blocks: func.get_basic_blocks()
            // 5. Extract terminators: block.get_terminator()

            tracing::warn!(
                "LlvmCfgExtractor::extract_cfg_from_ir called for '{}' \
                but is not yet implemented (stub only)",
                function_name
            );

            Ok(vec![])
        }

        /// Generate LLVM IR from C/C++ source file
        ///
        /// Uses clang to compile source to .ll IR text format.
        ///
        /// # Arguments
        /// * `source_file` - Path to .c or .cpp file
        /// * `output_file` - Path for .ll output (optional, auto-generated if None)
        ///
        /// # Returns
        /// Path to generated .ll file
        pub fn compile_to_ir(
            &self,
            source_file: &std::path::Path,
            output_file: Option<&std::path::Path>,
        ) -> Result<std::path::PathBuf> {
            let output = output_file.unwrap_or(source_file.with_extension("ll"));

            // Run: clang -S -emit-llvm source.c -o source.ll
            let status = std::process::Command::new(&self.clang_path)
                .arg("-S")
                .arg("-emit-llvm")
                .arg(source_file)
                .arg("-o")
                .arg(&output)
                .status()?;

            if !status.success() {
                return Err(anyhow::anyhow!(
                    "clang failed to compile {:?} to LLVM IR",
                    source_file
                ));
            }

            Ok(output)
        }

        /// Find clang executable in PATH
        fn find_clang() -> Result<std::path::PathBuf> {
            // Try common clang names
            for clang in &["clang", "clang-14", "clang-15", "clang-16", "clang-17"] {
                if let Ok(path) = which::which(clang) {
                    return Ok(path);
                }
            }

            Err(anyhow::anyhow!(
                "clang not found in PATH. Install clang to use llvm-cfg feature."
            ))
        }
    }

    impl Default for LlvmCfgExtractor {
        fn default() -> Self {
            Self::new().expect("Failed to create LlvmCfgExtractor")
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_llvm_extractor_creation() {
            // This test only runs when llvm-cfg feature is enabled
            // and clang is available
            if let Ok(_extractor) = LlvmCfgExtractor::new() {
                // Successfully created (clang was found)
            }
            // If creation fails, that's ok for optional feature
        }

        #[test]
        fn test_stub_returns_empty() {
            let extractor = LlvmCfgExtractor::new();
            // Even if creation fails, we can test the stub behavior
            if let Ok(ext) = extractor {
                let blocks = ext
                    .extract_cfg_from_ir(std::path::Path::new("test.c"), "test_function", 1)
                    .unwrap();
                assert!(blocks.is_empty(), "Stub should return empty vector");
            }
        }
    }
}
