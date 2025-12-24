//! Reference extraction from Rust source code
//!
//! Extracts factual, byte-accurate references to symbols without semantic analysis.

use crate::ingest::SymbolFact;
use serde::{Deserialize, Serialize};
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
        let referenced_symbol = symbols.iter().find(|s| {
            s.name.as_ref().map(|n| n == symbol_name).unwrap_or(false)
        })?;

        // Check if reference is OUTSIDE the symbol's defining span
        let ref_start = node.start_byte() as usize;
        let ref_end = node.end_byte() as usize;

        // Reference must start after the symbol's definition ends
        if ref_start < referenced_symbol.byte_end {
            return None; // Reference is within defining span
        }

        Some(ReferenceFact {
            file_path: file_path.clone(),
            referenced_symbol: symbol_name.to_string(),
            byte_start: ref_start,
            byte_end: ref_end,
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
}
