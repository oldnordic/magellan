pub mod c;
pub mod cpp;
mod cpp_symbols;
pub mod cuda;
pub mod detect;
pub mod fqn_resolver;
pub mod generic_extraction;
pub mod go;
pub mod imports;
pub mod java;
mod java_symbols;
pub mod javascript;
pub mod pool;
pub mod python;
mod types;
pub mod typescript;

// Re-exports from detect module
pub use detect::{detect_language, Language};
// Re-exports from imports module
pub use imports::{ImportFact, ImportKind};
// Re-exports from fqn_resolver module
pub use fqn_resolver::{build_fqn_map, resolve_qualified_symbol};
pub use types::{ImplRelation, ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};

use crate::common::safe_slice;
use std::path::{Path, PathBuf};

// Imports for FQN computation
use crate::graph::canonical_fqn::FqnBuilder;
use crate::graph::crate_name::detect_crate_name;

/// Parser that extracts symbol facts from Rust source code
///
/// Pure function: Input (path, contents) → Output `Vec<SymbolFact>`
/// No filesystem access. No global state. No caching.
pub struct Parser {
    /// tree-sitter parser for Rust grammar
    pub(crate) parser: tree_sitter::Parser,
}

impl Parser {
    /// Create a new parser for Rust source code
    pub fn new() -> anyhow::Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language)?;

        Ok(Self { parser })
    }

    /// Create parser wrapper from an existing tree-sitter parser
    pub(crate) fn from_parser(parser: tree_sitter::Parser) -> Self {
        Self { parser }
    }

    /// Extract symbol facts from Rust source code
    ///
    /// # Deprecated
    ///
    /// This method creates a new parser instance per call, which is inefficient
    /// for batch processing. Use `extract_symbols_with_parser` with the thread-local
    /// parser pool instead:
    ///
    /// ```rust,ignore
    /// use crate::ingest::pool::with_parser;
    /// use crate::ingest::Language;
    ///
    /// let facts = with_parser(Language::Rust, |parser| {
    ///     RustParser::extract_symbols_with_parser(parser, file_path, source)
    /// })?;
    /// ```
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    ///
    /// # Returns
    /// Vector of symbol facts found in the source
    ///
    /// # Guarantees
    /// - Pure function: same input → same output
    /// - No side effects
    /// - No filesystem access
    #[deprecated(
        since = "1.7.0",
        note = "Use extract_symbols_with_parser with parser pool for better performance"
    )]
    pub fn extract_symbols(&mut self, file_path: PathBuf, source: &[u8]) -> Vec<SymbolFact> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(), // Parse error: return empty
        };

        // Detect crate name for FQN computation
        // Use current directory as project root for crate detection
        let project_root = std::path::Path::new(".");
        let crate_name = detect_crate_name(project_root, &file_path);

        let root_node = tree.root_node();
        let mut facts = Vec::new();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);

        // Walk tree with scope tracking
        self.walk_tree_with_scope(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            &crate_name,
        );

        facts
    }

    /// Extract symbol facts using an external parser (for parser pooling).
    ///
    /// This static method allows sharing a parser instance across multiple calls,
    /// reducing allocation overhead when parsing many files.
    pub fn extract_symbols_with_parser(
        parser: &mut tree_sitter::Parser,
        file_path: PathBuf,
        source: &[u8],
    ) -> Vec<SymbolFact> {
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        // Detect crate name for FQN computation
        // Use current directory as project root for crate detection
        let project_root = std::path::Path::new(".");
        let crate_name = detect_crate_name(project_root, &file_path);

        let root_node = tree.root_node();
        let mut facts = Vec::new();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);

        // Walk tree with scope tracking
        Self::walk_tree_with_scope_static(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            &crate_name,
        );

        facts
    }

    /// Extract symbol facts from a pre-parsed tree.
    ///
    /// This static method allows extracting symbols without re-parsing,
    /// which is useful when the tree is already available from a previous parse.
    ///
    /// # Arguments
    /// * `tree` - The pre-parsed tree-sitter tree
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    ///
    /// # Returns
    /// Vector of symbol facts found in the source
    pub fn extract_symbols_from_tree(
        tree: &tree_sitter::Tree,
        file_path: PathBuf,
        source: &[u8],
    ) -> Vec<SymbolFact> {
        // Detect crate name for FQN computation
        let project_root = std::path::Path::new(".");
        let crate_name = detect_crate_name(project_root, &file_path);

        let root_node = tree.root_node();
        let mut facts = Vec::new();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);

        // Walk tree with scope tracking
        Self::walk_tree_with_scope_static(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            &crate_name,
        );

        facts
    }

    /// Static version of walk_tree_with_scope for external parser usage.
    fn walk_tree_with_scope_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
        crate_name: &str,
    ) {
        let kind = node.kind();

        // Track scope boundaries
        match kind {
            "mod_item" => {
                // Extract module name and push to scope
                if let Some(name) = Self::extract_name_static(node, source) {
                    scope_stack.push(&name);

                    // Create symbol fact for the module directly (extract_symbol_with_fqn_static skips mod_item)
                    let symbol_kind = SymbolKind::Module;
                    let normalized_kind = symbol_kind.normalized_key().to_string();
                    let fqn = scope_stack.fqn_for_symbol(&name);
                    let builder = FqnBuilder::new(
                        crate_name.to_string(),
                        file_path.to_string_lossy().to_string(),
                        ScopeSeparator::DoubleColon,
                    );
                    let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
                    let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

                    facts.push(SymbolFact {
                        file_path: file_path.clone(),
                        kind: symbol_kind,
                        kind_normalized: normalized_kind,
                        name: Some(name),
                        fqn: Some(fqn),
                        canonical_fqn: Some(canonical_fqn),
                        display_fqn: Some(display_fqn),
                        byte_start: node.start_byte(),
                        byte_end: node.end_byte(),
                        start_line: node.start_position().row + 1,
                        start_col: node.start_position().column,
                        end_line: node.end_position().row + 1,
                        end_col: node.end_position().column,
                    });

                    // Recurse into children (they're in this module's scope)
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        Self::walk_tree_with_scope_static(
                            &child,
                            source,
                            file_path,
                            facts,
                            scope_stack,
                            crate_name,
                        );
                    }
                    scope_stack.pop();
                    return;
                }
            }
            "impl_item" => {
                // impl blocks don't add to FQN (syntactic, not semantic)
                // But we need to track them for method scoping
                if let Some(type_name) = Self::extract_impl_name_static(node, source) {
                    scope_stack.push(&type_name);
                    // Don't create a symbol for the impl block itself
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        Self::walk_tree_with_scope_static(
                            &child,
                            source,
                            file_path,
                            facts,
                            scope_stack,
                            crate_name,
                        );
                    }
                    scope_stack.pop();
                    return;
                }
            }
            "trait_item" => {
                if let Some(name) = Self::extract_name_static(node, source) {
                    scope_stack.push(&name);

                    // Create symbol fact for the trait directly (extract_symbol_with_fqn_static skips trait_item)
                    let symbol_kind = SymbolKind::Interface; // Traits map to Interface
                    let normalized_kind = symbol_kind.normalized_key().to_string();
                    let fqn = scope_stack.fqn_for_symbol(&name);
                    let builder = FqnBuilder::new(
                        crate_name.to_string(),
                        file_path.to_string_lossy().to_string(),
                        ScopeSeparator::DoubleColon,
                    );
                    let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
                    let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

                    facts.push(SymbolFact {
                        file_path: file_path.clone(),
                        kind: symbol_kind,
                        kind_normalized: normalized_kind,
                        name: Some(name),
                        fqn: Some(fqn),
                        canonical_fqn: Some(canonical_fqn),
                        display_fqn: Some(display_fqn),
                        byte_start: node.start_byte(),
                        byte_end: node.end_byte(),
                        start_line: node.start_position().row + 1,
                        start_col: node.start_position().column,
                        end_line: node.end_position().row + 1,
                        end_col: node.end_position().column,
                    });

                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        Self::walk_tree_with_scope_static(
                            &child,
                            source,
                            file_path,
                            facts,
                            scope_stack,
                            crate_name,
                        );
                    }
                    scope_stack.pop();
                    return;
                }
            }
            _ => {}
        }

        // Check if this node is a symbol we care about
        if let Some(fact) =
            Self::extract_symbol_with_fqn_static(node, source, file_path, scope_stack, crate_name)
        {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::walk_tree_with_scope_static(
                &child,
                source,
                file_path,
                facts,
                scope_stack,
                crate_name,
            );
        }
    }

    /// Static version of extract_symbol_with_fqn for external parser usage.
    fn extract_symbol_with_fqn_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        scope_stack: &ScopeStack,
        crate_name: &str,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        // Skip scope-defining nodes (handled in walk_tree_with_scope)
        if matches!(kind, "mod_item" | "impl_item" | "trait_item") {
            return None;
        }

        let symbol_kind = match kind {
            "function_item" | "function_signature_item" => SymbolKind::Function,
            "struct_item" => SymbolKind::Class,
            "enum_item" => SymbolKind::Enum,
            _ => return None,
        };

        let name = Self::extract_name_static(node, source)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from current scope + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        // Build FQN builder for canonical and display FQN computation
        let builder = FqnBuilder::new(
            crate_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::DoubleColon,
        );
        let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
        let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

        Some(SymbolFact {
            file_path: file_path.to_path_buf(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name: Some(name),
            fqn: Some(fqn),
            canonical_fqn: Some(canonical_fqn),
            display_fqn: Some(display_fqn),
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            start_line: node.start_position().row + 1,
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }

    /// Static version of extract_name for external parser usage.
    fn extract_name_static(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let kind = node.kind();

        // For impl_item, extract the struct name being implemented
        if kind == "impl_item" {
            return Self::extract_impl_name_static(node, source);
        }

        // For most items, the name is in a child named "identifier" or "type_identifier"
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "type_identifier" => {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                _ => {}
            }
        }

        None
    }

    /// Static version of extract_impl_name for external parser usage.
    fn extract_impl_name_static(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // Access the 'type' field which always contains the struct name
        let type_node = node.child_by_field_name("type")?;

        let name_bytes = safe_slice(source, type_node.start_byte(), type_node.end_byte())?;
        std::str::from_utf8(name_bytes).ok().map(|s| s.to_string())
    }

    /// Extract all impl relationships from a parsed tree.
    ///
    /// Walks the tree for `impl_item` nodes and extracts:
    /// - Trait impls: `impl Trait for Type` → `ImplRelation { type_name, trait_name: Some("Trait") }`
    /// - Inherent impls: `impl Type` → `ImplRelation { type_name, trait_name: None }`
    ///
    /// This is a separate pass from symbol extraction to keep concerns clean.
    /// Inherent impls are included for use by downstream consumers (e.g., llmgrep
    /// impl block indexing).
    pub fn extract_impl_relations_static(
        tree: &tree_sitter::Tree,
        source: &[u8],
        file_path: &Path,
    ) -> Vec<ImplRelation> {
        let mut relations = Vec::new();

        fn walk(
            node: tree_sitter::Node,
            source: &[u8],
            file_path: &str,
            relations: &mut Vec<ImplRelation>,
        ) {
            if node.kind() == "impl_item" {
                // Extract the implementing type
                let type_name = node
                    .child_by_field_name("type")
                    .and_then(|n| safe_slice(source, n.start_byte(), n.end_byte()))
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .map(|s| s.to_string());

                if let Some(type_name) = type_name {
                    // Extract the trait (if present) — None for inherent impls
                    let trait_name = node
                        .child_by_field_name("trait")
                        .and_then(|n| safe_slice(source, n.start_byte(), n.end_byte()))
                        .and_then(|b| std::str::from_utf8(b).ok())
                        .map(|s| s.to_string());

                    relations.push(ImplRelation {
                        type_name,
                        trait_name,
                        file_path: file_path.to_string(),
                        byte_start: node.start_byte(),
                        byte_end: node.end_byte(),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                    });
                }
            }

            for child in node.children(&mut node.walk()) {
                walk(child, source, file_path, relations);
            }
        }

        walk(
            tree.root_node(),
            source,
            &file_path.to_string_lossy(),
            &mut relations,
        );
        relations
    }

    /// Walk tree-sitter tree recursively with scope tracking
    ///
    /// Tracks module, impl, and trait scope boundaries to build proper FQNs.
    /// - mod_item: pushes module name to scope
    /// - impl_item: pushes type name for method scoping (doesn't create symbol)
    /// - trait_item: pushes trait name to scope
    fn walk_tree_with_scope(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
        crate_name: &str,
    ) {
        let kind = node.kind();

        // Track scope boundaries
        match kind {
            "mod_item" => {
                // Extract module name and push to scope
                if let Some(name) = self.extract_name(node, source) {
                    scope_stack.push(&name);

                    // Create symbol fact for the module directly (extract_symbol_with_fqn skips mod_item)
                    let symbol_kind = SymbolKind::Module;
                    let normalized_kind = symbol_kind.normalized_key().to_string();
                    let fqn = scope_stack.fqn_for_symbol(&name);
                    let builder = FqnBuilder::new(
                        crate_name.to_string(),
                        file_path.to_string_lossy().to_string(),
                        ScopeSeparator::DoubleColon,
                    );
                    let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
                    let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

                    facts.push(SymbolFact {
                        file_path: file_path.clone(),
                        kind: symbol_kind,
                        kind_normalized: normalized_kind,
                        name: Some(name),
                        fqn: Some(fqn),
                        canonical_fqn: Some(canonical_fqn),
                        display_fqn: Some(display_fqn),
                        byte_start: node.start_byte(),
                        byte_end: node.end_byte(),
                        start_line: node.start_position().row + 1,
                        start_col: node.start_position().column,
                        end_line: node.end_position().row + 1,
                        end_col: node.end_position().column,
                    });

                    // Recurse into children (they're in this module's scope)
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        self.walk_tree_with_scope(
                            &child,
                            source,
                            file_path,
                            facts,
                            scope_stack,
                            crate_name,
                        );
                    }
                    scope_stack.pop();
                    return;
                }
            }
            "impl_item" => {
                // impl blocks don't add to FQN (syntactic, not semantic)
                // But we need to track them for method scoping
                if let Some(type_name) = self.extract_impl_name(node, source) {
                    scope_stack.push(&type_name);
                    // Don't create a symbol for the impl block itself
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        self.walk_tree_with_scope(
                            &child,
                            source,
                            file_path,
                            facts,
                            scope_stack,
                            crate_name,
                        );
                    }
                    scope_stack.pop();
                    return;
                }
            }
            "trait_item" => {
                if let Some(name) = self.extract_name(node, source) {
                    scope_stack.push(&name);

                    // Create symbol fact for the trait directly (extract_symbol_with_fqn skips trait_item)
                    let symbol_kind = SymbolKind::Interface; // Traits map to Interface
                    let normalized_kind = symbol_kind.normalized_key().to_string();
                    let fqn = scope_stack.fqn_for_symbol(&name);
                    let builder = FqnBuilder::new(
                        crate_name.to_string(),
                        file_path.to_string_lossy().to_string(),
                        ScopeSeparator::DoubleColon,
                    );
                    let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
                    let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

                    facts.push(SymbolFact {
                        file_path: file_path.clone(),
                        kind: symbol_kind,
                        kind_normalized: normalized_kind,
                        name: Some(name),
                        fqn: Some(fqn),
                        canonical_fqn: Some(canonical_fqn),
                        display_fqn: Some(display_fqn),
                        byte_start: node.start_byte(),
                        byte_end: node.end_byte(),
                        start_line: node.start_position().row + 1,
                        start_col: node.start_position().column,
                        end_line: node.end_position().row + 1,
                        end_col: node.end_position().column,
                    });

                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        self.walk_tree_with_scope(
                            &child,
                            source,
                            file_path,
                            facts,
                            scope_stack,
                            crate_name,
                        );
                    }
                    scope_stack.pop();
                    return;
                }
            }
            _ => {}
        }

        // Check if this node is a symbol we care about
        if let Some(fact) =
            self.extract_symbol_with_fqn(node, source, file_path, scope_stack, crate_name)
        {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_with_scope(&child, source, file_path, facts, scope_stack, crate_name);
        }
    }

    /// Extract a symbol fact with FQN from a tree-sitter node, if applicable
    ///
    /// Uses the current scope stack to build a fully-qualified name.
    /// Skips scope-defining nodes (mod_item, impl_item, trait_item) as they
    /// are handled in walk_tree_with_scope.
    fn extract_symbol_with_fqn(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        scope_stack: &ScopeStack,
        crate_name: &str,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        // Skip scope-defining nodes (handled in walk_tree_with_scope)
        if matches!(kind, "mod_item" | "impl_item" | "trait_item") {
            return None;
        }

        let symbol_kind = match kind {
            "function_item" | "function_signature_item" => SymbolKind::Function,
            "struct_item" => SymbolKind::Class,
            "enum_item" => SymbolKind::Enum,
            _ => return None,
        };

        let name = self.extract_name(node, source)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from current scope + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        // Build FQN builder for canonical and display FQN computation
        let builder = FqnBuilder::new(
            crate_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::DoubleColon,
        );
        let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
        let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

        Some(SymbolFact {
            file_path: file_path.to_path_buf(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name: Some(name),
            fqn: Some(fqn),
            canonical_fqn: Some(canonical_fqn),
            display_fqn: Some(display_fqn),
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            start_line: node.start_position().row + 1,
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }

    /// Extract name from a symbol node
    fn extract_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let kind = node.kind();

        // For impl_item, extract the struct name being implemented
        if kind == "impl_item" {
            return self.extract_impl_name(node, source);
        }

        // For most items, the name is in a child named "identifier" or "type_identifier"
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "type_identifier" => {
                    let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                    return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
                }
                _ => {}
            }
        }

        None
    }

    /// Extract the struct name from an impl_item node
    ///
    /// Handles:
    /// - `impl StructName { }` -> returns "StructName"
    /// - `impl Trait for StructName { }` -> returns "StructName"
    ///
    /// In tree-sitter Rust grammar:
    /// - Inherent impl: `impl StructName` -> has `type:` field pointing to StructName
    /// - Trait impl: `impl Trait for StructName` -> has `trait:` field (Trait) and `type:` field (StructName)
    ///   The `type:` field ALWAYS contains the struct name being implemented.
    fn extract_impl_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // Access the 'type' field which always contains the struct name
        let type_node = node.child_by_field_name("type")?;

        let name_bytes = safe_slice(source, type_node.start_byte(), type_node.end_byte())?;
        std::str::from_utf8(name_bytes).ok().map(|s| s.to_string())
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new().expect("Failed to create parser") // M-UNWRAP: tree-sitter rust language is a build-time invariant
    }
}

#[cfg(test)]
#[allow(
    deprecated,
    reason = "tests cover legacy serialization paths that remain accepted on input"
)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scope_stack_tests.rs"]
mod scope_stack_tests;
