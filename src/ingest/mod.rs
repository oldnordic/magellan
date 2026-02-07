pub mod c;
pub mod cpp;
pub mod detect;
pub mod java;
pub mod javascript;
pub mod pool;
pub mod python;
pub mod typescript;

// Re-exports from detect module
pub use detect::{detect_language, Language};

use crate::common::safe_slice;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Imports for FQN computation
use crate::graph::canonical_fqn::FqnBuilder;
use crate::graph::crate_name::detect_crate_name;

/// Kind of symbol extracted from source code
///
/// Language-agnostic symbol kinds that map across multiple programming languages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SymbolKind {
    /// Function definition
    Function,
    /// Method inside a class/impl block
    Method,
    /// Class or struct-like type definition
    /// Covers: Rust struct, Python class, Java class, C++ class, JS/TS class
    Class,
    /// Interface or trait definition
    /// Covers: Rust trait, Java interface, TypeScript interface
    Interface,
    /// Enum definition
    Enum,
    /// Module or package declaration
    /// Covers: Rust mod, Python module, Java package, JS/TS module
    Module,
    /// Union definition (C/C++)
    Union,
    /// Namespace definition
    /// Covers: C++ namespace, TypeScript namespace
    Namespace,
    /// Type alias
    /// Covers: TypeScript type, Rust type alias
    TypeAlias,
    /// Unknown symbol type
    Unknown,
}

/// Separator character for FQN construction per language
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeSeparator {
    /// Rust, C, C++ use :: separator
    DoubleColon,
    /// Python, Java, JavaScript, TypeScript use . separator
    Dot,
}

impl ScopeSeparator {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScopeSeparator::DoubleColon => "::",
            ScopeSeparator::Dot => ".",
        }
    }
}

/// Stack for tracking scope nesting during tree-sitter traversal
///
/// Maintains a hierarchy of scope names (modules, types, namespaces)
/// to build fully-qualified names for symbols.
///
/// # Example
/// ```rust
/// use magellan::ingest::{ScopeStack, ScopeSeparator};
/// let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
/// stack.push("my_crate");
/// stack.push("my_module");
/// assert_eq!(stack.current_fqn(), "my_crate::my_module");
/// stack.push("MyStruct");
/// assert_eq!(stack.current_fqn(), "my_crate::my_module::MyStruct");
/// ```
#[derive(Debug, Clone)]
pub struct ScopeStack {
    /// Scope components in order (e.g., ["my_crate", "my_module", "MyStruct"])
    scopes: Vec<String>,
    /// Separator for this language
    separator: ScopeSeparator,
}

impl ScopeStack {
    /// Create a new empty scope stack
    pub fn new(separator: ScopeSeparator) -> Self {
        Self {
            scopes: Vec::new(),
            separator,
        }
    }

    /// Push a new scope level onto the stack
    ///
    /// Used when entering a module, class, namespace, or other semantic scope.
    pub fn push(&mut self, scope: impl Into<String>) {
        self.scopes.push(scope.into());
    }

    /// Pop the most recent scope level from the stack
    ///
    /// Used when exiting a module, class, or namespace.
    /// Returns the popped scope name, or None if stack was empty.
    pub fn pop(&mut self) -> Option<String> {
        self.scopes.pop()
    }

    /// Get the current fully-qualified name
    ///
    /// Returns empty string if stack is empty (top-level symbols).
    /// Otherwise returns components joined by separator.
    pub fn current_fqn(&self) -> String {
        if self.scopes.is_empty() {
            String::new()
        } else {
            let sep = self.separator.as_str();
            self.scopes.join(sep)
        }
    }

    /// Get FQN for a symbol within the current scope
    ///
    /// If symbol_name is provided, appends it to current scope.
    /// If no current scope, returns symbol_name only.
    /// If symbol_name is empty and no scope, returns empty (for anonymous symbols).
    pub fn fqn_for_symbol(&self, symbol_name: &str) -> String {
        let current = self.current_fqn();
        if current.is_empty() {
            symbol_name.to_string()
        } else if symbol_name.is_empty() {
            current
        } else {
            format!("{}{}{}", current, self.separator.as_str(), symbol_name)
        }
    }

    /// Get the depth of the scope stack
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Check if stack is empty
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }

    /// Get the separator for this stack
    pub fn separator(&self) -> ScopeSeparator {
        self.separator
    }

    /// Get read-only access to the scope components
    ///
    /// Returns a slice of scope components in order.
    /// Used by FqnBuilder to construct display FQN.
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }
}

impl SymbolKind {
    /// Return the normalized string key for this symbol kind (used for CLI/JSON)
    pub fn normalized_key(&self) -> &'static str {
        match self {
            SymbolKind::Function => "fn",
            SymbolKind::Method => "method",
            SymbolKind::Class => "struct",
            SymbolKind::Interface => "trait",
            SymbolKind::Enum => "enum",
            SymbolKind::Module => "mod",
            SymbolKind::Union => "union",
            SymbolKind::Namespace => "namespace",
            SymbolKind::TypeAlias => "type_alias",
            SymbolKind::Unknown => "unknown",
        }
    }
}

/// A fact about a symbol extracted from source code
///
/// Pure data structure. No behavior. No semantic analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolFact {
    /// File containing this symbol
    pub file_path: PathBuf,
    /// Kind of symbol
    pub kind: SymbolKind,
    /// Canonical kind string (fn/struct/enum/...) derived during ingest
    pub kind_normalized: String,
    /// Symbol name (if any - some symbols like impl blocks may not have names)
    pub name: Option<String>,
    /// Fully-qualified name for stable symbol_id generation
    ///
    /// For v1, this is set to the simple symbol name for top-level symbols.
    /// Future versions will build proper hierarchical FQN (e.g., "module::Struct::method").
    #[serde(default)]
    pub fqn: Option<String>,

    /// Canonical fully-qualified name with file path for unambiguous identity
    #[serde(default)]
    pub canonical_fqn: Option<String>,

    /// Display fully-qualified name for human-readable output
    #[serde(default)]
    pub display_fqn: Option<String>,

    /// Byte offset where symbol starts in file
    pub byte_start: usize,
    /// Byte offset where symbol ends in file
    pub byte_end: usize,
    /// Line where symbol starts (1-indexed)
    pub start_line: usize,
    /// Column where symbol starts (0-indexed, bytes)
    pub start_col: usize,
    /// Line where symbol ends (1-indexed)
    pub end_line: usize,
    /// Column where symbol ends (0-indexed, bytes)
    pub end_col: usize,
}

/// Parser that extracts symbol facts from Rust source code
///
/// Pure function: Input (path, contents) → Output Vec<SymbolFact>
/// No filesystem access. No global state. No caching.
pub struct Parser {
    /// tree-sitter parser for Rust grammar
    parser: tree_sitter::Parser,
}

impl Parser {
    /// Create a new parser for Rust source code
    pub fn new() -> anyhow::Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_rust::language();
        parser.set_language(&language)?;

        Ok(Self { parser })
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
                    if let Some(fact) = Self::extract_symbol_with_fqn_static(
                        node,
                        source,
                        file_path,
                        scope_stack,
                        crate_name,
                    ) {
                        facts.push(fact);
                    }
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
                    if let Some(fact) = Self::extract_symbol_with_fqn_static(
                        node,
                        source,
                        file_path,
                        scope_stack,
                        crate_name,
                    ) {
                        facts.push(fact);
                    }
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
        file_path: &PathBuf,
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
            file_path: file_path.clone(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name: Some(name),
            fqn: Some(fqn),
            canonical_fqn: Some(canonical_fqn),
            display_fqn: Some(display_fqn),
            byte_start: node.start_byte() as usize,
            byte_end: node.end_byte() as usize,
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
                    let name_bytes = safe_slice(source, child.start_byte() as usize, child.end_byte() as usize)?;
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

        let name_bytes = safe_slice(source, type_node.start_byte() as usize, type_node.end_byte() as usize)?;
        std::str::from_utf8(name_bytes).ok().map(|s| s.to_string())
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
                    if let Some(fact) = self.extract_symbol_with_fqn(
                        node,
                        source,
                        file_path,
                        scope_stack,
                        crate_name,
                    ) {
                        facts.push(fact);
                    }
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
                    if let Some(fact) = self.extract_symbol_with_fqn(
                        node,
                        source,
                        file_path,
                        scope_stack,
                        crate_name,
                    ) {
                        facts.push(fact);
                    }
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
        file_path: &PathBuf,
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
            file_path: file_path.clone(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name: Some(name),
            fqn: Some(fqn),
            canonical_fqn: Some(canonical_fqn),
            display_fqn: Some(display_fqn),
            byte_start: node.start_byte() as usize,
            byte_end: node.end_byte() as usize,
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
                    let name_bytes = safe_slice(source, child.start_byte() as usize, child.end_byte() as usize)?;
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
    /// The `type:` field ALWAYS contains the struct name being implemented.
    fn extract_impl_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // Access the 'type' field which always contains the struct name
        let type_node = node.child_by_field_name("type")?;

        let name_bytes = safe_slice(source, type_node.start_byte() as usize, type_node.end_byte() as usize)?;
        std::str::from_utf8(name_bytes).ok().map(|s| s.to_string())
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new().expect("Failed to create parser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_serialization() {
        let fact = SymbolFact {
            file_path: PathBuf::from("/test/file.rs"),
            kind: SymbolKind::Function,
            kind_normalized: SymbolKind::Function.normalized_key().to_string(),
            name: Some("test_fn".to_string()),
            fqn: Some("test_fn".to_string()),
            canonical_fqn: None,
            display_fqn: None,
            byte_start: 0,
            byte_end: 100,
            start_line: 1,
            start_col: 0,
            end_line: 3,
            end_col: 1,
        };

        let json = serde_json::to_string(&fact).unwrap();
        let deserialized: SymbolFact = serde_json::from_str(&json).unwrap();

        assert_eq!(fact.file_path, deserialized.file_path);
        assert_eq!(fact.kind, deserialized.kind);
        assert_eq!(fact.name, deserialized.name);
        assert_eq!(fact.fqn, deserialized.fqn);
    }

    #[test]
    fn test_extract_impl_name_inherent() {
        let source = b"impl MyStruct { pub fn new() -> Self { Self } }";
        let mut parser = Parser::new().unwrap();
        let tree = parser.parser.parse(&source[..], None).unwrap();
        let root = tree.root_node();

        // Find the impl_item node
        let mut cursor = root.walk();
        let impl_node = root
            .children(&mut cursor)
            .find(|n: &tree_sitter::Node| n.kind() == "impl_item")
            .unwrap();

        let name = parser.extract_name(&impl_node, source);
        assert_eq!(name, Some("MyStruct".to_string()));
    }

    #[test]
    fn test_extract_impl_name_trait_impl() {
        let source = b"impl Default for MyStruct { fn default() -> Self { Self } }";
        let mut parser = Parser::new().unwrap();
        let tree = parser.parser.parse(&source[..], None).unwrap();
        let root = tree.root_node();

        // Find the impl_item node
        let mut cursor = root.walk();
        let impl_node = root
            .children(&mut cursor)
            .find(|n: &tree_sitter::Node| n.kind() == "impl_item")
            .unwrap();

        let name = parser.extract_name(&impl_node, source);
        assert_eq!(name, Some("MyStruct".to_string()));
    }

    #[test]
    fn test_extract_impl_name_both() {
        let content = r#"
pub struct MyStruct { pub value: i32 }

impl MyStruct {
    pub fn new() -> Self { Self { value: 42 } }
}

impl Default for MyStruct {
    fn default() -> Self { Self { value: 0 } }
}
"#;

        let mut parser = Parser::new().unwrap();
        let facts = parser.extract_symbols(PathBuf::from("/test.rs"), content.as_bytes());

        // With FQN-aware extraction, impl blocks don't create symbols.
        // Instead, methods get the impl type in their FQN.
        // Should find: struct, new, default
        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(methods.len(), 2);
        // Both methods should have MyStruct in their FQN
        assert!(methods[0].fqn.as_ref().unwrap().contains("MyStruct"));
        assert!(methods[1].fqn.as_ref().unwrap().contains("MyStruct"));
    }

    #[test]
    fn test_fqn_top_level_function() {
        let mut parser = Parser::new().unwrap();
        let source = b"pub fn top_function() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fqn, Some("top_function".to_string()));
    }

    #[test]
    fn test_fqn_module_function() {
        let mut parser = Parser::new().unwrap();
        let source = b"
mod my_module {
    pub fn module_function() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        let funcs: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].fqn, Some("my_module::module_function".to_string()));
    }

    #[test]
    fn test_fqn_nested_modules() {
        let mut parser = Parser::new().unwrap();
        let source = b"
mod outer {
    pub fn outer_fn() {}

    mod inner {
        pub fn inner_fn() {}
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        let funcs: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].fqn, Some("outer::outer_fn".to_string()));
        assert_eq!(funcs[1].fqn, Some("outer::inner::inner_fn".to_string()));
    }

    #[test]
    fn test_fqn_impl_method() {
        let mut parser = Parser::new().unwrap();
        let source = b"
pub struct MyStruct;

impl MyStruct {
    pub fn my_method(&self) {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].fqn, Some("MyStruct::my_method".to_string()));
    }

    #[test]
    fn test_fqn_trait_method() {
        let mut parser = Parser::new().unwrap();
        let source = b"
pub trait MyTrait {
    fn trait_method(&self);
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        // Find the trait method (function-like node inside trait)
        let methods: Vec<_> = facts
            .iter()
            .filter(|f| matches!(f.kind, SymbolKind::Function))
            .collect();

        assert!(!methods.is_empty(), "Should find trait method");
        let method = methods.first().unwrap();
        assert_eq!(method.fqn, Some("MyTrait::trait_method".to_string()));
    }

    #[test]
    fn test_fqn_always_populated() {
        let mut parser = Parser::new().unwrap();
        let source = b"pub fn test_fn() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        assert!(!facts.is_empty());
        assert!(facts[0].fqn.is_some(), "FQN should always be populated");
        assert!(
            !facts[0].fqn.as_ref().unwrap().is_empty(),
            "FQN should not be empty"
        );
    }

    // Tests for canonical_fqn and display_fqn computation

    #[test]
    fn test_canonical_fqn_format() {
        let mut parser = Parser::new().unwrap();
        let source = b"pub fn test_fn() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        assert!(!facts.is_empty());
        let fact = &facts[0];

        // canonical_fqn should be Some and contain file path
        assert!(
            fact.canonical_fqn.is_some(),
            "canonical_fqn should be populated"
        );
        let canonical = fact.canonical_fqn.as_ref().unwrap();

        // Format: crate_name::file_path::Kind symbol_name
        assert!(
            canonical.contains("test.rs"),
            "canonical_fqn should contain file path"
        );
        assert!(
            canonical.contains("Function"),
            "canonical_fqn should contain symbol kind"
        );
        assert!(
            canonical.contains("test_fn"),
            "canonical_fqn should contain symbol name"
        );
    }

    #[test]
    fn test_display_fqn_format() {
        let mut parser = Parser::new().unwrap();
        let source = b"pub fn test_fn() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        assert!(!facts.is_empty());
        let fact = &facts[0];

        // display_fqn should be Some and NOT contain file path
        assert!(
            fact.display_fqn.is_some(),
            "display_fqn should be populated"
        );
        let display = fact.display_fqn.as_ref().unwrap();

        // Display FQN should be human-readable (crate::symbol_name for top-level)
        assert!(
            !display.contains(".rs"),
            "display_fqn should not contain file extension"
        );
        assert!(
            display.contains("test_fn"),
            "display_fqn should contain symbol name"
        );
    }

    #[test]
    fn test_fqn_with_modules() {
        let mut parser = Parser::new().unwrap();
        let source = b"
mod my_module {
    pub fn module_function() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        let funcs: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(funcs.len(), 1);
        let fact = funcs[0];

        // Both FQNs should contain module path
        assert!(
            fact.fqn.as_ref().unwrap().contains("my_module"),
            "fqn should contain module"
        );
        assert!(
            fact.display_fqn.as_ref().unwrap().contains("my_module"),
            "display_fqn should contain module"
        );
    }

    #[test]
    fn test_fqn_with_impl() {
        let mut parser = Parser::new().unwrap();
        let source = b"
pub struct MyStruct;

impl MyStruct {
    pub fn my_method(&self) {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(methods.len(), 1);
        let fact = methods[0];

        // Both FQNs should contain the impl type
        assert!(
            fact.fqn.as_ref().unwrap().contains("MyStruct"),
            "fqn should contain impl type"
        );
        assert!(
            fact.display_fqn.as_ref().unwrap().contains("MyStruct"),
            "display_fqn should contain impl type"
        );
    }

    #[test]
    fn test_crate_name_in_fqn() {
        let mut parser = Parser::new().unwrap();
        let source = b"pub fn test_fn() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        assert!(!facts.is_empty());
        let fact = &facts[0];

        // Both FQNs should contain crate name
        let canonical = fact.canonical_fqn.as_ref().unwrap();
        let display = fact.display_fqn.as_ref().unwrap();

        // Crate name should be at the start
        assert!(
            canonical.starts_with("magellan::"),
            "canonical_fqn should start with crate name"
        );
        assert!(
            display.starts_with("magellan::"),
            "display_fqn should start with crate name"
        );
    }

    #[test]
    fn test_fqn_fields_populated() {
        let mut parser = Parser::new().unwrap();
        let source = b"pub fn test_fn() {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

        assert!(!facts.is_empty());
        let fact = &facts[0];

        // All three FQN fields should be populated
        assert!(fact.fqn.is_some(), "fqn should be Some");
        assert!(fact.canonical_fqn.is_some(), "canonical_fqn should be Some");
        assert!(fact.display_fqn.is_some(), "display_fqn should be Some");

        // canonical_fqn and display_fqn should be different
        assert_ne!(
            fact.canonical_fqn, fact.display_fqn,
            "canonical_fqn and display_fqn should differ"
        );
    }
}

#[cfg(test)]
mod scope_stack_tests {
    use super::*;

    #[test]
    fn test_empty_stack() {
        let stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        assert_eq!(stack.current_fqn(), "");
        assert!(stack.is_empty());
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_push_single_scope() {
        let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        stack.push("my_crate");
        assert_eq!(stack.current_fqn(), "my_crate");
        assert_eq!(stack.depth(), 1);
    }

    #[test]
    fn test_push_multiple_scopes() {
        let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        stack.push("my_crate");
        stack.push("my_module");
        stack.push("MyStruct");
        assert_eq!(stack.current_fqn(), "my_crate::my_module::MyStruct");
        assert_eq!(stack.depth(), 3);
    }

    #[test]
    fn test_pop_scope() {
        let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        stack.push("my_crate");
        stack.push("my_module");
        assert_eq!(stack.pop(), Some("my_module".to_string()));
        assert_eq!(stack.current_fqn(), "my_crate");
        assert_eq!(stack.pop(), Some("my_crate".to_string()));
        assert!(stack.is_empty());
    }

    #[test]
    fn test_fqn_for_symbol() {
        let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        assert_eq!(stack.fqn_for_symbol("top_level_fn"), "top_level_fn");
        stack.push("my_module");
        assert_eq!(stack.fqn_for_symbol("my_fn"), "my_module::my_fn");
        stack.push("MyStruct");
        assert_eq!(
            stack.fqn_for_symbol("method"),
            "my_module::MyStruct::method"
        );
    }

    #[test]
    fn test_fqn_for_anonymous_symbol() {
        let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        stack.push("my_module");
        // Empty symbol name uses parent scope
        assert_eq!(stack.fqn_for_symbol(""), "my_module");
    }

    #[test]
    fn test_dot_separator() {
        let mut stack = ScopeStack::new(ScopeSeparator::Dot);
        stack.push("com");
        stack.push("example");
        stack.push("MyClass");
        assert_eq!(stack.current_fqn(), "com.example.MyClass");
        assert_eq!(
            stack.fqn_for_symbol("myMethod"),
            "com.example.MyClass.myMethod"
        );
    }
}
