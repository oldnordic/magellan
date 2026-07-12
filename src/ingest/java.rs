//! Java symbol extraction using tree-sitter-java.
//!
//! Extracts classes, interfaces, enums, methods, and packages from Java source code.

use crate::ingest::java_symbols::{build_symbol_facts_from_tree, extract_method_name};
use crate::ingest::SymbolFact;
use crate::references::{CallFact, ReferenceFact};
use anyhow::Result;
use std::path::PathBuf;

/// Parser that extracts symbol facts from Java source code.
///
/// Pure function: Input (path, contents) → Output `Vec<SymbolFact>`
/// No filesystem access. No global state. No caching.
pub struct JavaParser {
    pub(crate) parser: tree_sitter::Parser,
}

impl JavaParser {
    /// Create a new parser for Java source code.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into())?;
        Ok(Self { parser })
    }

    /// Create parser wrapper from an existing tree-sitter parser
    pub(crate) fn from_parser(parser: tree_sitter::Parser) -> Self {
        Self { parser }
    }

    /// Extract symbol facts from Java source code.
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
        build_symbol_facts_from_tree(&tree.root_node(), file_path, source)
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

        build_symbol_facts_from_tree(&tree.root_node(), file_path, source)
    }

    /// Extract symbol facts from a pre-parsed tree.
    ///
    /// Avoids re-parsing when the tree is already available.
    pub fn extract_symbols_from_tree(
        tree: &tree_sitter::Tree,
        file_path: PathBuf,
        source: &[u8],
    ) -> Vec<SymbolFact> {
        build_symbol_facts_from_tree(&tree.root_node(), file_path, source)
    }

    /// Extract reference facts from Java source code.
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
            |node| {
                matches!(
                    node.kind(),
                    "identifier" | "type_identifier" | "field_access" | "qualified_name"
                )
            },
            |node, source| {
                let text = std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).ok()?;
                Some((text.to_string(), node.kind()))
            },
        )
    }

    /// Extract function call facts from Java source code.
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
            |node| node.kind() == "method_declaration",
            extract_method_name,
            "method_invocation",
            |node, source| {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "identifier" | "field_access" | "qualified_name" => {
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
}

impl Default for JavaParser {
    fn default() -> Self {
        Self::new().expect("Failed to create Java parser") // M-UNWRAP: tree-sitter language is a build-time invariant
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::SymbolKind;

    #[test]
    fn test_extract_class() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"public class MyClass {\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("MyClass".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_extract_interface() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"interface MyInterface {\n    void method();\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        // Should extract interface and method (flat structure)
        assert!(!facts.is_empty());

        let interfaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Interface)
            .collect();
        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].name, Some("MyInterface".to_string()));
    }

    #[test]
    fn test_extract_enum() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"enum Color {\n    RED, GREEN, BLUE\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].name, Some("Color".to_string()));
        assert_eq!(facts[0].kind, SymbolKind::Enum);
    }

    #[test]
    fn test_extract_method() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"class MyClass {\n    void myMethod() {}\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        // Should extract class and method (flat structure)
        assert!(facts.len() >= 2);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, Some("myMethod".to_string()));
    }

    #[test]
    fn test_extract_package() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"package com.example;\n\nclass Foo {}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        // Should extract package and class
        assert!(!facts.is_empty());

        let modules: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, Some("com.example".to_string()));
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.test;

class MyClass {
    void method1() {}
}

interface MyInterface {
    void method2();
}

enum Color {
    RED
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        // Should extract: package, class, method1, interface, method2, enum
        assert!(facts.len() >= 6);

        let modules: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);

        let interfaces: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Interface)
            .collect();
        assert_eq!(interfaces.len(), 1);

        let enums: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Enum)
            .collect();
        assert_eq!(enums.len(), 1);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 2); // method1 and method2
    }

    #[test]
    fn test_empty_file() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"";
        let facts = parser.extract_symbols(PathBuf::from("empty.java"), source);

        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_syntax_error_returns_empty() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"class Broken {\n    // invalid java";
        let facts = parser.extract_symbols(PathBuf::from("broken.java"), source);

        // Should handle gracefully - return empty (tree-sitter may still parse partial)
        // We don't crash
        assert!(
            facts.len() < 10,
            "Syntax error should not produce many symbols"
        );
    }

    #[test]
    fn test_byte_spans_within_bounds() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"class Foo {}";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        assert!(fact.byte_start < fact.byte_end);
        assert!(fact.byte_end <= source.len());
    }

    #[test]
    fn test_line_column_positions() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"class Foo {\n}\n";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        assert_eq!(facts.len(), 1);
        let fact = &facts[0];

        // Class starts at line 1
        assert_eq!(fact.start_line, 1);
        assert_eq!(fact.start_col, 0); // 'c' in 'class' is at column 0
    }

    #[test]
    fn test_nested_class() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
class Outer {
    class Inner {
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        // Should extract both classes (flat structure)
        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].name, Some("Outer".to_string()));
        assert_eq!(classes[1].name, Some("Inner".to_string()));
    }

    #[test]
    fn test_fqn_package_class_method() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

public class MyClass {
    public void myMethod() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let modules: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Module)
            .collect();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].fqn, Some("com.example".to_string()));

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].fqn, Some("com.example.MyClass".to_string()));

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(
            methods[0].fqn,
            Some("com.example.MyClass.myMethod".to_string())
        );
    }

    #[test]
    fn test_fqn_nested_class() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
class Outer {
    class Inner {
        void method() {}
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].fqn, Some("Outer".to_string()));
        assert_eq!(classes[1].fqn, Some("Outer.Inner".to_string()));

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].fqn, Some("Outer.Inner.method".to_string()));
    }

    #[test]
    fn test_canonical_fqn_with_package() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

public class MyClass {
    public void myMethod() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("src/test/Example.java"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        // Canonical FQN format: crate_name::file_path::Kind symbol_name
        assert!(classes[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("src/test/Example.java"));
        assert!(classes[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("Struct"));
        assert!(classes[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("MyClass"));

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        assert!(methods[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("src/test/Example.java"));
        assert!(methods[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("Method"));
        assert!(methods[0]
            .canonical_fqn
            .as_ref()
            .unwrap()
            .contains("myMethod"));
    }

    #[test]
    fn test_display_fqn_with_package() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

public class MyClass {
    public void myMethod() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 1);
        // Display FQN format: package.class
        let display_fqn = classes[0].display_fqn.as_ref().unwrap();
        assert_eq!(display_fqn, "com.example.MyClass");

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);
        // Display FQN format: package.class.method
        let display_fqn = methods[0].display_fqn.as_ref().unwrap();
        assert_eq!(display_fqn, "com.example.MyClass.myMethod");
    }

    #[test]
    fn test_all_fqn_types_computed() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

public class MyClass {
    public void myMethod() {}
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);

        // Verify all three FQN types are computed
        assert!(methods[0].fqn.is_some());
        assert!(methods[0].canonical_fqn.is_some());
        assert!(methods[0].display_fqn.is_some());

        // Verify package name is included in display FQN
        assert!(methods[0]
            .display_fqn
            .as_ref()
            .unwrap()
            .starts_with("com.example"));
    }

    #[test]
    fn test_fqn_nested_class_with_package() {
        let mut parser = JavaParser::new().unwrap();
        let source = b"
package com.example;

class Outer {
    class Inner {
        void method() {}
    }
}
";
        let facts = parser.extract_symbols(PathBuf::from("test.java"), source);

        let classes: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Class)
            .collect();
        assert_eq!(classes.len(), 2);

        // Outer class display FQN
        assert_eq!(
            classes[0].display_fqn.as_ref().unwrap(),
            "com.example.Outer"
        );

        // Inner class display FQN (nested)
        assert_eq!(
            classes[1].display_fqn.as_ref().unwrap(),
            "com.example.Outer.Inner"
        );

        let methods: Vec<_> = facts
            .iter()
            .filter(|f| f.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 1);

        // Method display FQN in nested class
        assert_eq!(
            methods[0].display_fqn.as_ref().unwrap(),
            "com.example.Outer.Inner.method"
        );
    }
}
