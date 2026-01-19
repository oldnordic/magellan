//! Reference and call extraction from Rust source code
//!
//! Extracts factual, byte-accurate references and calls to symbols without semantic analysis.

use crate::ingest::{SymbolFact, SymbolKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A fact about a reference to a symbol
///
/// Pure data structure. No behavior. No semantic resolution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReferenceFact {
    /// File containing this reference
    pub file_path: PathBuf,
    /// Name of the symbol being referenced
    pub referenced_symbol: String,
    /// Byte offset where reference starts in file
    pub byte_start: usize,
    /// Byte offset where reference ends in file
    pub byte_end: usize,
    /// Line where reference starts (1-indexed)
    pub start_line: usize,
    /// Column where reference starts (0-indexed, bytes)
    pub start_col: usize,
    /// Line where reference ends (1-indexed)
    pub end_line: usize,
    /// Column where reference ends (0-indexed, bytes)
    pub end_col: usize,
}

/// A fact about a function call (forward call graph edge)
///
/// Represents: caller function → callee function
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallFact {
    /// File containing this call
    pub file_path: PathBuf,
    /// Name of the calling function
    pub caller: String,
    /// Name of the function being called
    pub callee: String,
    /// Stable symbol ID of the caller (optional, for correlation)
    #[serde(default)]
    pub caller_symbol_id: Option<String>,
    /// Stable symbol ID of the callee (optional, for correlation)
    #[serde(default)]
    pub callee_symbol_id: Option<String>,
    /// Byte offset where call starts in file
    pub byte_start: usize,
    /// Byte offset where call ends in file
    pub byte_end: usize,
    /// Line where call starts (1-indexed)
    pub start_line: usize,
    /// Column where call starts (0-indexed, bytes)
    pub start_col: usize,
    /// Line where call ends (1-indexed)
    pub end_line: usize,
    /// Column where call ends (0-indexed, bytes)
    pub end_col: usize,
}

/// Reference extractor
pub struct ReferenceExtractor {
    parser: tree_sitter::Parser,
}

impl ReferenceExtractor {
    /// Create a new reference extractor
    pub fn new() -> anyhow::Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_rust::language();
        parser.set_language(&language)?;

        Ok(Self { parser })
    }

    /// Extract reference facts from Rust source code
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    /// * `symbols` - Symbols defined in this file (to match against and exclude)
    ///
    /// # Returns
    /// Vector of reference facts found in the source
    ///
    /// # Guarantees
    /// - Pure function: same input → same output
    /// - No side effects
    /// - No filesystem access
    /// - No semantic analysis (textual + position match only)
    pub fn extract_references(
        &mut self,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<ReferenceFact> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let root_node = tree.root_node();
        let mut references = Vec::new();

        // Walk tree and find references
        self.walk_tree_for_references(&root_node, source, &file_path, symbols, &mut references);

        references
    }

    /// Walk tree-sitter tree recursively and extract references
    fn walk_tree_for_references(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbols: &[SymbolFact],
        references: &mut Vec<ReferenceFact>,
    ) {
        // Check if this node is a reference we care about
        if let Some(reference) = self.extract_reference(node, source, file_path, symbols) {
            references.push(reference);

            // Don't recurse into scoped_identifier - we've already handled it
            // This prevents extracting child identifier nodes within it
            if node.kind() == "scoped_identifier" {
                return;
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_references(&child, source, file_path, symbols, references);
        }
    }

    /// Extract a reference fact from a tree-sitter node, if applicable
    fn extract_reference(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbols: &[SymbolFact],
    ) -> Option<ReferenceFact> {
        let kind = node.kind();

        // Only process identifier and scoped_identifier nodes
        match kind {
            "identifier" => {}
            "scoped_identifier" => {}
            _ => return None,
        }

        // Get the text of this node
        let text_bytes = &source[node.start_byte() as usize..node.end_byte() as usize];
        let text = std::str::from_utf8(text_bytes).ok()?;

        // For scoped_identifier (e.g., a::foo), extract the final component
        let symbol_name = if kind == "scoped_identifier" {
            // Split by :: and take the last part
            text.split("::").last().unwrap_or(text)
        } else {
            text
        };

        // Find if this matches any symbol
        let referenced_symbol = symbols
            .iter()
            .find(|s| s.name.as_ref().map(|n| n == symbol_name).unwrap_or(false))?;

        // Check if reference is OUTSIDE the symbol's defining span
        let ref_start = node.start_byte() as usize;
        let ref_end = node.end_byte() as usize;

        // Only apply span filter for same-file references (self-references)
        // Cross-file references should never be filtered by span
        if referenced_symbol.file_path == *file_path && ref_start < referenced_symbol.byte_end {
            return None; // Reference is within defining span (same file only)
        }

        Some(ReferenceFact {
            file_path: file_path.clone(),
            referenced_symbol: symbol_name.to_string(),
            byte_start: ref_start,
            byte_end: ref_end,
            start_line: node.start_position().row + 1,
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }
}

impl Default for ReferenceExtractor {
    fn default() -> Self {
        Self::new().expect("Failed to create reference extractor")
    }
}

/// Extension to Parser for reference extraction (convenience wrapper)
impl crate::ingest::Parser {
    /// Extract reference facts using the inner parser
    pub fn extract_references(
        &mut self,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<ReferenceFact> {
        let mut extractor = ReferenceExtractor::new().unwrap();
        extractor.extract_references(file_path, source, symbols)
    }

    /// Extract function call facts (forward call graph)
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    /// * `symbols` - Symbols defined in this file (to match against)
    ///
    /// # Returns
    /// Vector of CallFact representing caller → callee relationships
    ///
    /// # Guarantees
    /// - Only function calls are extracted (not type references)
    /// - Calls are extracted when a function identifier within a function body
    ///   references another function symbol
    /// - No semantic analysis (AST-based only)
    pub fn extract_calls(
        &mut self,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<CallFact> {
        let mut extractor = CallExtractor::new().unwrap();
        extractor.extract_calls(file_path, source, symbols)
    }
}

/// Call extractor for forward call graph
///
/// Extracts caller → callee relationships from function bodies
pub struct CallExtractor {
    parser: tree_sitter::Parser,
}

impl CallExtractor {
    /// Create a new call extractor
    pub fn new() -> anyhow::Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_rust::language();
        parser.set_language(&language)?;

        Ok(Self { parser })
    }

    /// Extract function call facts from Rust source code
    ///
    /// # Behavior
    /// 1. Parse the source code
    /// 2. Find all function definitions
    /// 3. For each function, find identifier nodes that reference other functions
    /// 4. Create CallFact for each unique caller → callee relationship
    pub fn extract_calls(
        &mut self,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<CallFact> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let root_node = tree.root_node();
        let mut calls = Vec::new();

        // Build map: symbol name → symbol fact (for quick lookup)
        let symbol_map: HashMap<String, &SymbolFact> = symbols
            .iter()
            .filter_map(|s| s.name.as_ref().map(|name| (name.clone(), s)))
            .collect();

        // Filter to only functions (potential callers and callees)
        let functions: Vec<&SymbolFact> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        // Walk tree and find calls within function bodies
        self.walk_tree_for_calls(
            &root_node,
            source,
            &file_path,
            &symbol_map,
            &functions,
            &mut calls,
        );

        calls
    }

    /// Walk tree-sitter tree and extract function calls
    fn walk_tree_for_calls(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbol_map: &HashMap<String, &SymbolFact>,
        _functions: &[&SymbolFact],
        calls: &mut Vec<CallFact>,
    ) {
        self.walk_tree_for_calls_with_caller(node, source, file_path, symbol_map, None, calls);
    }

    /// Walk tree-sitter tree and extract function calls, tracking current function
    fn walk_tree_for_calls_with_caller(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        symbol_map: &HashMap<String, &SymbolFact>,
        current_caller: Option<&SymbolFact>,
        calls: &mut Vec<CallFact>,
    ) {
        let kind = node.kind();

        // Track which function we're inside (if any)
        let caller: Option<&SymbolFact> = if kind == "function_item" {
            // Extract function name - this becomes the new caller for children
            self.extract_function_name(node, source)
                .and_then(|name| symbol_map.get(&name).copied())
        } else {
            current_caller
        };

        // If we have a caller and this is a call_expression, extract the call
        if kind == "call_expression" {
            if let Some(caller_fact) = caller {
                self.extract_calls_in_node(node, source, file_path, caller_fact, symbol_map, calls);
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_calls_with_caller(
                &child, source, file_path, symbol_map, caller, calls,
            );
        }
    }

    /// Extract function name from a function_item node
    fn extract_function_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "type_identifier" {
                let name_bytes = &source[child.start_byte() as usize..child.end_byte() as usize];
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        None
    }

    /// Extract calls within a node (function body)
    fn extract_calls_in_node(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        caller: &SymbolFact,
        symbol_map: &HashMap<String, &SymbolFact>,
        calls: &mut Vec<CallFact>,
    ) {
        // Look for call_expression nodes or identifier nodes
        let kind = node.kind();

        if kind == "call_expression" {
            // Extract the function being called
            if let Some(callee_name) = self.extract_callee_from_call(node, source) {
                // Only create call if callee is a known function symbol
                if symbol_map.contains_key(&callee_name) {
                    let node_start = node.start_byte() as usize;
                    let node_end = node.end_byte() as usize;
                    let call_fact = CallFact {
                        file_path: file_path.clone(),
                        caller: caller.name.clone().unwrap_or_default(),
                        callee: callee_name,
                        caller_symbol_id: None,
                        callee_symbol_id: None,
                        byte_start: node_start,
                        byte_end: node_end,
                        start_line: node.start_position().row + 1,
                        start_col: node.start_position().column,
                        end_line: node.end_position().row + 1,
                        end_col: node.end_position().column,
                    };
                    calls.push(call_fact);
                }
            }
        }
    }

    /// Extract callee name from a call_expression node
    fn extract_callee_from_call(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // The callee is typically the first child (identifier) or a scoped_identifier
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "identifier" {
                let name_bytes = &source[child.start_byte() as usize..child.end_byte() as usize];
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
            // Handle method calls like obj.method() - we want the method name
            if kind == "field_expression" || kind == "method_expression" {
                // For a.b(), extract "b"
                return self.extract_method_name(&child, source);
            }
        }
        None
    }

    /// Extract method name from a field_expression or method_expression
    fn extract_method_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // Look for the field_identifier (method name in a.b())
            if child.kind() == "field_identifier" {
                let name_bytes = &source[child.start_byte() as usize..child.end_byte() as usize];
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        None
    }
}

impl Default for CallExtractor {
    fn default() -> Self {
        Self::new().expect("Failed to create call extractor")
    }
}
