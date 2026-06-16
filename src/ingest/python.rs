//! Python symbol extraction using tree-sitter-python.
//!
//! Extracts functions, classes, and async functions from Python source code.

use crate::common::safe_slice;
use crate::graph::canonical_fqn::FqnBuilder;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Parser that extracts symbol facts from Python source code.
///
/// Pure function: Input (path, contents) → Output `Vec<SymbolFact>`
/// No filesystem access. No global state. No caching.
pub struct PythonParser {
    pub(crate) parser: tree_sitter::Parser,
}

impl PythonParser {
    /// Create a new parser for Python source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into())?;
        Ok(Self { parser })
    }

    /// Create parser wrapper from an existing tree-sitter parser
    pub(crate) fn from_parser(parser: tree_sitter::Parser) -> Self {
        Self { parser }
    }

    /// Extract symbol facts from Python source code.
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
        let mut scope_stack = ScopeStack::new(ScopeSeparator::Dot);

        // Use "." as project_root placeholder per decision FQN-17
        let package_name = ".";

        // Walk tree with scope tracking
        self.walk_tree_with_scope(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            package_name,
        );

        facts
    }

    /// Walk tree-sitter tree recursively with scope tracking
    ///
    /// Tracks class scope boundaries to build proper FQNs.
    /// - class_definition: pushes class name to scope
    fn walk_tree_with_scope(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
        package_name: &str,
    ) {
        let kind = node.kind();

        // Track class scope
        if kind == "class_definition" {
            if let Some(name) = self.extract_name(node, source) {
                // Create class symbol with parent scope
                if let Some(fact) =
                    self.extract_symbol_with_fqn(node, source, file_path, scope_stack, package_name)
                {
                    facts.push(fact);
                }
                // Push class scope for children
                scope_stack.push(&name);
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.walk_tree_with_scope(
                        &child,
                        source,
                        file_path,
                        facts,
                        scope_stack,
                        package_name,
                    );
                }
                scope_stack.pop();
                return;
            }
        }

        // Check if this node is a symbol we care about
        if let Some(fact) =
            self.extract_symbol_with_fqn(node, source, file_path, scope_stack, package_name)
        {
            facts.push(fact);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_with_scope(&child, source, file_path, facts, scope_stack, package_name);
        }
    }

    /// Extract a symbol fact with FQN from a tree-sitter node, if applicable
    ///
    /// Uses the current scope stack to build a fully-qualified name.
    /// For class_definition, creates the symbol with parent scope (not including self).
    fn extract_symbol_with_fqn(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        scope_stack: &ScopeStack,
        package_name: &str,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "function_definition" => SymbolKind::Function,
            "class_definition" => SymbolKind::Class,
            _ => return None, // Not a symbol we track
        };

        let name = self.extract_name(node, source)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from current scope + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        // Compute canonical and display FQNs using FqnBuilder
        let builder = FqnBuilder::new(
            package_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::Dot,
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
            start_line: node.start_position().row + 1, // tree-sitter is 0-indexed
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
        })
    }

    /// Extract name from a symbol node.
    fn extract_name(&self, node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // For Python, the name is in a child named "identifier"
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }

        None
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

        let root_node = tree.root_node();
        let mut facts = Vec::new();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::Dot);

        // Use "." as project_root placeholder per decision FQN-17
        let package_name = ".";

        // Walk tree with scope tracking
        Self::walk_tree_with_scope_static(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            package_name,
        );

        facts
    }

    /// Extract symbol facts from a pre-parsed tree.
    ///
    /// Avoids re-parsing when the tree is already available.
    pub fn extract_symbols_from_tree(
        tree: &tree_sitter::Tree,
        file_path: PathBuf,
        source: &[u8],
    ) -> Vec<SymbolFact> {
        let root_node = tree.root_node();
        let mut facts = Vec::new();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::Dot);
        let package_name = ".";
        Self::walk_tree_with_scope_static(
            &root_node,
            source,
            &file_path,
            &mut facts,
            &mut scope_stack,
            package_name,
        );
        facts
    }

    /// Static version of walk_tree_with_scope for external parser usage.
    fn walk_tree_with_scope_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        facts: &mut Vec<SymbolFact>,
        scope_stack: &mut ScopeStack,
        package_name: &str,
    ) {
        let kind = node.kind();

        // Track class scope
        if kind == "class_definition" {
            if let Some(name) = Self::extract_name_static(node, source) {
                // Create class symbol with parent scope
                if let Some(fact) = Self::extract_symbol_with_fqn_static(
                    node,
                    source,
                    file_path,
                    scope_stack,
                    package_name,
                ) {
                    facts.push(fact);
                }
                // Push class scope for children
                scope_stack.push(&name);
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    Self::walk_tree_with_scope_static(
                        &child,
                        source,
                        file_path,
                        facts,
                        scope_stack,
                        package_name,
                    );
                }
                scope_stack.pop();
                return;
            }
        }

        // Check if this node is a symbol we care about
        if let Some(fact) =
            Self::extract_symbol_with_fqn_static(node, source, file_path, scope_stack, package_name)
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
                package_name,
            );
        }
    }

    /// Static version of extract_symbol_with_fqn for external parser usage.
    fn extract_symbol_with_fqn_static(
        node: &tree_sitter::Node,
        source: &[u8],
        file_path: &Path,
        scope_stack: &ScopeStack,
        package_name: &str,
    ) -> Option<SymbolFact> {
        let kind = node.kind();

        let symbol_kind = match kind {
            "function_definition" => SymbolKind::Function,
            "class_definition" => SymbolKind::Class,
            _ => return None,
        };

        let name = Self::extract_name_static(node, source)?;
        let normalized_kind = symbol_kind.normalized_key().to_string();

        // Build FQN from current scope + symbol name
        let fqn = scope_stack.fqn_for_symbol(&name);

        // Compute canonical and display FQNs using FqnBuilder
        let builder = FqnBuilder::new(
            package_name.to_string(),
            file_path.to_string_lossy().to_string(),
            ScopeSeparator::Dot,
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
        // For Python, the name is in a child named "identifier"
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }

        None
    }

    /// Extract reference facts from Python source code.
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    /// * `symbols` - Symbols defined in this file (to match against)
    ///
    /// # Returns
    /// Vector of reference facts found in the source
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
        Self::extract_references_from_tree(&tree, file_path, source, symbols)
    }

    /// Extract reference facts from a pre-parsed tree.
    pub fn extract_references_from_tree(
        tree: &tree_sitter::Tree,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<ReferenceFact> {
        use crate::ingest::generic_extraction;
        generic_extraction::extract_references_from_tree(
            tree,
            file_path,
            source,
            symbols,
            |node| matches!(node.kind(), "identifier" | "attribute"),
            |node, source| {
                let text = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).ok()?;
                Some((text.to_string(), node.kind()))
            },
        )
    }

    /// Extract function call facts from Python source code.
    ///
    /// # Arguments
    /// * `file_path` - Path to the file (for context only, not accessed)
    /// * `source` - Source code content as bytes
    /// * `symbols` - Symbols defined in this file (to match against)
    ///
    /// # Returns
    /// Vector of CallFact representing caller → callee relationships
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
        Self::extract_calls_from_tree(&tree, file_path, source, symbols)
    }

    /// Extract call facts from a pre-parsed tree.
    pub fn extract_calls_from_tree(
        tree: &tree_sitter::Tree,
        file_path: PathBuf,
        source: &[u8],
        symbols: &[SymbolFact],
    ) -> Vec<CallFact> {
        use crate::ingest::generic_extraction;
        generic_extraction::extract_calls_from_tree(
            tree,
            file_path,
            source,
            symbols,
            |node| node.kind() == "function_definition",
            Self::extract_function_name_static,
            "call",
            |node, source| {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "identifier" | "attribute" => {
                            let text =
                                std::str::from_utf8(&source[child.start_byte()..child.end_byte()])
                                    .ok()?;
                            return Some((text.to_string(), child.kind()));
                        }
                        _ => {}
                    }
                }
                None
            },
        )
    }

    fn extract_function_name_static(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        None
    }
}

impl Default for PythonParser {
    fn default() -> Self {
        Self::new().expect("Failed to create Python parser") // M-UNWRAP: tree-sitter language is a build-time invariant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_function() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"def foo():\n    pass\n";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("foo".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_extract_class() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"class MyClass:\n    pass\n";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyClass".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_function_in_class() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"
class MyClass:
    def method(self):
        pass
";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        // Should extract both class and method (flat structure)
        assert!(facts.len() >= 2);

        // Check for class
        let class: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(class.len(), 1);
        assert_eq!(class[0].name, Some("MyClass".to_string()));

        // Check for function
        let functions: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, Some("method".to_string()));
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"
def func_a():
    pass

class ClassA:
    pass

def func_b():
    pass
";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        assert!(facts.len() >= 3);

        let functions: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();
        assert_eq!(functions.len(), 2);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
    }

    #[test]
    fn test_empty_file() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"";
        let facts = parser.extract_symbols(PathBuf::from("empty.py"), source);

        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_syntax_error_returns_empty() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"def broken(\n    # invalid python";
        let facts = parser.extract_symbols(PathBuf::from("broken.py"), source);

        // Should handle gracefully - return empty (tree-sitter may still parse partial)
        // We don't crash
        assert!(
            facts.len() < 10,
            "Syntax error should not produce many symbols"
        );
    }

    #[test]
    fn test_byte_spans_within_bounds() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"def test(): pass";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        assert!(fact.byte_start < fact.byte_end);
        assert!(fact.byte_end <= source.len());
    }

    #[test]
    fn test_line_column_positions() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"def foo():\n    pass\n";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Function starts at line 1
        assert_eq!(fact.start_line, 1);
        assert_eq!(fact.start_col, 0); // 'd' in 'def' is at column 0
    }

    #[test]
    fn test_async_function() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"async def async_func():\n    pass\n";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("async_func".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_nested_classes() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"
class Outer:
    class Inner:
        pass
";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        // Should extract both classes (flat structure)
        assert!(facts.len() >= 2);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2);
    }

    #[test]
    fn test_decorated_function() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"
@decorator
def decorated_func():
    pass
";
        let _facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        // decorated_function_definition is the node kind for decorated functions
        // We may not extract it if we don't handle that node type
        // For now, this test documents current behavior
        // Note: Tree-sitter may extract decorated functions as function_definition
    }

    #[test]
    fn test_fqn_class_method() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"
class MyClass:
    def my_method(self):
        pass
";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].fqn, Some("MyClass.my_method".to_string()));
    }

    #[test]
    fn test_fqn_nested_classes() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"
class Outer:
    class Inner:
        def method(self):
            pass
";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].fqn, Some("Outer".to_string()));
        assert_eq!(classes[1].fqn, Some("Outer.Inner".to_string()));

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].fqn, Some("Outer.Inner.method".to_string()));
    }

    #[test]
    fn test_canonical_fqn_format() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"def my_function():\n    pass\n";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Canonical FQN format: package_name::file_path::Kind symbol_name
        assert!(fact.canonical_fqn.is_some());
        let canonical = fact.canonical_fqn.as_ref().unwrap();
        assert!(canonical.contains("::Function my_function"));
        assert!(canonical.contains("test.py"));
        assert!(canonical.starts_with("."));
    }

    #[test]
    fn test_display_fqn_format() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"def my_function():\n    pass\n";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Display FQN is human-readable, excludes file path
        assert!(fact.display_fqn.is_some());
        let display = fact.display_fqn.as_ref().unwrap();
        // Package "." with "." separator produces ".." prefix
        assert_eq!(display, "..my_function");
    }

    #[test]
    fn test_fqn_with_class() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"
class MyClass:
    def my_method(self):
        pass
";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();

        assert_eq!(classes.len(), 1);
        let class = classes[0];

        // Class canonical FQN includes file path
        assert!(class.canonical_fqn.is_some());
        assert!(class
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("::Struct MyClass"));

        // Class display FQN is package.class
        // Package "." with "." separator produces ".." prefix
        assert_eq!(class.display_fqn.as_ref().unwrap(), "..MyClass");

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(methods.len(), 1);
        let method = methods[0];

        // Method display FQN includes class scope
        assert_eq!(method.display_fqn.as_ref().unwrap(), "..MyClass.my_method");
    }

    #[test]
    fn test_fqn_nested_class_python() {
        let mut parser = PythonParser::new().unwrap();
        let source = b"
class Outer:
    class Inner:
        def nested_method(self):
            pass
";
        let facts = parser.extract_symbols(PathBuf::from("test.py"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();

        assert_eq!(classes.len(), 2);

        // Outer class display FQN
        // Package "." with "." separator produces ".." prefix
        assert_eq!(classes[0].display_fqn.as_ref().unwrap(), "..Outer");

        // Inner class display FQN includes outer class
        assert_eq!(classes[1].display_fqn.as_ref().unwrap(), "..Outer.Inner");

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Function)
            .collect();

        assert_eq!(methods.len(), 1);

        // Method display FQN includes full nesting
        assert_eq!(
            methods[0].display_fqn.as_ref().unwrap(),
            "..Outer.Inner.nested_method"
        );
    }
}
