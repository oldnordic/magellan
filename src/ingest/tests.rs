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
