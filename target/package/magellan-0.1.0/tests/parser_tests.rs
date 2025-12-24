use magellan::{Parser, SymbolFact, SymbolKind};
use std::path::PathBuf;

#[test]
fn test_empty_file() {
    let mut parser = Parser::new().unwrap();
    let source = b"";
    let facts = parser.extract_symbols(PathBuf::from("empty.rs"), source);

    assert_eq!(facts.len(), 0, "Empty file should produce no symbols");
}

#[test]
fn test_syntax_error_file() {
    let mut parser = Parser::new().unwrap();
    let source = b"fn broken { this is not valid rust }}}";
    let facts = parser.extract_symbols(PathBuf::from("broken.rs"), source);

    // Should handle gracefully - either empty or partial results
    // We don't crash
    assert!(facts.len() < 10, "Syntax error should not produce many symbols");
}

#[test]
fn test_single_function() {
    let mut parser = Parser::new().unwrap();
    let source = b"fn hello() {}";
    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    assert_eq!(facts.len(), 1, "Should extract one function");
    let fact = &facts[0];

    assert_eq!(fact.kind, SymbolKind::Function);
    assert_eq!(fact.name, Some("hello".to_string()));
    assert_eq!(fact.file_path, PathBuf::from("test.rs"));
}

#[test]
fn test_struct_definition() {
    let mut parser = Parser::new().unwrap();
    let source = b"struct MyStruct { field: i32 }";
    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    assert_eq!(facts.len(), 1, "Should extract one struct");
    let fact = &facts[0];

    assert_eq!(fact.kind, SymbolKind::Struct);
    assert_eq!(fact.name, Some("MyStruct".to_string()));
}

#[test]
fn test_enum_definition() {
    let mut parser = Parser::new().unwrap();
    let source = b"enum MyEnum { A, B }";
    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    assert_eq!(facts.len(), 1, "Should extract one enum");
    let fact = &facts[0];

    assert_eq!(fact.kind, SymbolKind::Enum);
    assert_eq!(fact.name, Some("MyEnum".to_string()));
}

#[test]
fn test_trait_definition() {
    let mut parser = Parser::new().unwrap();
    let source = b"trait MyTrait { fn method(&self); }";
    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    assert_eq!(facts.len(), 1, "Should extract one trait");
    let fact = &facts[0];

    assert_eq!(fact.kind, SymbolKind::Trait);
    assert_eq!(fact.name, Some("MyTrait".to_string()));
}

#[test]
fn test_module_declaration() {
    let mut parser = Parser::new().unwrap();
    let source = b"mod my_module;";
    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    assert_eq!(facts.len(), 1, "Should extract one module");
    let fact = &facts[0];

    assert_eq!(fact.kind, SymbolKind::Module);
    assert_eq!(fact.name, Some("my_module".to_string()));
}

#[test]
fn test_impl_block() {
    let mut parser = Parser::new().unwrap();
    let source = b"impl MyStruct { fn method(&self) {} }";
    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    // impl block + method inside
    assert!(
        facts.len() >= 1,
        "Should extract at least impl block or method"
    );

    // Find the method
    let methods: Vec<_> = facts
        .iter()
        .filter(|f| f.kind == SymbolKind::Unknown || f.kind == SymbolKind::Method)
        .collect();

    assert!(
        !methods.is_empty(),
        "Should find method inside impl block"
    );
}

#[test]
fn test_multiple_symbols() {
    let mut parser = Parser::new().unwrap();
    let source = b"
        fn func_a() {}
        struct StructA;
        enum EnumA { X }
        trait TraitA {}
        mod mod_a;
    ";

    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    // Should extract at least 5 symbols
    assert!(
        facts.len() >= 5,
        "Should extract at least 5 symbols, got {}",
        facts.len()
    );

    // Check each kind is present
    let kinds: Vec<_> = facts.iter().map(|f| &f.kind).collect();

    assert!(
        kinds.contains(&&SymbolKind::Function),
        "Should contain function"
    );
    assert!(
        kinds.contains(&&SymbolKind::Struct),
        "Should contain struct"
    );
    assert!(
        kinds.contains(&&SymbolKind::Enum),
        "Should contain enum"
    );
    assert!(
        kinds.contains(&&SymbolKind::Trait),
        "Should contain trait"
    );
    assert!(
        kinds.contains(&&SymbolKind::Module),
        "Should contain module"
    );
}

#[test]
fn test_nested_modules() {
    let mut parser = Parser::new().unwrap();
    let source = b"
        mod outer {
            mod inner {
                fn nested_fn() {}
            }
        }
    ";

    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    // Should extract: outer, inner, nested_fn (flat structure)
    assert!(
        facts.len() >= 3,
        "Should extract at least 3 symbols from nested modules"
    );

    // All should have file_path set
    for fact in &facts {
        assert_eq!(fact.file_path, PathBuf::from("test.rs"));
    }
}

#[test]
fn test_byte_spans() {
    let mut parser = Parser::new().unwrap();
    let source = b"fn test() {}";
    let facts = parser.extract_symbols(PathBuf::from("test.rs"), source);

    assert_eq!(facts.len(), 1);
    let fact = &facts[0];

    // Byte spans should be within source bounds
    assert!(fact.byte_start < fact.byte_end);
    assert!(fact.byte_end <= source.len());
}

#[test]
fn test_pure_function_same_input() {
    let mut parser = Parser::new().unwrap();
    let source = b"fn reproducible() {}";
    let path = PathBuf::from("test.rs");

    let facts1 = parser.extract_symbols(path.clone(), source);
    let facts2 = parser.extract_symbols(path, source);

    // Same input should produce identical output
    assert_eq!(facts1, facts2, "Parser should be pure and deterministic");
}
