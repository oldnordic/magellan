pub mod c;
pub mod cpp;
pub mod detect;
pub mod java;
pub mod javascript;
pub mod python;
pub mod typescript;

// Re-exports from detect module
pub use detect::{detect_language, Language};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        if self.scopes.is_empty() {
            None
        } else {
            Some(self.scopes.pop().unwrap())
        }
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
    pub fn extract_symbols(&mut self, file_path: PathBuf, source: &[u8]) -> Vec<SymbolFact> {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(), // Parse error: return empty
        };

        let root_node = tree.root_node();
        let mut facts = Vec::new();

        // Walk the tree and extract symbols
        self.walk_tree(&root_node, &source, &file_path, &mut facts);

        facts
    }

    /// Walk tree-sitter tree recursively and extract symbols
    fn walk_tree(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
        facts: &mut Vec<SymbolFact>,
    ) {
        // Check if this node is a symbol we care about
        if let Some(fact) = self.extract_symbol(node, source, file_path) {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(&child, source, file_path, facts);
        }
    }

    /// Extract a symbol fact from a tree-sitter node, if applicable
    fn extract_symbol(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &PathBuf,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "function_item" => SymbolKind::Function,
            "struct_item" => SymbolKind::Class, // Rust struct → Class (language-agnostic)
            "enum_item" => SymbolKind::Enum,
            "trait_item" => SymbolKind::Interface, // Rust trait → Interface (language-agnostic)
            "impl_item" => SymbolKind::Unknown,    // impl blocks have no name in v0
            "mod_item" => SymbolKind::Module,
            _ => return None, // Not a symbol we track
        };

        // Try to extract name
        let name = self.extract_name(node, source);

        let normalized_kind = symbol_kind.normalized_key().to_string();
        let fqn = name.clone(); // For v1, FQN is just the symbol name
        Some(SymbolFact {
            file_path: file_path.clone(),
            kind: symbol_kind,
            kind_normalized: normalized_kind,
            name,
            fqn,
            byte_start: node.start_byte() as usize,
            byte_end: node.end_byte() as usize,
            start_line: node.start_position().row + 1, // tree-sitter is 0-indexed
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
                    let name_bytes =
                        &source[child.start_byte() as usize..child.end_byte() as usize];
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

        let name_bytes = &source[type_node.start_byte() as usize..type_node.end_byte() as usize];
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

        // Should find: struct, inherent impl, trait impl
        let impl_facts: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Unknown)
            .collect();

        // Both impls should have MyStruct as their name
        assert_eq!(impl_facts.len(), 2);
        assert_eq!(impl_facts[0].name, Some("MyStruct".to_string()));
        assert_eq!(impl_facts[1].name, Some("MyStruct".to_string()));
    }
}
